//! `ai_draft_reply` orchestrator.
//!
//! Generates a reply draft for one message + optional user intent. The actual send happens
//! later via `smtp::send_draft` — this module never touches the network.
//!
//! Few-shot style anchor (5 recent sent messages, per SPEC Sprint 5) is deferred to
//! Sprint 6: at MVP we don't yet track sent items separately from `send_log`, and bootstrapping
//! the anchor table from `send_log` only catches messages this app sent. Worth the extra
//! complexity once the app has shipped enough mail to anchor on.

use secrecy::SecretString;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::ai::{extract_json, prompts, AiClient, CompletionRequest, SystemBlock, UserMessage};
use crate::db::ai_results::{self, AiResultInsert};
use crate::db::messages::MessageHeader;
use crate::db::{ai_role_defaults, bodies, messages, Pool};
use crate::error::{AppError, AppResult};
use crate::keychain;

const ROLE: &str = "draft";
const KIND: &str = "draft";
const MAX_OUTPUT_TOKENS: u32 = 2048;
const MAX_BODY_CHARS: usize = 20_000;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Draft {
    pub subject: String,
    pub body: String,
    pub tone: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DraftResult {
    #[serde(flatten)]
    pub draft: Draft,
    pub source: &'static str, // "fresh" | "cached"
    pub model: String,
    pub input_tokens: Option<i32>,
    pub output_tokens: Option<i32>,
    pub cache_read_tokens: Option<i32>,
}

/// `force=true` 时跳过 ai_results 缓存强制重新生成，并用新结果覆盖旧缓存。
pub async fn draft_reply(
    pool: &Pool,
    message_id: Uuid,
    intent: Option<&str>,
    force: bool,
) -> AppResult<DraftResult> {
    let model = ai_role_defaults::resolve_model(pool, ROLE)
        .await?
        .ok_or_else(|| {
            AppError::Config("请先在 AI 设置中配置默认起草模型 (role = draft)".into())
        })?;
    let model_uuid = model.id;
    let api_key: SecretString =
        tokio::task::spawn_blocking(move || keychain::get_ai_key(model_uuid))
            .await
            .map_err(|e| AppError::Other(anyhow::anyhow!(e)))??;

    let msg = messages::get(pool, message_id)
        .await?
        .ok_or_else(|| AppError::Config(format!("message {message_id} not found")))?;
    let body = bodies::get(pool, message_id).await?.ok_or_else(|| {
        AppError::Config(format!(
            "正文尚未加载，请先打开邮件 (message_id={message_id})"
        ))
    })?;

    let body_text = pick_body_for_draft(&body)?;
    let intent_text = intent.unwrap_or("").trim();
    let user_prompt = build_user_prompt(&msg, &body_text, intent_text);
    let prompt_hash = compute_prompt_hash(prompts::DRAFT_SYSTEM, intent_text, &user_prompt);

    // #71 缓存决策委托给纯函数 should_use_cache，便于单元测试。
    let cached_row = ai_results::get(pool, message_id, KIND, &prompt_hash).await?;
    if should_use_cache(force, cached_row.is_some()) {
        // 已由 should_use_cache 确认有缓存（cached_present=true），unwrap 安全。
        let cached = cached_row.expect("should_use_cache guarantees cached_row is Some");
        let draft: Draft = serde_json::from_value(cached.output)?;
        tracing::info!(message_id = %message_id, "draft cache hit");
        return Ok(DraftResult {
            draft,
            source: "cached",
            model: cached.model,
            input_tokens: cached.input_tokens,
            output_tokens: cached.output_tokens,
            cache_read_tokens: cached.cache_read_tokens,
        });
    }

    let client = AiClient::build(&model, api_key)?;
    let response = client
        .complete(CompletionRequest {
            max_tokens: MAX_OUTPUT_TOKENS,
            system: vec![SystemBlock {
                text: prompts::DRAFT_SYSTEM.to_string(),
                cache: true,
            }],
            messages: vec![UserMessage {
                content: user_prompt,
            }],
        })
        .await?;

    let draft: Draft = serde_json::from_str(extract_json(&response.text)).map_err(|e| {
        AppError::Ai(format!(
            "起草模型未返回合法 JSON：{e}\n原文：{}",
            truncate_for_log(&response.text)
        ))
    })?;

    let input_tokens = i32::try_from(response.usage.input_tokens).ok();
    let output_tokens = i32::try_from(response.usage.output_tokens).ok();
    let cache_read_tokens = response
        .usage
        .cache_read_input_tokens
        .and_then(|v| i32::try_from(v).ok());

    let stored = ai_results::insert(
        pool,
        &AiResultInsert {
            message_id,
            kind: KIND.into(),
            model: response.model.clone(),
            prompt_hash,
            output: serde_json::to_value(&draft)?,
            input_tokens,
            output_tokens,
            cache_read_tokens,
        },
    )
    .await?;

    tracing::info!(
        message_id = %message_id,
        tone = %draft.tone,
        input_tokens = ?stored.input_tokens,
        output_tokens = ?stored.output_tokens,
        "draft fresh result persisted"
    );

    Ok(DraftResult {
        draft,
        source: "fresh",
        model: stored.model,
        input_tokens: stored.input_tokens,
        output_tokens: stored.output_tokens,
        cache_read_tokens: stored.cache_read_tokens,
    })
}

fn pick_body_for_draft(body: &crate::db::bodies::MessageBody) -> AppResult<String> {
    let raw = body
        .text_plain
        .as_deref()
        .or(body.html.as_deref())
        .ok_or_else(|| AppError::Ai("邮件没有可参考的正文内容".into()))?;
    if raw.chars().count() <= MAX_BODY_CHARS {
        return Ok(raw.to_string());
    }
    let truncated: String = raw.chars().take(MAX_BODY_CHARS).collect();
    Ok(format!(
        "{truncated}\n\n[... 邮件已截断，仅参考前 {MAX_BODY_CHARS} 字符]"
    ))
}

fn build_user_prompt(msg: &MessageHeader, body: &str, intent: &str) -> String {
    let intent_section = if intent.is_empty() {
        String::from("（未指定回复意图，请按礼貌的默认回复处理）")
    } else {
        format!("我的回复意图：\n{intent}")
    };
    format!(
        "原邮件\n\n主题：{subject}\n发件人：{from}\n\n正文：\n{body}\n\n---\n\n{intent_section}",
        subject = msg.subject.as_deref().unwrap_or("(无主题)"),
        from = msg.from_addr.as_deref().unwrap_or("(无发件人)"),
        body = body,
        intent_section = intent_section,
    )
}

fn compute_prompt_hash(system: &str, intent: &str, user: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(system.as_bytes());
    hasher.update(b"\n---\n");
    hasher.update(intent.as_bytes());
    hasher.update(b"\n");
    hasher.update(user.as_bytes());
    hex::encode(hasher.finalize())
}

fn truncate_for_log(s: &str) -> String {
    let limit = 500;
    if s.chars().count() <= limit {
        s.to_string()
    } else {
        let cut: String = s.chars().take(limit).collect();
        format!("{cut}…")
    }
}

/// 缓存使用决策纯函数：`force=true` 时无论是否有缓存都绕过；否则有缓存才复用。
///
/// 将 `if !force` 内联条件提纯为独立函数，便于对缓存判定逻辑做直接单元测试。
fn should_use_cache(force: bool, cached_present: bool) -> bool {
    !force && cached_present
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompt_hash_changes_with_intent() {
        let a = compute_prompt_hash("sys", "", "user");
        let b = compute_prompt_hash("sys", "婉拒", "user");
        assert_ne!(a, b);
    }

    #[test]
    fn prompt_hash_changes_with_user_text() {
        let a = compute_prompt_hash("sys", "intent", "user1");
        let b = compute_prompt_hash("sys", "intent", "user2");
        assert_ne!(a, b);
    }

    /// A fenced ```json response (common from OpenAI-compatible vendors) must still
    /// deserialize into `Draft` once routed through `extract_json` — assert on the parsed
    /// struct, never on raw model text.
    #[test]
    fn fenced_response_parses_into_draft() {
        let raw = "```json\n{\"subject\":\"Re: 合作\",\"body\":\"好的\",\"tone\":\"polite\"}\n```";
        let draft: Draft = serde_json::from_str(extract_json(raw)).expect("should parse");
        assert_eq!(draft.subject, "Re: 合作");
        assert_eq!(draft.tone, "polite");
    }

    // I1：直接覆盖 should_use_cache 缓存判定纯函数的三个分支。
    #[test]
    fn cache_used_when_not_forced_and_present() {
        assert!(should_use_cache(false, true));
    }

    #[test]
    fn cache_skipped_when_forced_even_if_present() {
        assert!(!should_use_cache(true, true));
    }

    #[test]
    fn cache_skipped_when_not_forced_but_absent() {
        assert!(!should_use_cache(false, false));
    }
}

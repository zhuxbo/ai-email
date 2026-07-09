//! `ai_summarize` orchestrator.
//!
//! Flow:
//!   1. Resolve the user-configured "summary" model via `ai_role_defaults`. Bail with a
//!      clear "请先在 AI 设置中配置默认摘要模型" error if none is configured.
//!   2. Pull the API key for that model from the OS keychain (spawn_blocking).
//!   3. Materialize the whole thread via `load_thread_context` (guarantees the current body is
//!      in the DB), then take the current message's net increment (signature/quote/repeat
//!      stripped per filter rules) via `net_body_for` to save tokens.
//!   4. Compute `prompt_hash = sha256(system_prompt || \n---\n || user_prompt)`.
//!   5. `ai_results::get` by `(message_id, "summary", prompt_hash)` — return immediately
//!      on hit, with `source = "cached"`.
//!   6. Build the AiClient (Anthropic or OpenAI), call complete().
//!   7. Parse the JSON response. Fail loudly on malformed output.
//!   8. Persist to `ai_results`, return with `source = "fresh"` + usage breakdown.

use secrecy::SecretString;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::ai::{
    extract_json, prompts, safe_model_json_error, AiClient, CompletionRequest, SystemBlock,
    UserMessage,
};
use crate::db::ai_results::{self, AiResultInsert};
use crate::db::messages::MessageHeader;
use crate::db::{ai_role_defaults, Pool};
use crate::error::{AppError, AppResult};
use crate::keychain;

const ROLE: &str = "summary";
const MAX_OUTPUT_TOKENS: u32 = 1024;
/// Hard cap on body length sent to the model. ~50k Chinese chars ≈ ~80k tokens, well
/// within Sonnet's window but already expensive — long mail is rare and the bulk of the
/// signal is in the first few thousand characters.
const MAX_BODY_CHARS: usize = 50_000;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub tldr: String,
    pub bullets: Vec<String>,
    pub language: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SummaryResult {
    #[serde(flatten)]
    pub summary: Summary,
    /// "fresh" = new API call, "cached" = pulled from ai_results.
    pub source: &'static str,
    pub model: String,
    pub input_tokens: Option<i32>,
    pub output_tokens: Option<i32>,
    pub cache_read_tokens: Option<i32>,
}

pub async fn summarize_message(pool: &Pool, message_id: Uuid) -> AppResult<SummaryResult> {
    let model = ai_role_defaults::resolve_model(pool, ROLE)
        .await?
        .ok_or_else(|| {
            AppError::Config("请先在 AI 设置中配置默认摘要模型 (role = summary)".into())
        })?;
    let model_uuid = model.id;
    let api_key: SecretString =
        tokio::task::spawn_blocking(move || keychain::get_ai_key(model_uuid))
            .await
            .map_err(|e| AppError::Other(anyhow::anyhow!(e)))??;

    // Plan B：物化整条会话取净增量。删去旧"未缓存即 bail"守卫——load_thread_context 保证 body 入库。
    let ctx = crate::ai::context::load_thread_context(pool, message_id).await?;
    let current = ctx
        .members
        .get(ctx.current_index)
        .ok_or_else(|| AppError::Config(format!("message {message_id} not found")))?;
    let msg = current.header.clone();
    let body_text = net_body_for(pool, &ctx, "summary", MAX_BODY_CHARS).await?;
    let user_prompt = build_user_prompt(&msg, &body_text);
    let prompt_hash = compute_prompt_hash(prompts::SUMMARY_SYSTEM, &user_prompt);

    if let Some(cached) = ai_results::get(pool, message_id, "summary", &prompt_hash).await? {
        let summary: Summary = serde_json::from_value(cached.output)?;
        tracing::info!(message_id = %message_id, "summary cache hit");
        return Ok(SummaryResult {
            summary,
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
                text: prompts::SUMMARY_SYSTEM.to_string(),
                cache: true,
            }],
            messages: vec![UserMessage {
                content: user_prompt,
            }],
        })
        .await?;

    let summary: Summary = serde_json::from_str(extract_json(&response.text))
        .map_err(|e| safe_model_json_error("摘要模型", &e))?;

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
            kind: "summary".into(),
            model: response.model.clone(),
            prompt_hash,
            output: serde_json::to_value(&summary)?,
            input_tokens,
            output_tokens,
            cache_read_tokens,
        },
    )
    .await?;

    tracing::info!(
        message_id = %message_id,
        input_tokens = ?stored.input_tokens,
        output_tokens = ?stored.output_tokens,
        cache_read = ?stored.cache_read_tokens,
        "summary fresh result persisted"
    );

    Ok(SummaryResult {
        summary,
        source: "fresh",
        model: stored.model,
        input_tokens: stored.input_tokens,
        output_tokens: stored.output_tokens,
        cache_read_tokens: stored.cache_read_tokens,
    })
}

fn build_user_prompt(msg: &MessageHeader, body: &str) -> String {
    format!(
        "主题：{subject}\n发件人：{from}\n\n正文：\n{body}",
        subject = msg.subject.as_deref().unwrap_or("(无主题)"),
        from = msg.from_addr.as_deref().unwrap_or("(无发件人)"),
        body = body,
    )
}

fn compute_prompt_hash(system: &str, user: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(system.as_bytes());
    hasher.update(b"\n---\n");
    hasher.update(user.as_bytes());
    hex::encode(hasher.finalize())
}

/// 取当前封净增量并截断。filter_disabled=1 时跳过剥离用原文。无 thread 上下文时退化为当前封原文。
/// 抽出来供 summarize/draft/translate 共用，每个 role 能力默认不同（capability_defaults）。
pub(crate) async fn net_body_for(
    pool: &Pool,
    ctx: &crate::ai::context::ThreadContext,
    role: &str,
    max_chars: usize,
) -> AppResult<String> {
    use crate::ai::extract::{extract_increment, resolve_target_actions};

    let current = ctx
        .members
        .get(ctx.current_index)
        .ok_or_else(|| AppError::Ai("会话上下文缺当前封".into()))?;
    // 当前封原文（text_plain 优先，回退 html）。
    let raw = current
        .text_plain
        .clone()
        .or_else(|| current.html.clone())
        .ok_or_else(|| AppError::Ai("邮件没有可处理的正文内容".into()))?;

    // filter_disabled 直接读 header（Task 6 已给 MessageHeader 加该字段并贯通 SELECT）——
    // 不再 messages::get 重查该列，省一次 DB roundtrip。
    let disabled = current.header.filter_disabled;

    let net = if disabled {
        raw
    } else {
        // 前序封只取 text_plain：仅-HTML 的前序封映射为 None，符合 extract_increment 契约
        //（§2.6：物化失败/仅-HTML 的前序项为 None，行/句匹配退启发式）。前序 HTML→text 回退留后期。
        let prior: Vec<Option<String>> = ctx.members[..ctx.current_index]
            .iter()
            .map(|m| m.text_plain.clone())
            .collect();
        let sender = current.header.from_addr.as_deref();
        let resolved = crate::db::filter_rules::resolve_for(pool, sender).await?;
        let actions = resolve_target_actions(
            role,
            resolved.signature,
            resolved.quote,
            resolved.repeat,
            resolved.signature_pattern.as_deref(),
        );
        let result = extract_increment(&raw, &prior, current.is_own, &actions);
        // 净增量为空（全剥）是合法的——退回原文避免喂空 prompt。
        if result.net.trim().is_empty() {
            raw
        } else {
            result.net
        }
    };

    if net.chars().count() <= max_chars {
        Ok(net)
    } else {
        let truncated: String = net.chars().take(max_chars).collect();
        Ok(format!(
            "{truncated}\n\n[... 已截断，仅取前 {max_chars} 字符]"
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompt_hash_is_stable() {
        let a = compute_prompt_hash("sys", "user");
        let b = compute_prompt_hash("sys", "user");
        assert_eq!(a, b);
        assert_eq!(a.len(), 64, "sha256 hex must be 64 chars");
    }

    #[test]
    fn prompt_hash_changes_with_input() {
        let base = compute_prompt_hash("sys", "user");
        assert_ne!(base, compute_prompt_hash("sys", "user "));
        assert_ne!(base, compute_prompt_hash("sys2", "user"));
    }

    #[test]
    fn prompt_hash_separator_prevents_collision() {
        // Without the "\n---\n" separator, ("foo", "bar") and ("foob", "ar") would hash
        // identically. Confirm the separator does its job.
        assert_ne!(
            compute_prompt_hash("foo", "bar"),
            compute_prompt_hash("foob", "ar"),
        );
    }

    /// A fenced ```json response must still deserialize into `Summary` via `extract_json`.
    /// Structural assertion on the parsed value, not on raw text.
    #[test]
    fn fenced_response_parses_into_summary() {
        let raw = "```json\n{\"tldr\":\"摘要\",\"bullets\":[\"a\",\"b\"],\"language\":\"zh\"}\n```";
        let summary: Summary = serde_json::from_str(extract_json(raw)).expect("should parse");
        assert_eq!(summary.tldr, "摘要");
        assert_eq!(summary.bullets.len(), 2);
    }
}

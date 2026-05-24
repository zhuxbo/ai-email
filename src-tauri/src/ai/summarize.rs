//! `ai_summarize` orchestrator.
//!
//! Flow:
//!   1. Resolve the user-configured "summary" model via `ai_role_defaults`. Bail with a
//!      clear "请先在 AI 设置中配置默认摘要模型" error if none is configured.
//!   2. Pull the API key for that model from the OS keychain (spawn_blocking).
//!   3. Load message header + cached body from PG. Bail if the body isn't cached — we want
//!      the user to open the message first so the lazy-fetch lands the body.
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

use crate::ai::{prompts, AiClient, CompletionRequest, SystemBlock, UserMessage};
use crate::db::ai_results::{self, AiResultInsert};
use crate::db::messages::MessageHeader;
use crate::db::{ai_role_defaults, bodies, messages, Pool};
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

    let msg = messages::get(pool, message_id)
        .await?
        .ok_or_else(|| AppError::Config(format!("message {message_id} not found")))?;
    let body = bodies::get(pool, message_id).await?.ok_or_else(|| {
        AppError::Config(format!(
            "正文尚未加载，请先打开邮件 (message_id={message_id})"
        ))
    })?;

    let body_text = pick_body_for_summary(&body)?;
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

    let summary: Summary = serde_json::from_str(&response.text).map_err(|e| {
        AppError::Ai(format!(
            "模型未返回合法 JSON：{e}\n原文：{}",
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

fn pick_body_for_summary(body: &crate::db::bodies::MessageBody) -> AppResult<String> {
    // Prefer text/plain — it's already the model's preferred input. HTML fallback is a
    // last resort for messages with no plain alternative (Sprint 3+ may strip HTML tags
    // server-side; for now we just send the raw HTML and let the model handle it).
    let raw = body
        .text_plain
        .as_deref()
        .or(body.html.as_deref())
        .ok_or_else(|| AppError::Ai("邮件没有可摘要的正文内容".into()))?;
    if raw.chars().count() <= MAX_BODY_CHARS {
        return Ok(raw.to_string());
    }
    let truncated: String = raw.chars().take(MAX_BODY_CHARS).collect();
    Ok(format!(
        "{truncated}\n\n[... 邮件已截断，仅摘要前 {MAX_BODY_CHARS} 字符]"
    ))
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

fn truncate_for_log(s: &str) -> String {
    let limit = 500;
    if s.chars().count() <= limit {
        s.to_string()
    } else {
        let cut: String = s.chars().take(limit).collect();
        format!("{cut}…")
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
}

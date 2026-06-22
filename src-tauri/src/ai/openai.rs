//! OpenAI-compatible `/v1/chat/completions` provider.
//!
//! Covers OpenAI itself plus every domestic vendor that ships an OpenAI-compatible endpoint —
//! DeepSeek, 智谱 GLM, Moonshot Kimi, 通义 Qwen, 字节豆包, Groq, Together, etc. The only
//! per-vendor difference is `base_url` and the `model_id` they expect.
//!
//! Differences vs Anthropic provider:
//!   • System prompts are sent as `messages[0]` with `role: "system"` — multiple system
//!     blocks are concatenated with "\n\n".
//!   • No `cache_control` field; the OpenAI server auto-caches long prefixes (≥1024 tokens
//!     on first-party OpenAI; most others have no caching). We surface `cache_read_tokens`
//!     from `usage.prompt_tokens_details.cached_tokens` when present.

use secrecy::{ExposeSecret, SecretString};
use serde_json::json;

use crate::ai::{CompletionRequest, CompletionResponse, Usage};
use crate::error::{AppError, AppResult};

pub const DEFAULT_BASE_URL: &str = "https://api.openai.com";

pub struct OpenAIProvider {
    http: reqwest::Client,
    api_key: SecretString,
    base_url: String,
    model_id: String,
}

impl OpenAIProvider {
    pub fn new(
        http: reqwest::Client,
        api_key: SecretString,
        base_url: String,
        model_id: String,
    ) -> Self {
        Self {
            http,
            api_key,
            base_url,
            model_id,
        }
    }

    pub async fn complete(&self, req: CompletionRequest) -> AppResult<CompletionResponse> {
        let system_text: String = req
            .system
            .iter()
            .map(|s| s.text.as_str())
            .collect::<Vec<_>>()
            .join("\n\n");

        let mut messages: Vec<serde_json::Value> = Vec::with_capacity(req.messages.len() + 1);
        if !system_text.is_empty() {
            messages.push(json!({ "role": "system", "content": system_text }));
        }
        for m in &req.messages {
            messages.push(json!({ "role": "user", "content": m.content }));
        }

        let body = json!({
            "model": self.model_id,
            "max_tokens": req.max_tokens,
            "messages": messages,
        });

        let resp = self
            .http
            .post(format!("{}/v1/chat/completions", self.base_url))
            .bearer_auth(self.api_key.expose_secret())
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await?;

        // Read the status before touching the body so a non-JSON error page (gateway 502/503
        // HTML, captive portal, rate-limit text/plain) surfaces as `HTTP {status}: …` instead
        // of being swallowed by a generic decode error.
        let status = resp.status();
        let bytes = resp.bytes().await?;
        parse_completion(status, &bytes, &self.model_id)
    }
}

/// Turn the raw HTTP status + body bytes into a [`CompletionResponse`].
///
/// Split out from `complete` so the status/JSON/truncation/usage logic is unit-testable
/// without hitting the network (per the no-real-network test rule).
fn parse_completion(
    status: reqwest::StatusCode,
    bytes: &[u8],
    fallback_model: &str,
) -> AppResult<CompletionResponse> {
    // Parse from slice; on non-2xx fall back to the raw body so we never lose the upstream
    // error text even when it isn't JSON.
    let payload: serde_json::Value =
        serde_json::from_slice(bytes).unwrap_or(serde_json::Value::Null);

    if !status.is_success() {
        return Err(AppError::Ai(format!(
            "HTTP {status}: {}",
            error_message(&payload, bytes)
        )));
    }

    // A "length" finish reason means generation hit max_tokens and `content` is a truncated
    // fragment. Report it explicitly so the caller doesn't see a vague parse failure later.
    if payload["choices"][0]["finish_reason"].as_str() == Some("length") {
        return Err(AppError::Ai(
            "AI 响应被 max_tokens 截断，请调高 max_tokens 或缩短输入".into(),
        ));
    }

    let text = payload["choices"][0]["message"]["content"]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| AppError::Ai("response had no choices[0].message.content".into()))?
        .to_string();

    let model = payload["model"]
        .as_str()
        .unwrap_or(fallback_model)
        .to_string();

    let usage = parse_usage(&payload["usage"]);

    tracing::info!(
        provider = "openai",
        model = %model,
        input_tokens = usage.input_tokens,
        output_tokens = usage.output_tokens,
        cache_read = ?usage.cache_read_input_tokens,
        "completion done"
    );

    Ok(CompletionResponse { text, model, usage })
}

/// Best-effort error message from a non-2xx body. Fallback chain covers the spread of
/// (non-)standard vendor error shapes:
///   1. `{ error: { message } }`            — standard OpenAI
///   2. `{ error: "..." }`                  — `error` as a bare string (some compatible servers)
///   3. `{ message: "...", code: 401 }`     — error flattened at top level, no `error` wrapper
///   4. raw body snippet                    — anything else (HTML gateway page, plain text)
fn error_message(payload: &serde_json::Value, bytes: &[u8]) -> String {
    payload["error"]["message"]
        .as_str()
        .or_else(|| payload["error"].as_str())
        .or_else(|| payload["message"].as_str())
        .map(str::to_string)
        .unwrap_or_else(|| body_snippet(bytes))
}

/// A short, lossy-UTF-8 snippet of the raw body for error messages — used when a non-2xx
/// response has no parseable error message (HTML gateway pages, plain-text rate limits).
fn body_snippet(bytes: &[u8]) -> String {
    const LIMIT: usize = 500;
    let text = String::from_utf8_lossy(bytes);
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return "(empty response body)".to_string();
    }
    if trimmed.chars().count() <= LIMIT {
        trimmed.to_string()
    } else {
        let cut: String = trimmed.chars().take(LIMIT).collect();
        format!("{cut}…")
    }
}

/// OpenAI's usage shape:
///   { "prompt_tokens": N, "completion_tokens": M,
///     "prompt_tokens_details": { "cached_tokens": K } }
/// Domestic vendors mostly skip `prompt_tokens_details`; we just leave the cache field None.
fn parse_usage(v: &serde_json::Value) -> Usage {
    Usage {
        input_tokens: read_u32(&v["prompt_tokens"]).unwrap_or(0),
        output_tokens: read_u32(&v["completion_tokens"]).unwrap_or(0),
        cache_creation_input_tokens: None,
        cache_read_input_tokens: read_u32(&v["prompt_tokens_details"]["cached_tokens"]),
    }
}

/// Read a token count that may be a JSON integer or float (some compatible vendors serialize
/// counts as `1234.0`), saturating into u32 — overflow clamps to `u32::MAX` rather than
/// wrapping (the old `as u32` truncated the low 32 bits). `None` when absent / not a number.
fn read_u32(v: &serde_json::Value) -> Option<u32> {
    v.as_u64()
        .or_else(|| v.as_f64().map(|f| f as u64))
        .map(|n| u32::try_from(n).unwrap_or(u32::MAX))
}

#[cfg(test)]
mod tests {
    use super::*;
    use reqwest::StatusCode;

    const OK: StatusCode = StatusCode::OK;

    fn err_string(r: AppResult<CompletionResponse>) -> String {
        match r {
            Err(AppError::Ai(m)) => m,
            other => panic!("expected AppError::Ai, got {other:?}"),
        }
    }

    // ── #4: non-2xx with non-JSON body keeps status + raw text ──────────────────

    #[test]
    fn non_json_error_body_reports_status_and_snippet() {
        let html = b"<html>503 upstream timeout</html>";
        let msg = err_string(parse_completion(StatusCode::SERVICE_UNAVAILABLE, html, "m"));
        assert!(msg.contains("503"), "status preserved: {msg}");
        assert!(
            msg.contains("upstream timeout"),
            "raw body preserved: {msg}"
        );
    }

    #[test]
    fn empty_error_body_does_not_panic() {
        let msg = err_string(parse_completion(StatusCode::BAD_GATEWAY, b"", "m"));
        assert!(msg.contains("502"), "status: {msg}");
    }

    // ── #58: error message fallback chain ───────────────────────────────────────

    #[test]
    fn standard_error_object_message() {
        let body = br#"{"error":{"message":"rate limit exceeded","type":"rate_limit"}}"#;
        let msg = err_string(parse_completion(StatusCode::TOO_MANY_REQUESTS, body, "m"));
        assert!(msg.contains("rate limit exceeded"), "{msg}");
    }

    #[test]
    fn error_as_bare_string() {
        let body = br#"{"error":"invalid token"}"#;
        let msg = err_string(parse_completion(StatusCode::UNAUTHORIZED, body, "m"));
        assert!(msg.contains("invalid token"), "{msg}");
    }

    #[test]
    fn top_level_message_without_error_wrapper() {
        // Non-standard vendor: error flattened at top level with no `error` key. The raw
        // cause must survive instead of degrading to "(no message)".
        let body = br#"{"message":"api key disabled","code":401}"#;
        let msg = err_string(parse_completion(StatusCode::UNAUTHORIZED, body, "m"));
        assert!(
            msg.contains("api key disabled"),
            "top-level message kept: {msg}"
        );
    }

    // ── #28: finish_reason == "length" → explicit truncation error ──────────────

    #[test]
    fn length_finish_reason_is_explicit_truncation_error() {
        // Truncated but non-empty content must NOT be returned as success.
        let body = br#"{"choices":[{"message":{"content":"{\"subj"},"finish_reason":"length"}],"model":"gpt","usage":{}}"#;
        let msg = err_string(parse_completion(OK, body, "m"));
        assert!(msg.contains("max_tokens"), "mentions cause: {msg}");
    }

    #[test]
    fn stop_finish_reason_parses_normally() {
        let body = br#"{"choices":[{"message":{"content":"hi"},"finish_reason":"stop"}],"model":"gpt","usage":{"prompt_tokens":7,"completion_tokens":1}}"#;
        let resp = parse_completion(OK, body, "m").expect("should succeed");
        assert_eq!(resp.text, "hi");
        assert_eq!(resp.usage.input_tokens, 7);
    }

    // ── #60 / #61: usage saturation + float tolerance ───────────────────────────

    #[test]
    fn usage_overflow_saturates_to_u32_max() {
        // 5e9 > u32::MAX: must clamp, not wrap to a misleading small value.
        let v = serde_json::json!({ "prompt_tokens": 5_000_000_000u64, "completion_tokens": 3 });
        let usage = parse_usage(&v);
        assert_eq!(usage.input_tokens, u32::MAX, "saturates not wraps");
        assert_eq!(usage.output_tokens, 3);
    }

    #[test]
    fn usage_float_field_is_parsed_not_zeroed() {
        // A vendor serializing counts as floats (1234.0) used to read as None → 0.
        let v = serde_json::json!({ "prompt_tokens": 1234.0, "completion_tokens": 56.0 });
        let usage = parse_usage(&v);
        assert_eq!(usage.input_tokens, 1234);
        assert_eq!(usage.output_tokens, 56);
    }

    #[test]
    fn usage_cached_tokens_when_present() {
        let v = serde_json::json!({
            "prompt_tokens": 100,
            "completion_tokens": 20,
            "prompt_tokens_details": { "cached_tokens": 40 }
        });
        let usage = parse_usage(&v);
        assert_eq!(usage.cache_read_input_tokens, Some(40));
    }
}

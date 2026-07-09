//! Anthropic `/v1/messages` provider.
//!
//! Honors `cache_control: { type: "ephemeral" }` on system blocks marked `cache = true`.
//! For OpenAI-compatible providers see `crate::ai::openai`.

use secrecy::{ExposeSecret, SecretString};
use serde_json::json;

use crate::ai::{CompletionRequest, CompletionResponse, Usage};
use crate::error::{AppError, AppResult};

pub const DEFAULT_BASE_URL: &str = "https://api.anthropic.com";

pub struct AnthropicProvider {
    http: reqwest::Client,
    api_key: SecretString,
    base_url: String,
    model_id: String,
}

impl AnthropicProvider {
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
        let system_blocks: Vec<_> = req
            .system
            .iter()
            .map(|s| {
                let mut block = json!({ "type": "text", "text": s.text });
                if s.cache {
                    block["cache_control"] = json!({ "type": "ephemeral" });
                }
                block
            })
            .collect();
        let messages: Vec<_> = req
            .messages
            .iter()
            .map(|m| json!({ "role": "user", "content": m.content }))
            .collect();

        let body = json!({
            "model": self.model_id,
            "max_tokens": req.max_tokens,
            "system": system_blocks,
            "messages": messages,
        });

        let resp = self
            .http
            .post(format!("{}/v1/messages", self.base_url))
            .header("x-api-key", self.api_key.expose_secret())
            .header("anthropic-version", "2023-06-01")
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
    // Parse from slice so success handling can inspect JSON. Non-2xx responses intentionally
    // return only status/category to the frontend; raw provider text may contain secrets.
    let payload: serde_json::Value =
        serde_json::from_slice(bytes).unwrap_or(serde_json::Value::Null);

    if !status.is_success() {
        return Err(AppError::Ai(format!(
            "AI provider request failed (HTTP {status})"
        )));
    }

    // A `max_tokens` stop means the body (often JSON) is truncated mid-output. Report it
    // explicitly so the caller doesn't see a vague "未返回合法 JSON" downstream.
    if payload["stop_reason"].as_str() == Some("max_tokens") {
        return Err(AppError::Ai(
            "AI 响应被 max_tokens 截断，请调高 max_tokens 或缩短输入".into(),
        ));
    }

    let text = payload["content"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|b| b["text"].as_str())
                .collect::<Vec<_>>()
                .join("")
        })
        .filter(|s| !s.is_empty())
        .ok_or_else(|| AppError::Ai("response had no text content".into()))?;

    let model = payload["model"]
        .as_str()
        .unwrap_or(fallback_model)
        .to_string();
    let usage = parse_usage(&payload["usage"]);

    tracing::info!(
        provider = "anthropic",
        model = %model,
        input_tokens = usage.input_tokens,
        output_tokens = usage.output_tokens,
        cache_creation = ?usage.cache_creation_input_tokens,
        cache_read = ?usage.cache_read_input_tokens,
        "completion done"
    );

    Ok(CompletionResponse { text, model, usage })
}

/// Anthropic usage shape:
///   { "input_tokens": N, "output_tokens": M,
///     "cache_creation_input_tokens": K, "cache_read_input_tokens": J }
///
/// Parsed field-by-field so one out-of-range / wrong-typed field can't zero out the whole
/// `Usage` (the old `from_value(..).unwrap_or_default()` was all-or-nothing). Values are
/// read as u64 then saturated into u32 — overflow clamps to `u32::MAX` rather than wrapping.
fn parse_usage(v: &serde_json::Value) -> Usage {
    Usage {
        input_tokens: read_u32(&v["input_tokens"]).unwrap_or(0),
        output_tokens: read_u32(&v["output_tokens"]).unwrap_or(0),
        cache_creation_input_tokens: read_u32(&v["cache_creation_input_tokens"]),
        cache_read_input_tokens: read_u32(&v["cache_read_input_tokens"]),
    }
}

/// Read a token count that may be a JSON integer or float, saturating into u32.
/// Returns `None` when the field is absent or not a number.
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

    // ── provider errors are user-safe: keep status/category, do not surface raw body ──

    #[test]
    fn non_json_error_body_reports_status_without_raw_snippet() {
        let html = b"<html><body>gateway-secret-body</body></html>";
        let msg = err_string(parse_completion(StatusCode::BAD_GATEWAY, html, "m"));
        assert!(msg.contains("502"), "status preserved: {msg}");
        assert!(
            !msg.contains("gateway-secret-body"),
            "raw body hidden: {msg}"
        );
        assert!(msg.contains("provider"), "safe category present: {msg}");
    }

    #[test]
    fn json_error_body_keeps_status_without_raw_message() {
        let body = br#"{"error":{"message":"invalid x-api-key","type":"authentication_error"}}"#;
        let msg = err_string(parse_completion(StatusCode::UNAUTHORIZED, body, "m"));
        assert!(msg.contains("401"), "status: {msg}");
        assert!(
            !msg.contains("invalid x-api-key"),
            "raw provider message hidden: {msg}"
        );
    }

    #[test]
    fn empty_error_body_does_not_panic() {
        let msg = err_string(parse_completion(StatusCode::SERVICE_UNAVAILABLE, b"", "m"));
        assert!(msg.contains("503"), "status: {msg}");
    }

    // ── #31: stop_reason == "max_tokens" → explicit truncation error ────────────

    #[test]
    fn max_tokens_stop_reason_is_explicit_truncation_error() {
        // Even with valid-looking (truncated) content, a max_tokens stop must surface as a
        // dedicated error rather than parsing through to the orchestrator.
        let body = br#"{"content":[{"type":"text","text":"{\"a\":"}],"stop_reason":"max_tokens","model":"claude","usage":{"input_tokens":10,"output_tokens":2048}}"#;
        let msg = err_string(parse_completion(OK, body, "m"));
        assert!(msg.contains("max_tokens"), "mentions cause: {msg}");
    }

    #[test]
    fn end_turn_stop_reason_parses_normally() {
        let body = br#"{"content":[{"type":"text","text":"hello"}],"stop_reason":"end_turn","model":"claude","usage":{"input_tokens":10,"output_tokens":5}}"#;
        let resp = parse_completion(OK, body, "m").expect("should succeed");
        assert_eq!(resp.text, "hello");
        assert_eq!(resp.usage.output_tokens, 5);
    }

    // ── #62: usage parsed field-by-field, one bad field can't zero the rest ─────

    #[test]
    fn usage_in_range_float_field_still_parses() {
        // Regression for the old `from_value(..).unwrap_or_default()` all-or-nothing path:
        // a float-typed field (200.0) used to fail u32 deserialization and zero EVERY field.
        let v = serde_json::json!({
            "input_tokens": 100,
            "output_tokens": 200.0,
            "cache_read_input_tokens": 50
        });
        let usage = parse_usage(&v);
        assert_eq!(
            usage.input_tokens, 100,
            "sibling field survives bad neighbor"
        );
        assert_eq!(usage.output_tokens, 200);
        assert_eq!(usage.cache_read_input_tokens, Some(50));
    }

    #[test]
    fn usage_overflow_saturates_to_u32_max() {
        let v = serde_json::json!({ "input_tokens": 5_000_000_000u64, "output_tokens": 3 });
        let usage = parse_usage(&v);
        assert_eq!(
            usage.input_tokens,
            u32::MAX,
            "overflow saturates, not wraps"
        );
        assert_eq!(usage.output_tokens, 3);
    }

    #[test]
    fn usage_missing_fields_default_to_zero_and_none() {
        let usage = parse_usage(&serde_json::json!({}));
        assert_eq!(usage.input_tokens, 0);
        assert_eq!(usage.output_tokens, 0);
        assert_eq!(usage.cache_creation_input_tokens, None);
        assert_eq!(usage.cache_read_input_tokens, None);
    }
}

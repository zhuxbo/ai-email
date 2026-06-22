//! AI provider abstraction.
//!
//! Two providers are wired at MVP:
//!   • [`anthropic`] — native `/v1/messages`, ephemeral prompt cache aware.
//!   • [`openai`]    — `/v1/chat/completions`. Covers OpenAI, DeepSeek, 智谱 GLM,
//!                    Moonshot Kimi, 通义 Qwen, 字节豆包, Groq, Together, etc. via
//!                    `base_url` override.
//!
//! Dispatch is an `enum AiClient`, not `dyn AiProvider` — closed set, no async-trait dance.
//! Build one with [`AiClient::build`] from a stored `AiModel` row + a SecretString key.
//!
//! The shared `CompletionRequest` / `CompletionResponse` / `Usage` types live here so
//! provider modules and call-site code agree on shape.

pub mod anthropic;
pub mod classify;
pub mod draft;
pub mod openai;
pub mod prompts;
pub mod summarize;
pub mod translate;

use std::time::Duration;

use secrecy::SecretString;
use serde::Deserialize;

use crate::db::ai_models::AiModel;
use crate::error::{AppError, AppResult};

// ── Shared request / response shape ──────────────────────────────────────────

/// One system-prompt segment. `cache = true` is honored by [`anthropic`] (adds
/// `cache_control: { type: "ephemeral" }`) and silently ignored by [`openai`] —
/// OpenAI handles prompt caching automatically; domestic vendors mostly don't cache.
pub struct SystemBlock {
    pub text: String,
    pub cache: bool,
}

pub struct UserMessage {
    pub content: String,
}

pub struct CompletionRequest {
    pub max_tokens: u32,
    pub system: Vec<SystemBlock>,
    pub messages: Vec<UserMessage>,
}

#[derive(Debug, Clone)]
pub struct CompletionResponse {
    pub text: String,
    pub model: String,
    pub usage: Usage,
}

/// Provider-agnostic usage breakdown. Each provider fills the fields it tracks; missing
/// values surface as `0` for the required tokens and `None` for the cache fields.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Usage {
    #[serde(default)]
    pub input_tokens: u32,
    #[serde(default)]
    pub output_tokens: u32,
    #[serde(default)]
    pub cache_creation_input_tokens: Option<u32>,
    #[serde(default)]
    pub cache_read_input_tokens: Option<u32>,
}

// ── Response JSON extraction ────────────────────────────────────────────────────

/// Strip markdown fences / chatter around a JSON payload so `serde_json::from_str`
/// doesn't hard-fail on the common ```json … ``` wrapper or a leading preamble.
///
/// OpenAI-compatible vendors frequently wrap structured output in a fenced code block
/// or prepend a sentence ("Here is the JSON:"). We don't fully parse — we just narrow
/// to the most likely JSON span and let `serde_json` do the real validation:
///   1. `trim()`.
///   2. If it opens with a ``` fence, drop the opening fence line (optionally carrying a
///      language tag like ```json) and the trailing fence, then trim again.
///   3. Otherwise, slice from the first `{`/`[` to the last `}`/`]` (whichever pair sits
///      furthest out), dropping any prose before/after.
///
/// If no brace/bracket is found, returns the trimmed input unchanged so the caller's
/// `serde_json` error still surfaces the real text.
pub(crate) fn extract_json(raw: &str) -> &str {
    let trimmed = raw.trim();

    if let Some(rest) = trimmed.strip_prefix("```") {
        // Opening fence: skip the remainder of that first line (covers ```json, ```JSON,
        // bare ```), then drop a trailing ``` fence if present.
        let after_lang = match rest.find('\n') {
            Some(nl) => &rest[nl + 1..],
            None => "", // fence with nothing after it
        };
        let inner = after_lang
            .trim_end()
            .strip_suffix("```")
            .unwrap_or(after_lang);
        return inner.trim();
    }

    let start = trimmed.find(['{', '[']).unwrap_or(0);
    let end = trimmed
        .rfind(['}', ']'])
        .map(|i| i + 1)
        .unwrap_or(trimmed.len());

    if start < end {
        &trimmed[start..end]
    } else {
        trimmed
    }
}

// ── Dispatch ──────────────────────────────────────────────────────────────────

pub enum AiClient {
    Anthropic(anthropic::AnthropicProvider),
    OpenAI(openai::OpenAIProvider),
}

impl AiClient {
    /// Build a fresh client for an `AiModel` row. The reqwest client inside is also fresh
    /// — at MVP call rate (single-digit calls per minute) the pool reuse savings are
    /// negligible and a fresh client picks up any rotated keys/proxies immediately.
    pub fn build(model: &AiModel, api_key: SecretString) -> AppResult<Self> {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(120))
            .user_agent("ai-email/0.1.0 (+https://github.com/zhuxbo/ai-email)")
            .build()
            .map_err(|e| AppError::Ai(format!("failed to build http client: {e}")))?;

        match model.provider.as_str() {
            "anthropic" => {
                let base_url = model
                    .base_url
                    .clone()
                    .unwrap_or_else(|| anthropic::DEFAULT_BASE_URL.to_string());
                Ok(Self::Anthropic(anthropic::AnthropicProvider::new(
                    http,
                    api_key,
                    base_url,
                    model.model_id.clone(),
                )))
            }
            "openai" => {
                let base_url = model
                    .base_url
                    .clone()
                    .unwrap_or_else(|| openai::DEFAULT_BASE_URL.to_string());
                Ok(Self::OpenAI(openai::OpenAIProvider::new(
                    http,
                    api_key,
                    base_url,
                    model.model_id.clone(),
                )))
            }
            other => Err(AppError::Config(format!(
                "unknown ai provider: {other} (expected 'anthropic' or 'openai')"
            ))),
        }
    }

    pub async fn complete(&self, req: CompletionRequest) -> AppResult<CompletionResponse> {
        match self {
            Self::Anthropic(p) => p.complete(req).await,
            Self::OpenAI(p) => p.complete(req).await,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::extract_json;
    use serde_json::Value;

    /// The extracted span must be valid JSON of the expected shape. We assert structurally
    /// (parse + field check), never on raw model text.
    fn parse(raw: &str) -> Value {
        serde_json::from_str(extract_json(raw)).expect("extract_json output should parse as JSON")
    }

    #[test]
    fn passes_through_bare_object() {
        let v = parse(r#"{"category":"work","priority":1}"#);
        assert_eq!(v["category"], "work");
    }

    #[test]
    fn passes_through_bare_array() {
        let v = parse(r#"[{"id":1},{"id":2}]"#);
        assert_eq!(v.as_array().unwrap().len(), 2);
    }

    #[test]
    fn strips_surrounding_whitespace() {
        let v = parse("  \n\t{\"ok\":true}\n  ");
        assert_eq!(v["ok"], true);
    }

    #[test]
    fn strips_json_lang_fence() {
        let raw = "```json\n{\"tldr\":\"hi\",\"bullets\":[]}\n```";
        let v = parse(raw);
        assert_eq!(v["tldr"], "hi");
    }

    #[test]
    fn strips_uppercase_lang_fence() {
        let raw = "```JSON\n[1,2,3]\n```";
        assert_eq!(parse(raw).as_array().unwrap().len(), 3);
    }

    #[test]
    fn strips_bare_fence_without_lang_tag() {
        let raw = "```\n{\"a\":1}\n```";
        assert_eq!(parse(raw)["a"], 1);
    }

    #[test]
    fn strips_fence_with_outer_whitespace() {
        let raw = "\n\n  ```json\n{\"a\":1}\n```  \n";
        assert_eq!(parse(raw)["a"], 1);
    }

    #[test]
    fn strips_leading_prose_preamble() {
        let raw = "Here is the JSON you asked for:\n{\"subject\":\"Re: hi\",\"body\":\"x\"}";
        assert_eq!(parse(raw)["subject"], "Re: hi");
    }

    #[test]
    fn strips_trailing_prose_after_object() {
        let raw = "{\"a\":1}\n\nHope that helps!";
        assert_eq!(parse(raw)["a"], 1);
    }

    #[test]
    fn handles_nested_object_to_outermost_braces() {
        let raw = "prefix {\"outer\":{\"inner\":2}} suffix";
        let v = parse(raw);
        assert_eq!(v["outer"]["inner"], 2);
    }

    #[test]
    fn array_with_prose_both_sides() {
        let raw = "result: [{\"id\":1}] done";
        assert_eq!(parse(raw).as_array().unwrap().len(), 1);
    }

    #[test]
    fn no_braces_returns_trimmed_input_unchanged() {
        // No JSON structure at all: return trimmed text so the caller's serde error
        // surfaces the real (non-JSON) response.
        assert_eq!(extract_json("  not json at all  "), "not json at all");
    }

    #[test]
    fn empty_input_stays_empty() {
        assert_eq!(extract_json("   "), "");
    }
}

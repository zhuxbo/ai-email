//! `ai_translate` orchestrator.
//!
//! Same shape as [`crate::ai::summarize`]: resolve "translate" role default → keychain key →
//! body lookup → cache check → API call → persist → return.
//!
//! Cache key includes the `target` language so the same message translated to zh-CN and
//! en-US live as separate `ai_results` rows.

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

const ROLE: &str = "translate";
const KIND: &str = "translate";
const MAX_OUTPUT_TOKENS: u32 = 4096;
const MAX_TEXT_TRANSLATION_TOKENS: u32 = 2048;
/// Larger than summary's 50k because translation is roughly 1:1 in tokens — we can fit
/// most real-world mail with room to spare. Anything longer probably has structural noise
/// that translation wouldn't recover from anyway.
const MAX_BODY_CHARS: usize = 30_000;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Translation {
    pub target: String,
    pub subject: String,
    pub body: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TranslateResult {
    #[serde(flatten)]
    pub translation: Translation,
    pub source: &'static str, // "fresh" | "cached"
    pub model: String,
    pub input_tokens: Option<i32>,
    pub output_tokens: Option<i32>,
    pub cache_read_tokens: Option<i32>,
}

pub async fn translate_message(
    pool: &Pool,
    message_id: Uuid,
    target: &str,
) -> AppResult<TranslateResult> {
    let target = normalize_target(target)?;

    let model = ai_role_defaults::resolve_model(pool, ROLE)
        .await?
        .ok_or_else(|| {
            AppError::Config("请先在 AI 设置中配置默认翻译模型 (role = translate)".into())
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

    let body_text = pick_body_for_translate(&body)?;
    let user_prompt = build_user_prompt(&msg, &body_text, &target);
    let prompt_hash = compute_prompt_hash(prompts::TRANSLATE_SYSTEM, &target, &user_prompt);

    if let Some(cached) = ai_results::get(pool, message_id, KIND, &prompt_hash).await? {
        let translation: Translation = serde_json::from_value(cached.output)?;
        tracing::info!(message_id = %message_id, target = %target, "translate cache hit");
        return Ok(TranslateResult {
            translation,
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
                text: prompts::TRANSLATE_SYSTEM.to_string(),
                cache: true,
            }],
            messages: vec![UserMessage {
                content: user_prompt,
            }],
        })
        .await?;

    let mut translation: Translation =
        serde_json::from_str(extract_json(&response.text)).map_err(|e| {
            AppError::Ai(format!(
                "翻译模型未返回合法 JSON：{e}\n原文：{}",
                truncate_for_log(&response.text)
            ))
        })?;
    // Defensive: the model occasionally returns a slightly different target tag (e.g. "zh"
    // instead of "zh-CN"). Force the field to match what we asked for so the UI's cache
    // assumptions hold.
    translation.target = target.clone();

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
            output: serde_json::to_value(&translation)?,
            input_tokens,
            output_tokens,
            cache_read_tokens,
        },
    )
    .await?;

    tracing::info!(
        message_id = %message_id,
        target = %target,
        input_tokens = ?stored.input_tokens,
        output_tokens = ?stored.output_tokens,
        "translate fresh result persisted"
    );

    Ok(TranslateResult {
        translation,
        source: "fresh",
        model: stored.model,
        input_tokens: stored.input_tokens,
        output_tokens: stored.output_tokens,
        cache_read_tokens: stored.cache_read_tokens,
    })
}

fn pick_body_for_translate(body: &crate::db::bodies::MessageBody) -> AppResult<String> {
    let raw = body
        .text_plain
        .as_deref()
        .or(body.html.as_deref())
        .ok_or_else(|| AppError::Ai("邮件没有可翻译的正文内容".into()))?;
    if raw.chars().count() <= MAX_BODY_CHARS {
        return Ok(raw.to_string());
    }
    let truncated: String = raw.chars().take(MAX_BODY_CHARS).collect();
    Ok(format!(
        "{truncated}\n\n[... 邮件已截断，仅翻译前 {MAX_BODY_CHARS} 字符]"
    ))
}

fn build_user_prompt(msg: &MessageHeader, body: &str, target: &str) -> String {
    format!(
        "目标语言：{target}\n\n主题：{subject}\n\n正文：\n{body}",
        target = target,
        subject = msg.subject.as_deref().unwrap_or("(无主题)"),
        body = body,
    )
}

fn compute_prompt_hash(system: &str, target: &str, user: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(system.as_bytes());
    hasher.update(b"\n---\n");
    hasher.update(target.as_bytes());
    hasher.update(b"\n");
    hasher.update(user.as_bytes());
    hex::encode(hasher.finalize())
}

/// Reject empty / obviously bogus target strings; we don't enforce a closed set since
/// BCP-47 is large, but the input shouldn't be free-form garbage either.
fn normalize_target(raw: &str) -> AppResult<String> {
    let t = raw.trim();
    if t.is_empty() {
        return Err(AppError::Ai("target language must not be empty".into()));
    }
    if t.len() > 16 {
        return Err(AppError::Ai(format!(
            "target language too long: {} (BCP-47 codes are short)",
            truncate_for_log(t)
        )));
    }
    Ok(t.to_string())
}

fn truncate_for_log(s: &str) -> String {
    let limit = 200;
    if s.chars().count() <= limit {
        s.to_string()
    } else {
        let cut: String = s.chars().take(limit).collect();
        format!("{cut}…")
    }
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TextTranslation {
    pub text: String,
}

fn build_text_user_prompt(target: &str, text: &str) -> String {
    format!("目标语言：{target}\n\n文本：\n{text}")
}

/// 翻译自由文本（草稿回译）。复用 translate 角色模型；不读 DB、不缓存。
pub async fn translate_text(pool: &Pool, text: &str, target: &str) -> AppResult<TextTranslation> {
    let target = normalize_target(target)?;
    let model = ai_role_defaults::resolve_model(pool, ROLE)
        .await?
        .ok_or_else(|| AppError::Config(format!("未指派 {ROLE} 角色模型")))?;
    let model_uuid = model.id;
    let api_key: SecretString =
        tokio::task::spawn_blocking(move || keychain::get_ai_key(model_uuid))
            .await
            .map_err(|e| AppError::Other(anyhow::anyhow!(e)))??;
    let client = AiClient::build(&model, api_key)?;
    let req = CompletionRequest {
        max_tokens: MAX_TEXT_TRANSLATION_TOKENS,
        system: vec![SystemBlock {
            text: prompts::TRANSLATE_TEXT_SYSTEM.to_string(),
            cache: false,
        }],
        messages: vec![UserMessage {
            content: build_text_user_prompt(&target, text),
        }],
    };
    let resp = client.complete(req).await?;
    tracing::info!(
        target = %target,
        input_tokens = resp.usage.input_tokens,
        output_tokens = resp.usage.output_tokens,
        "translate_text fresh"
    );
    Ok(TextTranslation {
        text: resp.text.trim().to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_target_trims() {
        assert_eq!(normalize_target("  zh-CN  ").unwrap(), "zh-CN");
    }

    #[test]
    fn normalize_target_rejects_empty() {
        assert!(normalize_target("").is_err());
        assert!(normalize_target("   ").is_err());
    }

    #[test]
    fn normalize_target_rejects_long_input() {
        // 17 个 ASCII 字节 → 超上限
        assert!(normalize_target(&"x".repeat(17)).is_err());
        // 恰好 16 个 ASCII 字节 → 应接受（边界值）
        assert!(normalize_target(&"x".repeat(16)).is_ok());
    }

    // #51 多字节 UTF-8 场景：len() 计字节而非字符数。
    // BCP-47 标签实际为纯 ASCII（"zh-CN"、"en-US" 等），故生产路径不受影响；
    // 但以下用例显式记录实现的边界行为，防止将来改为字符计数时产生回归。
    #[test]
    fn normalize_target_multibyte_within_byte_limit_is_accepted() {
        // "zh语" = 2 ASCII + 3字节汉字 = 5 字节，远低于 16 字节上限 → 应接受
        assert!(normalize_target("zh语").is_ok());
    }

    #[test]
    fn normalize_target_multibyte_exceeding_byte_limit_is_rejected() {
        // 7 个汉字 = 21 字节 > 16，用字节计数 → 应拒绝
        // （BCP-47 不会出现此情形，但验证上限判断不会 panic 或截断字符边界）
        let long_cjk = "中文语言目标设"; // 7 CJK = 21 字节
        assert!(normalize_target(long_cjk).is_err());
    }

    #[test]
    fn normalize_target_multibyte_emoji_within_byte_limit_is_accepted() {
        // 单个 emoji（4 字节）远低于 16 字节上限，不应 panic 也不应误拒
        assert!(normalize_target("🌍").is_ok());
    }

    #[test]
    fn normalize_target_multibyte_no_panic_at_boundary() {
        // 4 个 emoji = 16 字节，恰好在上限边界，确认不 panic 也不误截字符边界
        let four_emoji = "🌍🌎🌏🗺"; // 4×4 = 16 字节
        assert!(normalize_target(four_emoji).is_ok());
        // 5 个 emoji = 20 字节，应被拒绝
        let five_emoji = "🌍🌎🌏🗺🌐";
        assert!(normalize_target(five_emoji).is_err());
    }

    #[test]
    fn prompt_hash_includes_target() {
        let a = compute_prompt_hash("sys", "zh-CN", "u");
        let b = compute_prompt_hash("sys", "en-US", "u");
        assert_ne!(a, b, "different targets must yield different hashes");
    }

    #[test]
    fn text_user_prompt_includes_target_and_body() {
        let p = build_text_user_prompt("en-US", "你好世界");
        assert!(p.contains("目标语言：en-US"));
        assert!(p.contains("文本：\n你好世界"));
    }

    /// A fenced ```json response must still deserialize into `Translation` via `extract_json`.
    /// Structural assertion on the parsed value, not on raw text.
    #[test]
    fn fenced_response_parses_into_translation() {
        let raw = "```json\n{\"target\":\"zh-CN\",\"subject\":\"主题\",\"body\":\"正文\"}\n```";
        let t: Translation = serde_json::from_str(extract_json(raw)).expect("should parse");
        assert_eq!(t.target, "zh-CN");
        assert_eq!(t.subject, "主题");
    }
}

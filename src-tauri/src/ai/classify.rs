//! `ai_classify` orchestrator.
//!
//! Flow:
//!   1. Resolve "classify" role default → AiModel. Bail with a UI-friendly error if unset.
//!   2. Pull keychain key on a blocking thread.
//!   3. Fetch the message rows (id + subject + from + snippet).
//!   4. Chunk into batches of `BATCH_SIZE`; for each chunk:
//!      - Build a numbered prompt — the model echoes back the input ids.
//!      - Per-message sha256(system || \n---\n || per_msg_payload) used as the
//!        `ai_results.prompt_hash` so re-classification on the same input is a cache hit.
//!      - Skip cache-hit messages from the API call body (still return their cached result
//!        in the SyncReport).
//!      - Call complete() once per chunk.
//!      - Parse JSON array; persist per-message (ai_results + messages.priority/category +
//!        message_tags).
//!
//! Designed for the inbox-sync background path: classify_message_ids is called from a
//! `tokio::spawn` after `sync_inbox` returns. Errors are logged and surfaced via the
//! returned Vec — callers (UI) decide how to display.

use std::collections::HashMap;

use secrecy::SecretString;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::ai::{extract_json, prompts, AiClient, CompletionRequest, SystemBlock, UserMessage};
use crate::db::ai_results::{self, AiResultInsert};
use crate::db::messages::{self, ClassifyInput};
use crate::db::{ai_role_defaults, message_tags, Pool};
use crate::error::{AppError, AppResult};
use crate::keychain;

const ROLE: &str = "classify";
const KIND: &str = "classify";
const BATCH_SIZE: usize = 20;
const MAX_OUTPUT_TOKENS: u32 = 2048;

/// Valid `category` values. The classifier prompt forces these but we double-check here so
/// a bad model response can't poison the DB.
const VALID_CATEGORIES: &[&str] = &["personal", "work", "notification", "promotion", "spam"];

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Classification {
    pub category: String,
    pub priority: i32,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClassifyResult {
    pub message_id: Uuid,
    #[serde(flatten)]
    pub classification: Classification,
    pub source: &'static str, // "fresh" | "cached"
}

/// What the model returns per input. Deserialised from the JSON array element.
#[derive(Debug, Deserialize)]
struct ModelClassifyItem {
    id: Uuid,
    category: String,
    priority: i32,
    #[serde(default)]
    tags: Vec<String>,
}

pub async fn classify_message_ids(
    pool: &Pool,
    message_ids: &[Uuid],
) -> AppResult<Vec<ClassifyResult>> {
    if message_ids.is_empty() {
        return Ok(Vec::new());
    }
    let model = ai_role_defaults::resolve_model(pool, ROLE)
        .await?
        .ok_or_else(|| {
            AppError::Config("请先在 AI 设置中配置默认分类模型 (role = classify)".into())
        })?;
    let model_uuid = model.id;
    let api_key: SecretString =
        tokio::task::spawn_blocking(move || keychain::get_ai_key(model_uuid))
            .await
            .map_err(|e| AppError::Other(anyhow::anyhow!(e)))??;

    let inputs = messages::fetch_for_classify(pool, message_ids).await?;
    let mut results: Vec<ClassifyResult> = Vec::with_capacity(inputs.len());

    let client = AiClient::build(&model, api_key)?;

    for chunk in inputs.chunks(BATCH_SIZE) {
        let chunk_results = classify_chunk(pool, &client, chunk).await?;
        results.extend(chunk_results);
    }

    tracing::info!(count = results.len(), "classify batch done");
    Ok(results)
}

async fn classify_chunk(
    pool: &Pool,
    client: &AiClient,
    chunk: &[ClassifyInput],
) -> AppResult<Vec<ClassifyResult>> {
    // Per-message prompt_hash for cache lookup; skip cache hits from the live API call.
    let hashes: Vec<String> = chunk
        .iter()
        .map(|m| compute_item_hash(prompts::CLASSIFY_SYSTEM, m))
        .collect();

    let mut pending_inputs: Vec<&ClassifyInput> = Vec::new();
    let mut pending_hashes: Vec<&str> = Vec::new();
    let mut out: Vec<ClassifyResult> = Vec::with_capacity(chunk.len());

    for (msg, hash) in chunk.iter().zip(hashes.iter()) {
        match ai_results::get(pool, msg.id, KIND, hash).await? {
            Some(cached) => {
                let classification: Classification = serde_json::from_value(cached.output)?;
                out.push(ClassifyResult {
                    message_id: msg.id,
                    classification,
                    source: "cached",
                });
            }
            None => {
                pending_inputs.push(msg);
                pending_hashes.push(hash.as_str());
            }
        }
    }

    if pending_inputs.is_empty() {
        return Ok(out);
    }

    let user_prompt = build_chunk_prompt(&pending_inputs);
    let response = client
        .complete(CompletionRequest {
            max_tokens: MAX_OUTPUT_TOKENS,
            system: vec![SystemBlock {
                text: prompts::CLASSIFY_SYSTEM.to_string(),
                cache: true,
            }],
            messages: vec![UserMessage {
                content: user_prompt,
            }],
        })
        .await?;

    let items: Vec<ModelClassifyItem> = serde_json::from_str(extract_json(&response.text))
        .map_err(|e| {
            AppError::Ai(format!(
                "分类模型未返回合法 JSON 数组：{e}\n原文：{}",
                truncate_for_log(&response.text)
            ))
        })?;

    // Keep the first item per id. A duplicate id means the model echoed one input twice
    // (or hallucinated a collision); silently letting the later one win would discard the
    // earlier classification non-deterministically. Warn and drop the duplicate instead.
    let mut by_id: HashMap<Uuid, ModelClassifyItem> = HashMap::with_capacity(items.len());
    for item in items {
        match by_id.entry(item.id) {
            std::collections::hash_map::Entry::Occupied(e) => {
                tracing::warn!(
                    message_id = %e.key(),
                    "classify response had duplicate id, keeping first and dropping duplicate"
                );
            }
            std::collections::hash_map::Entry::Vacant(slot) => {
                slot.insert(item);
            }
        }
    }

    // Roughly amortize the response's input / output / cache tokens across pending items.
    // Not exact (the model returned one combined response) but good enough for the audit log.
    let per_input = response
        .usage
        .input_tokens
        .checked_div(pending_inputs.len().max(1) as u32)
        .unwrap_or(0);
    let per_output = response
        .usage
        .output_tokens
        .checked_div(pending_inputs.len().max(1) as u32)
        .unwrap_or(0);
    let per_cache = response
        .usage
        .cache_read_input_tokens
        .and_then(|n| n.checked_div(pending_inputs.len().max(1) as u32));

    for (msg, hash) in pending_inputs.iter().zip(pending_hashes.iter()) {
        let Some(item) = by_id.get(&msg.id) else {
            tracing::warn!(message_id = %msg.id, "classify response missing id, skipping");
            continue;
        };
        let category = normalize_category(&item.category);
        let priority = clamp_priority(item.priority);
        let tags: Vec<String> = item
            .tags
            .iter()
            .filter(|t| !t.trim().is_empty())
            .take(3)
            .map(|t| t.trim().to_string())
            .collect();

        let classification = Classification {
            category: category.clone(),
            priority,
            tags: tags.clone(),
        };

        // Persist
        messages::update_classification(pool, msg.id, priority, &category).await?;
        message_tags::replace_ai_tags(pool, msg.id, &tags).await?;
        ai_results::insert(
            pool,
            &AiResultInsert {
                message_id: msg.id,
                kind: KIND.into(),
                model: response.model.clone(),
                prompt_hash: (*hash).to_string(),
                output: serde_json::to_value(&classification)?,
                input_tokens: i32::try_from(per_input).ok(),
                output_tokens: i32::try_from(per_output).ok(),
                cache_read_tokens: per_cache.and_then(|n| i32::try_from(n).ok()),
            },
        )
        .await?;

        out.push(ClassifyResult {
            message_id: msg.id,
            classification,
            source: "fresh",
        });
    }

    Ok(out)
}

fn build_chunk_prompt(inputs: &[&ClassifyInput]) -> String {
    let mut s = String::with_capacity(inputs.len() * 200);
    s.push_str("待分类的邮件列表：\n\n");
    for (i, msg) in inputs.iter().enumerate() {
        s.push_str(&format!(
            "[{i}]\nid: {id}\n主题: {subject}\n发件人: {from}\n片段: {snippet}\n\n",
            i = i + 1,
            id = msg.id,
            subject = msg.subject.as_deref().unwrap_or("(无主题)"),
            from = msg.from_addr.as_deref().unwrap_or("(无发件人)"),
            snippet = msg.snippet.as_deref().unwrap_or("(无片段)"),
        ));
    }
    s
}

fn compute_item_hash(system: &str, msg: &ClassifyInput) -> String {
    let mut hasher = Sha256::new();
    hasher.update(system.as_bytes());
    hasher.update(b"\n---\n");
    hasher.update(msg.id.as_bytes());
    hasher.update(b"\n");
    hasher.update(msg.subject.as_deref().unwrap_or("").as_bytes());
    hasher.update(b"\n");
    hasher.update(msg.from_addr.as_deref().unwrap_or("").as_bytes());
    hasher.update(b"\n");
    hasher.update(msg.snippet.as_deref().unwrap_or("").as_bytes());
    hex::encode(hasher.finalize())
}

fn normalize_category(raw: &str) -> String {
    let lower = raw.trim().to_lowercase();
    if VALID_CATEGORIES.contains(&lower.as_str()) {
        lower
    } else {
        // Defensive: model went off-script. Bucket as 'notification' (safest neutral default)
        // so the message still shows up; user can re-classify manually later.
        tracing::warn!(
            raw,
            "unknown category from model, defaulting to 'notification'"
        );
        "notification".to_string()
    }
}

fn clamp_priority(p: i32) -> i32 {
    p.clamp(1, 3)
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
    fn normalize_category_accepts_known_values() {
        assert_eq!(normalize_category("PERSONAL"), "personal");
        assert_eq!(normalize_category("  Work  "), "work");
        assert_eq!(normalize_category("spam"), "spam");
    }

    #[test]
    fn normalize_category_buckets_unknown_to_notification() {
        assert_eq!(normalize_category("urgent"), "notification");
        assert_eq!(normalize_category(""), "notification");
        assert_eq!(normalize_category("社交"), "notification");
    }

    #[test]
    fn clamp_priority_clips_extremes() {
        assert_eq!(clamp_priority(0), 1);
        assert_eq!(clamp_priority(-5), 1);
        assert_eq!(clamp_priority(4), 3);
        assert_eq!(clamp_priority(100), 3);
        assert_eq!(clamp_priority(2), 2);
    }
}

//! Per-message AI commands. They look up the configured model via `ai_role_defaults` and
//! the key from the OS keychain inside the orchestrator — the command layer stays a thin
//! State+id wrapper.

use tauri::State;
use uuid::Uuid;

use crate::ai::classify::{self, ClassifyResult};
use crate::ai::draft::{self, DraftResult};
use crate::ai::summarize::{self, SummaryResult};
use crate::ai::translate::{self, TextTranslation, TranslateResult};
use crate::auto_reply;
use crate::error::AppResult;
use crate::AppState;

#[tauri::command]
pub async fn ai_summarize(state: State<'_, AppState>, id: Uuid) -> AppResult<SummaryResult> {
    summarize::summarize_message(state.pool().await?, id).await
}

/// Manual (re-)classify of one or more messages. Sync auto-fires the background path; this
/// command is the explicit "重新分类" hook the UI can wire to a button later.
///
/// 分类写回 category/priority 后立即评估自动回复规则（#70），使建议队列按新分类更新。
/// 规则评估失败仅 warn，不影响分类结果返回——符合 best-effort 惯例。
#[tauri::command]
pub async fn ai_classify(
    state: State<'_, AppState>,
    ids: Vec<Uuid>,
) -> AppResult<Vec<ClassifyResult>> {
    let pool = state.pool().await?;
    let results = classify::classify_message_ids(pool, &ids).await?;
    // 评估自动回复规则：仅对本次分类成功写回的 id 评估，失败 warn 不传播。
    let classified_ids: Vec<Uuid> = results.iter().map(|r| r.message_id).collect();
    if let Err(e) = auto_reply::evaluate_rules_for_messages(pool, &classified_ids).await {
        tracing::warn!(error = %e, "ai_classify: auto-reply rule eval failed (non-fatal)");
    }
    Ok(results)
}

#[tauri::command]
pub async fn ai_translate(
    state: State<'_, AppState>,
    id: Uuid,
    target: String,
) -> AppResult<TranslateResult> {
    translate::translate_message(state.pool().await?, id, &target).await
}

#[tauri::command]
pub async fn ai_translate_text(
    state: State<'_, AppState>,
    text: String,
    target: String,
) -> AppResult<TextTranslation> {
    translate::translate_text(state.pool().await?, &text, &target).await
}

// #71 force=Some(true) 时绕过缓存强制重新生成，省略或 false 时走正常缓存路径。
#[tauri::command]
pub async fn ai_draft_reply(
    state: State<'_, AppState>,
    id: Uuid,
    intent: Option<String>,
    force: Option<bool>,
) -> AppResult<DraftResult> {
    draft::draft_reply(
        state.pool().await?,
        id,
        intent.as_deref(),
        force.unwrap_or(false),
    )
    .await
}

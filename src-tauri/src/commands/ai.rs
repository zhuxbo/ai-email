//! Per-message AI commands. They look up the configured model via `ai_role_defaults` and
//! the key from the OS keychain inside the orchestrator — the command layer stays a thin
//! State+id wrapper.

use tauri::State;
use uuid::Uuid;

use crate::ai::classify::{self, ClassifyResult};
use crate::ai::draft::{self, DraftResult};
use crate::ai::summarize::{self, SummaryResult};
use crate::ai::translate::{self, TranslateResult};
use crate::error::AppResult;
use crate::AppState;

#[tauri::command]
pub async fn ai_summarize(state: State<'_, AppState>, id: Uuid) -> AppResult<SummaryResult> {
    summarize::summarize_message(&state.db, id).await
}

/// Manual (re-)classify of one or more messages. Sync auto-fires the background path; this
/// command is the explicit "重新分类" hook the UI can wire to a button later.
#[tauri::command]
pub async fn ai_classify(
    state: State<'_, AppState>,
    ids: Vec<Uuid>,
) -> AppResult<Vec<ClassifyResult>> {
    classify::classify_message_ids(&state.db, &ids).await
}

#[tauri::command]
pub async fn ai_translate(
    state: State<'_, AppState>,
    id: Uuid,
    target: String,
) -> AppResult<TranslateResult> {
    translate::translate_message(&state.db, id, &target).await
}

#[tauri::command]
pub async fn ai_draft_reply(
    state: State<'_, AppState>,
    id: Uuid,
    intent: Option<String>,
) -> AppResult<DraftResult> {
    draft::draft_reply(&state.db, id, intent.as_deref()).await
}

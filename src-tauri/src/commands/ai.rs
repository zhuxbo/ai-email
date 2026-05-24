//! Per-message AI commands. They look up the configured model via `ai_role_defaults` and
//! the key from the OS keychain inside the orchestrator — the command layer stays a thin
//! State+id wrapper.

use tauri::State;
use uuid::Uuid;

use crate::ai::summarize::{self, SummaryResult};
use crate::error::AppResult;
use crate::AppState;

#[tauri::command]
pub async fn ai_summarize(state: State<'_, AppState>, id: Uuid) -> AppResult<SummaryResult> {
    summarize::summarize_message(&state.db, id).await
}

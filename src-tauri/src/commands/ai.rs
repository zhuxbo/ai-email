//! AI commands. Each one builds a fresh `AnthropicClient` so a rotated API key in the
//! environment picks up on the next call — cheap (~ms) since reqwest's internal pool is
//! lazy. Move to `AppState`-cached client when call rate justifies it.

use tauri::State;
use uuid::Uuid;

use crate::ai::client::AnthropicClient;
use crate::ai::summarize::{self, SummaryResult};
use crate::error::AppResult;
use crate::AppState;

#[tauri::command]
pub async fn ai_summarize(state: State<'_, AppState>, id: Uuid) -> AppResult<SummaryResult> {
    let client = AnthropicClient::from_env()?;
    summarize::summarize_message(&state.db, &client, id).await
}

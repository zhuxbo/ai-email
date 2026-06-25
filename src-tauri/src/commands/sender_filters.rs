//! 黑白名单命令：薄封装，业务逻辑在 db::sender_filters。

use tauri::State;
use uuid::Uuid;

use crate::db::sender_filters::{self, SenderFilter};
use crate::error::AppResult;
use crate::AppState;

#[tauri::command]
pub async fn sender_filters_list(state: State<'_, AppState>) -> AppResult<Vec<SenderFilter>> {
    sender_filters::load_all(state.pool().await?).await
}

#[tauri::command]
pub async fn sender_filters_add(
    state: State<'_, AppState>,
    list_type: String,
    value: String,
    note: Option<String>,
) -> AppResult<SenderFilter> {
    let (match_type, pattern) = sender_filters::normalize_entry(&value)?;
    let note = note.and_then(|n| {
        let t = n.trim();
        (!t.is_empty()).then(|| t.to_string())
    });
    sender_filters::insert(
        state.pool().await?,
        &list_type,
        &match_type,
        &pattern,
        note.as_deref(),
    )
    .await
}

#[tauri::command]
pub async fn sender_filters_remove(state: State<'_, AppState>, id: Uuid) -> AppResult<()> {
    sender_filters::delete(state.pool().await?, id).await
}

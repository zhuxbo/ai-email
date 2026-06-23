//! 系统级命令——不依赖业务逻辑，可在 DB 未就绪时调用。

use crate::error::AppResult;
use crate::{AppState, DbStatusPayload};

/// 查询 DB 初始化状态——不依赖连接池，pool 未就绪时也可正常工作。
#[tauri::command]
pub async fn db_status(state: tauri::State<'_, AppState>) -> AppResult<DbStatusPayload> {
    let status = state.db_init_status.lock().await.clone();
    Ok(crate::db_init_status_to_payload(status))
}

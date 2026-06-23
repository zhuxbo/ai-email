//! 系统级命令——不依赖业务逻辑，可在 DB 未就绪时调用。

use crate::error::AppResult;
use crate::{AppState, DbInitStatus, DbStatusPayload};

/// 查询 DB 初始化状态——不依赖连接池，pool 未就绪时也可正常工作。
#[tauri::command]
pub async fn db_status(state: tauri::State<'_, AppState>) -> AppResult<DbStatusPayload> {
    let status = state.db_init_status.lock().await.clone();
    let payload = match status {
        DbInitStatus::Initializing => DbStatusPayload {
            status: "initializing".into(),
            message: None,
        },
        DbInitStatus::Ready => DbStatusPayload {
            status: "ready".into(),
            message: None,
        },
        DbInitStatus::Failed(msg) => DbStatusPayload {
            status: "error".into(),
            message: Some(msg),
        },
    };
    Ok(payload)
}

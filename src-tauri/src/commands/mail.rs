//! Mail commands: sync + listing.
//!
//! Body / detail commands move here in Sprint 1.4; AI commands in Sprints 2+.

use tauri::State;
use uuid::Uuid;

use crate::db::mailboxes::{self, Mailbox};
use crate::db::messages::{self, MessageHeader};
use crate::db::{self};
use crate::error::{AppError, AppResult};
use crate::imap::sync::{self, SyncReport};
use crate::keychain;
use crate::AppState;

#[tauri::command]
pub async fn inbox_sync(state: State<'_, AppState>, account_id: Uuid) -> AppResult<SyncReport> {
    let account = db::accounts::get(&state.db, account_id)
        .await?
        .ok_or_else(|| AppError::Config(format!("account {account_id} not found")))?;

    let auth = tokio::task::spawn_blocking(move || keychain::get_auth_code(account_id))
        .await
        .map_err(|e| AppError::Other(anyhow::anyhow!(e)))??;

    sync::sync_inbox(&state.db, &account, &auth).await
}

#[tauri::command]
pub async fn mailboxes_list(
    state: State<'_, AppState>,
    account_id: Uuid,
) -> AppResult<Vec<Mailbox>> {
    mailboxes::list(&state.db, account_id).await
}

#[tauri::command]
pub async fn messages_list(
    state: State<'_, AppState>,
    mailbox_id: Uuid,
    limit: i64,
    offset: i64,
) -> AppResult<Vec<MessageHeader>> {
    messages::list_in_mailbox(&state.db, mailbox_id, limit, offset).await
}

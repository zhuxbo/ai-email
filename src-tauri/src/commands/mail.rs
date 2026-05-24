//! Mail commands: sync, listing, detail, lazy body fetch.
//!
//! AI commands move into separate command modules in Sprints 2+.

use tauri::State;
use uuid::Uuid;

use crate::db::bodies::{self, MessageBody};
use crate::db::mailboxes::{self, Mailbox};
use crate::db::messages::{self, MessageHeader};
use crate::db::{self};
use crate::error::{AppError, AppResult};
use crate::imap::client::ImapClient;
use crate::imap::parse;
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

#[tauri::command]
pub async fn message_get(state: State<'_, AppState>, id: Uuid) -> AppResult<MessageHeader> {
    messages::get(&state.db, id)
        .await?
        .ok_or_else(|| AppError::Config(format!("message {id} not found")))
}

/// Returns the cached body if we already have it; otherwise opens IMAP, fetches `BODY[]`,
/// persists the result, and backfills `snippet` + `has_attachment` on the header row.
///
/// Side effect on first call: opens an IMAP session, so this command is comparatively slow
/// (~1–3s on a warm network). Subsequent calls hit the cache and return in <50ms.
#[tauri::command]
pub async fn message_body(state: State<'_, AppState>, id: Uuid) -> AppResult<MessageBody> {
    if let Some(body) = bodies::get(&state.db, id).await? {
        return Ok(body);
    }

    let msg = messages::get(&state.db, id)
        .await?
        .ok_or_else(|| AppError::Config(format!("message {id} not found")))?;
    let account = db::accounts::get(&state.db, msg.account_id)
        .await?
        .ok_or_else(|| AppError::Config(format!("account {} not found", msg.account_id)))?;
    let mailbox = mailboxes::get(&state.db, msg.mailbox_id)
        .await?
        .ok_or_else(|| AppError::Config(format!("mailbox {} not found", msg.mailbox_id)))?;
    let uid = u32::try_from(msg.imap_uid)
        .map_err(|_| AppError::Imap(format!("invalid imap_uid: {}", msg.imap_uid)))?;

    let account_id = account.id;
    let auth = tokio::task::spawn_blocking(move || keychain::get_auth_code(account_id))
        .await
        .map_err(|e| AppError::Other(anyhow::anyhow!(e)))??;

    let port = u16::try_from(account.imap_port)
        .map_err(|_| AppError::Imap(format!("invalid imap_port: {}", account.imap_port)))?;

    let mut client = ImapClient::connect(&account.imap_host, port, &account.email, &auth).await?;
    client.select(&mailbox.name).await?;
    let raw = client.uid_fetch_body(uid).await?;
    if let Err(e) = client.logout().await {
        tracing::warn!(error = ?e, "imap logout failed (non-fatal)");
    }

    let parsed = parse::parse_body(&raw);
    let snippet = parsed
        .text_plain
        .as_deref()
        .and_then(|t| parse::snippet(t, 200));

    let body = bodies::upsert(&state.db, id, &parsed).await?;
    messages::mark_body_fetched(&state.db, id, parsed.has_attachment, snippet).await?;
    tracing::info!(message_id = %id, "message body fetched and cached");
    Ok(body)
}

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
use crate::imap::client::{resolve_trash_mailbox, ImapClient};
use crate::imap::parse;
use crate::imap::sync::{self, SyncReport};
use crate::keychain;
use crate::smtp::{self, SendDraft, SendReceipt};
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

#[tauri::command]
pub async fn smtp_send(state: State<'_, AppState>, draft: SendDraft) -> AppResult<SendReceipt> {
    smtp::send_draft(&state.db, &draft).await
}

/// `set_seen` / `set_flagged` 公共流程：解析 → connect → select → STORE → logout → 本地 flags 同步。
async fn set_flag_impl(db: &db::Pool, id: Uuid, flag: &str, add: bool) -> AppResult<()> {
    let msg = messages::get(db, id)
        .await?
        .ok_or_else(|| AppError::Config(format!("message {id} not found")))?;
    let account = db::accounts::get(db, msg.account_id)
        .await?
        .ok_or_else(|| AppError::Config(format!("account {} not found", msg.account_id)))?;
    let mailbox = mailboxes::get(db, msg.mailbox_id)
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
    client.uid_set_flag(uid, flag, add).await?;
    if let Err(e) = client.logout().await {
        tracing::warn!(error = ?e, "imap logout failed (non-fatal)");
    }

    // 本地 flags 与服务端同步：在原 flags 基础上去重 add / remove。
    let mut flags = msg.flags.clone();
    let f = flag.to_string();
    if add {
        if !flags.contains(&f) {
            flags.push(f);
        }
    } else {
        flags.retain(|x| x != &f);
    }
    messages::update_flags(db, id, &flags).await
}

#[tauri::command]
pub async fn message_set_seen(state: State<'_, AppState>, id: Uuid, seen: bool) -> AppResult<()> {
    set_flag_impl(&state.db, id, "\\Seen", seen).await
}

#[tauri::command]
pub async fn message_set_flagged(
    state: State<'_, AppState>,
    id: Uuid,
    flagged: bool,
) -> AppResult<()> {
    set_flag_impl(&state.db, id, "\\Flagged", flagged).await
}

/// 删除 = 移到废纸篓（可恢复）。move 成功即逻辑成功；本地 remove 失败仅 warn 返 Ok
/// （服务端权威态已变，宁留极罕见幽灵行也不让用户看到删除回退）。
#[tauri::command]
pub async fn message_delete(state: State<'_, AppState>, id: Uuid) -> AppResult<()> {
    let db = &state.db;
    let msg = messages::get(db, id)
        .await?
        .ok_or_else(|| AppError::Config(format!("message {id} not found")))?;
    let account = db::accounts::get(db, msg.account_id)
        .await?
        .ok_or_else(|| AppError::Config(format!("account {} not found", msg.account_id)))?;
    let mailbox = mailboxes::get(db, msg.mailbox_id)
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
    let boxes = client.list_mailboxes().await?;
    let trash = resolve_trash_mailbox(&boxes)
        .ok_or_else(|| AppError::Imap("未找到废纸篓文件夹".to_string()))?;
    client.select(&mailbox.name).await?;
    client.uid_move(uid, &trash).await?;
    if let Err(e) = client.logout().await {
        tracing::warn!(error = ?e, "imap logout failed (non-fatal)");
    }

    if let Err(e) = messages::remove(db, id).await {
        tracing::warn!(message_id = %id, error = ?e, "local remove after trash-move failed (non-fatal)");
    }
    Ok(())
}

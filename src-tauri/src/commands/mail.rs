//! Mail commands: sync, listing, detail, lazy body fetch.
//!
//! AI commands move into separate command modules in Sprints 2+.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::{Arc, Mutex as StdMutex};

use tauri::State;
use tauri_plugin_dialog::{DialogExt, FilePath};
use uuid::Uuid;

use crate::db::bodies::{self, MessageBody};
use crate::db::mailboxes::{self, Mailbox};
use crate::db::messages::{self, MessageHeader};
use crate::db::{self};
use crate::error::{AppError, AppResult};
use crate::imap::manager::ImapManager;
use crate::imap::parse;
use crate::imap::sync::{self, SyncReport};
use crate::keychain;
use crate::smtp::{self, SendDraft, SendReceipt};
use crate::AppState;

const MAX_PAGE_LIMIT: i64 = 200;
const MAX_BULK_SEEN_IDS: usize = 500;
const DEFAULT_ATTACHMENT_FILENAME: &str = "attachment.bin";

fn normalize_page_limit(limit: i64) -> AppResult<i64> {
    if limit <= 0 {
        return Err(AppError::Config(format!(
            "limit must be positive, got {limit}"
        )));
    }
    Ok(limit.min(MAX_PAGE_LIMIT))
}

fn normalize_page_offset(offset: i64) -> AppResult<i64> {
    if offset < 0 {
        return Err(AppError::Config(format!(
            "offset must not be negative, got {offset}"
        )));
    }
    Ok(offset)
}

fn validate_bulk_seen_ids(ids: &[Uuid]) -> AppResult<()> {
    if ids.len() > MAX_BULK_SEEN_IDS {
        return Err(AppError::Config(format!(
            "too many message ids: {} (max {MAX_BULK_SEEN_IDS})",
            ids.len()
        )));
    }
    Ok(())
}

#[derive(Debug)]
struct AccountSyncGuard<'a> {
    account_id: Uuid,
    sync_in_flight: &'a StdMutex<HashSet<Uuid>>,
}

impl Drop for AccountSyncGuard<'_> {
    fn drop(&mut self) {
        match self.sync_in_flight.lock() {
            Ok(mut set) => {
                set.remove(&self.account_id);
            }
            Err(e) => {
                tracing::error!(error = %e, account_id = %self.account_id, "sync guard lock poisoned");
            }
        }
    }
}

fn try_acquire_account_sync(
    sync_in_flight: &StdMutex<HashSet<Uuid>>,
    account_id: Uuid,
) -> AppResult<AccountSyncGuard<'_>> {
    let mut set = sync_in_flight
        .lock()
        .map_err(|e| AppError::Other(anyhow::anyhow!("sync guard lock poisoned: {e}")))?;
    if !set.insert(account_id) {
        return Err(AppError::Config(
            "该账户正在同步，请等待当前同步完成".into(),
        ));
    }
    Ok(AccountSyncGuard {
        account_id,
        sync_in_flight,
    })
}

/// 同步收件箱。`force=true`（手动同步）绕过失败冷却；自动同步（force=false）在
/// 连续失败 ≥2 次后的冷却期内直接跳过，不再向服务端发起登录（防止持续撞限流）。
/// 无论成败都记录到 [`crate::imap::backoff::SyncBackoff`]（成功清零、失败 +1）。
#[tauri::command]
pub async fn inbox_sync(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    account_id: Uuid,
    force: Option<bool>,
) -> AppResult<SyncReport> {
    let _guard = try_acquire_account_sync(&state.sync_in_flight, account_id)?;
    if !force.unwrap_or(false) {
        let remaining = state
            .sync_backoff
            .remaining(account_id, std::time::Instant::now());
        if !remaining.is_zero() {
            // 静默跳过：冷却是保护行为不是故障，返回 Ok + 标记，前端不记错误横幅。
            // 冷却到期后的下一次自动同步会自然重试（实测恢复成功）。
            tracing::info!(
                account_id = %account_id,
                remaining_secs = remaining.as_secs(),
                "自动同步处于失败冷却期，本轮跳过"
            );
            return Ok(SyncReport {
                new_message_count: 0,
                total_in_mailbox: 0,
                cooldown_skipped: true,
            });
        }
    }
    let pool = state.pool().await?;
    let account = db::accounts::get(pool, account_id)
        .await?
        .ok_or_else(|| AppError::Config(format!("account {account_id} not found")))?;

    let auth = tokio::task::spawn_blocking(move || keychain::get_auth_code(account_id))
        .await
        .map_err(|e| AppError::Other(anyhow::anyhow!(e)))??;

    let result = sync::sync_inbox(
        pool,
        &state.imap,
        &account,
        &auth,
        state.cancel.clone(),
        Arc::clone(&state.account_tokens),
        app,
    )
    .await;
    match &result {
        Ok(_) => state.sync_backoff.record_success(account_id),
        Err(_) => state
            .sync_backoff
            .record_failure(account_id, std::time::Instant::now()),
    }
    result
}

#[tauri::command]
pub async fn mailboxes_list(
    state: State<'_, AppState>,
    account_id: Uuid,
) -> AppResult<Vec<Mailbox>> {
    mailboxes::list(state.pool().await?, account_id).await
}

/// Sync a specific mailbox on demand. Used when the user navigates to a non-INBOX folder
/// (Sent, Drafts, Trash, etc.). Does not trigger AI classification or auto-reply evaluation.
/// 用户驱动的操作不走冷却检查，但结果同样计入退避（成功即证明连通，清零计数）。
#[tauri::command]
pub async fn mailbox_sync(
    state: State<'_, AppState>,
    account_id: Uuid,
    mailbox_name: String,
) -> AppResult<SyncReport> {
    let _guard = try_acquire_account_sync(&state.sync_in_flight, account_id)?;
    let pool = state.pool().await?;
    let account = db::accounts::get(pool, account_id)
        .await?
        .ok_or_else(|| AppError::Config(format!("account {account_id} not found")))?;

    let auth = tokio::task::spawn_blocking(move || keychain::get_auth_code(account_id))
        .await
        .map_err(|e| AppError::Other(anyhow::anyhow!(e)))??;

    let result = sync::sync_mailbox(pool, &state.imap, &account, &auth, &mailbox_name).await;
    match &result {
        Ok(_) => state.sync_backoff.record_success(account_id),
        Err(_) => state
            .sync_backoff
            .record_failure(account_id, std::time::Instant::now()),
    }
    result
}

#[tauri::command]
pub async fn messages_list(
    state: State<'_, AppState>,
    mailbox_id: Uuid,
    limit: i64,
    offset: i64,
) -> AppResult<Vec<MessageHeader>> {
    let limit = normalize_page_limit(limit)?;
    let offset = normalize_page_offset(offset)?;
    messages::list_in_mailbox(state.pool().await?, mailbox_id, limit, offset).await
}

#[tauri::command]
pub async fn message_get(state: State<'_, AppState>, id: Uuid) -> AppResult<MessageHeader> {
    messages::get(state.pool().await?, id)
        .await?
        .ok_or_else(|| AppError::Config(format!("message {id} not found")))
}

/// Returns the cached body if we already have it; otherwise opens IMAP, fetches `BODY[]`,
/// persists the result, and backfills `snippet` + `has_attachment` on the header row.
///
/// Side effect on first call: opens an IMAP session, so this command is comparatively slow
/// (~1–3s on a warm network). Subsequent calls hit the cache and return in <50ms.
///
/// Concurrent requests for the same message id are de-duplicated via a single-flight map in
/// `AppState::body_in_flight`: only the first caller opens an IMAP session; latecomers wait
/// on a `watch::Receiver<bool>` and then read the newly cached row.
///
/// Watch 通道持有最新值，leader 先完成也不丢唤醒（不同于 `Notify::notify_waiters`）：迟到者
/// 克隆 receiver 后先读当前值，已为 `true` 则直接查缓存，否则调用 `changed().await` 等待。
#[tauri::command]
pub async fn message_body(state: State<'_, AppState>, id: Uuid) -> AppResult<MessageBody> {
    message_body_impl(state.pool().await?, &state.imap, &state.body_in_flight, id).await
}

/// `message_body` 的可测内核：把 `State` 依赖拆成显式参数（pool + 连接管理器 + single-flight map），
/// 便于在单元测试里注入并复现「迟到者」分支，无需构造完整 `AppState`。
async fn message_body_impl(
    pool: &db::Pool,
    imap: &ImapManager,
    body_in_flight: &tokio::sync::Mutex<
        std::collections::HashMap<Uuid, tokio::sync::watch::Receiver<bool>>,
    >,
    id: Uuid,
) -> AppResult<MessageBody> {
    // 快路径：缓存命中直接返回。
    if let Some(body) = bodies::get(pool, id).await? {
        if !body_cache_needs_refetch(&body) {
            return Ok(body);
        }
        tracing::info!(message_id = %id, "cached message body contains cid image; refetching");
    }

    // 检查是否已有并发请求在取此 id 的 body。
    let mut rx = {
        let mut map = body_in_flight.lock().await;
        if let Some(existing) = map.get(&id) {
            // 其他请求已在 in-flight：克隆 receiver 后释放锁，等待 leader 完成。
            existing.clone()
        } else {
            // 自己是 leader：建 watch 通道（初始 false），插入 map，立即释放锁，执行 IMAP 取。
            let (tx, rx) = tokio::sync::watch::channel(false);
            map.insert(id, rx);
            drop(map);

            // RAII guard：无论 fetch 成功/失败/panic，均自动移除 map 条目并将 watch 设为 true，
            // 确保等待者不会永久阻塞（M3：消除 leader panic 时的条目泄漏）。
            let _guard = BodyInFlightGuard {
                id,
                body_in_flight,
                tx,
            };

            return fetch_and_cache_body(pool, imap, id).await;
            // _guard 在此 drop：移除 map 条目 + send(true) 通知所有等待者。
        }
    };

    // 迟到者：先检查当前值，leader 可能在我们克隆 receiver 之前已完成。
    // watch 持有最新值，不丢唤醒，因此无论先后顺序都能正确拿到通知。
    if !*rx.borrow() {
        // leader 尚未完成，等待值变为 true。
        let _ = rx.changed().await;
    }

    // leader 完成后查缓存：命中即返回。
    if let Some(body) = bodies::get(pool, id).await? {
        return Ok(body);
    }

    // 缓存仍空 = leader 取正文失败（如 IMAP 超时 / 网络错误）且未写缓存。迟到者回退自取一次：
    // 成功则返回正文，失败则透传真实的 IMAP/网络错误，而非误导性的 "not in cache"。多个迟到者
    // 各自重取属罕见的错误恢复路径，可接受（不重新竞选 leader，避免逻辑复杂化）。
    fetch_and_cache_body(pool, imap, id).await
}

fn body_cache_needs_refetch(body: &MessageBody) -> bool {
    body.html
        .as_deref()
        .is_some_and(parse::contains_cid_image_src)
}

/// RAII guard：在 drop 时从 single-flight map 移除条目，并通过 watch sender 通知所有等待者。
/// 保证 leader 无论成功、失败还是 panic，等待者均不会永久挂起。
struct BodyInFlightGuard<'a> {
    id: Uuid,
    body_in_flight:
        &'a tokio::sync::Mutex<std::collections::HashMap<Uuid, tokio::sync::watch::Receiver<bool>>>,
    tx: tokio::sync::watch::Sender<bool>,
}

impl Drop for BodyInFlightGuard<'_> {
    fn drop(&mut self) {
        // 同步移除 map 条目；try_lock 在 drop 路径上（不能 .await）。
        // 正常情况下锁绝不会被本任务持有（fetch 期间锁已释放），try_lock 应立即成功。
        // 极端情况（如 panic 在锁内）try_lock 失败也无妨：条目残留最多造成下一次请求走迟到者路径，
        // 而 send(true) 总会执行，等待者不会挂起。
        if let Ok(mut map) = self.body_in_flight.try_lock() {
            map.remove(&self.id);
        }
        // 发送 true：所有持有 receiver 的迟到者均会被唤醒（watch 持状态，不丢通知）。
        let _ = self.tx.send(true);
    }
}

/// 在复用连接上 select → `UID FETCH BODY[]`，返回该消息的完整 RFC822 原文。
/// 供 `fetch_and_cache_body`（正文）与附件命令（列附件 / 取附件字节）复用。
async fn fetch_raw_body(db: &db::Pool, imap: &ImapManager, id: Uuid) -> AppResult<Vec<u8>> {
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

    let auth = get_account_auth(account.id).await?;
    let mailbox_name = mailbox.name.as_str();
    imap.run(&account, &auth, |mut lease| {
        Box::pin(async move {
            let client = lease.client()?;
            client.select(mailbox_name).await?;
            client.uid_fetch_body(uid).await
        })
    })
    .await
}

/// 从 keychain 取账户授权码（spawn_blocking 包装）。
async fn get_account_auth(account_id: Uuid) -> AppResult<secrecy::SecretString> {
    tokio::task::spawn_blocking(move || keychain::get_auth_code(account_id))
        .await
        .map_err(|e| AppError::Other(anyhow::anyhow!(e)))?
}

/// IMAP 取 body 并持久化到 DB，供 `message_body` 调用。
async fn fetch_and_cache_body(
    db: &db::Pool,
    imap: &ImapManager,
    id: Uuid,
) -> AppResult<MessageBody> {
    let msg = messages::get(db, id)
        .await?
        .ok_or_else(|| AppError::Config(format!("message {id} not found")))?;
    let account = db::accounts::get(db, msg.account_id)
        .await?
        .ok_or_else(|| AppError::Config(format!("account {} not found", msg.account_id)))?;
    let auth = get_account_auth(account.id).await?;
    crate::imap::materialize::materialize_one(db, imap, &account, &auth, id).await?;
    tracing::info!(message_id = %id, "message body fetched and cached");
    bodies::get(db, id)
        .await?
        .ok_or_else(|| AppError::Config(format!("body still missing after materialize: {id}")))
}

/// 列出某邮件的附件元信息（IMAP 取原文 → 解析，不入库）。供详情页附件区展示。
#[tauri::command]
pub async fn message_attachments(
    state: State<'_, AppState>,
    id: Uuid,
) -> AppResult<Vec<parse::AttachmentMeta>> {
    let raw = fetch_raw_body(state.pool().await?, &state.imap, id).await?;
    Ok(parse::parse_attachments(&raw))
}

fn attachment_default_filename(filename: Option<&str>) -> String {
    let Some(filename) = filename else {
        return DEFAULT_ATTACHMENT_FILENAME.to_string();
    };
    let candidate = filename
        .trim()
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or("")
        .trim();
    let sanitized: String = candidate.chars().filter(|c| !c.is_control()).collect();
    if sanitized.is_empty() || sanitized == "." || sanitized == ".." {
        DEFAULT_ATTACHMENT_FILENAME.to_string()
    } else {
        sanitized
    }
}

fn file_path_to_native_path(file_path: FilePath) -> AppResult<PathBuf> {
    file_path.into_path().map_err(|_| {
        AppError::Config("当前平台暂不支持向该目标保存附件，请选择本机文件路径".into())
    })
}

async fn write_attachment_to_path(path: PathBuf, bytes: Vec<u8>) -> AppResult<()> {
    tokio::task::spawn_blocking(move || std::fs::write(path, bytes))
        .await
        .map_err(|e| AppError::Other(anyhow::anyhow!(e)))?
        .map_err(|e| AppError::Config(format!("写入附件失败：{e}")))?;
    Ok(())
}

/// 把某邮件第 `index` 个附件写入用户在后端原生保存框中选择的位置。
/// 前端只传 message id + 附件序号，不再把任意文件路径穿过命令边界。
#[tauri::command]
pub async fn message_attachment_save(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    id: Uuid,
    index: usize,
) -> AppResult<()> {
    let raw = fetch_raw_body(state.pool().await?, &state.imap, id).await?;
    let default_name = attachment_default_filename(
        parse::parse_attachments(&raw)
            .get(index)
            .map(|attachment| attachment.filename.as_str()),
    );
    let bytes = parse::extract_attachment_bytes(&raw, index)
        .ok_or_else(|| AppError::Config(format!("attachment {index} not found")))?;

    let (tx, rx) = tokio::sync::oneshot::channel();
    app.dialog()
        .file()
        .set_title("保存附件")
        .set_file_name(default_name)
        .save_file(move |file_path| {
            let _ = tx.send(file_path);
        });

    let Some(file_path) = rx
        .await
        .map_err(|e| AppError::Other(anyhow::anyhow!("save dialog failed: {e}")))?
    else {
        return Ok(());
    };
    let path = file_path_to_native_path(file_path)?;
    write_attachment_to_path(path, bytes).await?;
    Ok(())
}

#[tauri::command]
pub async fn smtp_send(state: State<'_, AppState>, draft: SendDraft) -> AppResult<SendReceipt> {
    smtp::send_draft(state.pool().await?, &state.imap, &draft).await
}

/// `set_seen` / `set_flagged` 公共流程：解析 → 复用连接 select → STORE → 本地 flags 同步。
async fn set_flag_impl(
    db: &db::Pool,
    imap: &ImapManager,
    id: Uuid,
    flag: &str,
    add: bool,
) -> AppResult<()> {
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

    let auth = get_account_auth(account.id).await?;
    let mailbox_name = mailbox.name.as_str();
    imap.run(&account, &auth, |mut lease| {
        Box::pin(async move {
            let client = lease.client()?;
            client.select(mailbox_name).await?;
            client.uid_set_flag(uid, flag, add).await
        })
    })
    .await?;

    // 原子更新本地 flags：IMAP 往返成功后直接在 DB 内做单 flag add/remove，
    // 避免读-改-写并发竞争（#30）。
    messages::update_flag_atomic(db, id, flag, add).await
}

#[tauri::command]
pub async fn message_set_seen(state: State<'_, AppState>, id: Uuid, seen: bool) -> AppResult<()> {
    set_flag_impl(state.pool().await?, &state.imap, id, "\\Seen", seen).await
}

#[tauri::command]
pub async fn message_set_flagged(
    state: State<'_, AppState>,
    id: Uuid,
    flagged: bool,
) -> AppResult<()> {
    set_flag_impl(state.pool().await?, &state.imap, id, "\\Flagged", flagged).await
}

/// 批量标 `\Seen`（「全部已读」）。`ids` 可跨账户/信箱：按 (account, mailbox) 分组，每组一次
/// IMAP `UID STORE +FLAGS \Seen`，成功后本地批量标。某组失败即返回错误（已成功组的本地 flags
/// 已更新，前端 reload 会反映真实状态）。空 `ids` 直接返回。
#[tauri::command]
pub async fn messages_mark_seen_bulk(state: State<'_, AppState>, ids: Vec<Uuid>) -> AppResult<()> {
    use std::collections::HashMap;

    validate_bulk_seen_ids(&ids)?;
    let pool = state.pool().await?;
    if ids.is_empty() {
        return Ok(());
    }

    // 按 (account_id, mailbox_id) 分组收集 uid + id（跳过查不到的 id）。
    let mut groups: HashMap<(Uuid, Uuid), (Vec<u32>, Vec<Uuid>)> = HashMap::new();
    for id in ids {
        let Some(msg) = messages::get(pool, id).await? else {
            continue;
        };
        let uid = u32::try_from(msg.imap_uid)
            .map_err(|_| AppError::Imap(format!("invalid imap_uid: {}", msg.imap_uid)))?;
        let entry = groups.entry((msg.account_id, msg.mailbox_id)).or_default();
        entry.0.push(uid);
        entry.1.push(id);
    }

    for ((account_id, mailbox_id), (uids, msg_ids)) in groups {
        let account = db::accounts::get(pool, account_id)
            .await?
            .ok_or_else(|| AppError::Config(format!("account {account_id} not found")))?;
        let mailbox = mailboxes::get(pool, mailbox_id)
            .await?
            .ok_or_else(|| AppError::Config(format!("mailbox {mailbox_id} not found")))?;
        let auth = get_account_auth(account_id).await?;
        let mailbox_name = mailbox.name.as_str();
        let uids = uids.as_slice();

        state
            .imap
            .run(&account, &auth, |mut lease| {
                Box::pin(async move {
                    let client = lease.client()?;
                    client.select(mailbox_name).await?;
                    client.uid_set_flag_bulk(uids, "\\Seen", true).await
                })
            })
            .await?;

        // IMAP 成功 → 本地批量标 \Seen（持久化）。
        for id in msg_ids {
            messages::update_flag_atomic(pool, id, "\\Seen", true).await?;
        }
    }
    Ok(())
}

// ── 折叠列表（B1: db::folded） ────────────────────────────────────────────

/// 全局搜索：覆盖全部账户全部信箱的已同步邮件。主题/发件人/摘要覆盖全部行；
/// 正文覆盖已物化（打开过/会话加载过）的缓存。多词 AND，相关度（主题 > 发件人 >
/// 摘要/正文）+ 时间倒序。`account_id` Some 时限定单账户。
#[tauri::command]
pub async fn messages_search(
    state: State<'_, AppState>,
    query: String,
    account_id: Option<Uuid>,
    limit: Option<i64>,
) -> AppResult<Vec<messages::SearchRow>> {
    let limit = limit.unwrap_or(100).clamp(1, MAX_PAGE_LIMIT) as usize;
    messages::search_messages(state.pool().await?, &query, account_id, limit).await
}

/// 单信箱折叠列表：scope = 该信箱内的消息，折成 thread / sender / single 行。
#[tauri::command]
pub async fn mailbox_folded(
    state: State<'_, AppState>,
    mailbox_id: Uuid,
    limit: i64,
) -> AppResult<Vec<db::folded::FoldedItem>> {
    let limit = normalize_page_limit(limit)?;
    db::folded::mailbox_folded(state.pool().await?, mailbox_id, limit).await
}

/// 账户级收件箱折叠列表：汇聚账户下所有 inbox 类信箱（排除 Sent）。
#[tauri::command]
pub async fn account_inbox_folded(
    state: State<'_, AppState>,
    account_id: Uuid,
    limit: i64,
) -> AppResult<Vec<db::folded::FoldedItem>> {
    let limit = normalize_page_limit(limit)?;
    db::folded::account_inbox_folded(state.pool().await?, account_id, limit).await
}

// ── 全部已读（纯本地，不打 IMAP；见 db::messages::mark_seen_where） ────────────

/// 单信箱「全部已读」：把该信箱内所有消息标 `\Seen`（纯本地）。
#[tauri::command]
pub async fn mailbox_mark_seen(state: State<'_, AppState>, mailbox_id: Uuid) -> AppResult<()> {
    messages::mailbox_mark_seen(state.pool().await?, mailbox_id).await
}

/// 账户级收件箱「全部已读」：把账户下所有 inbox 类信箱内的消息标 `\Seen`（纯本地）。
#[tauri::command]
pub async fn account_inbox_mark_seen(
    state: State<'_, AppState>,
    account_id: Uuid,
) -> AppResult<()> {
    messages::account_inbox_mark_seen(state.pool().await?, account_id).await
}

/// 判断 IMAP 错误是否表示「邮件在服务端已不存在」（QQ: "Mails not exist!"）。
/// 这类错误下移动到废纸篓无意义——邮件本就没了，应容错为「已删除」继续本地清理，不报错。
fn imap_msg_already_gone(e: &AppError) -> bool {
    let AppError::Imap(msg) = e else {
        return false;
    };
    let lower = msg.to_lowercase();
    lower.contains("not exist")
        || lower.contains("no such message")
        || lower.contains("does not exist")
}

/// 从本地 mailboxes 缓存解析废纸篓名：优先 special_use='trash'；缺行时在复用连接上
/// 强制 LIST 刷新一次自愈（服务端新出现的 Trash / 首次同步前被删除等罕见场景）。
async fn resolve_trash_name(
    pool: &db::Pool,
    imap: &ImapManager,
    account: &db::accounts::Account,
    auth: &secrecy::SecretString,
) -> AppResult<String> {
    if let Some(trash) = mailboxes::get_by_special_use(pool, account.id, "trash").await? {
        return Ok(trash.name);
    }
    tracing::info!(account_id = %account.id, "trash mailbox row missing; forcing LIST refresh");
    imap.run(account, auth, |mut lease| {
        Box::pin(async move {
            let client = lease.client()?;
            sync::refresh_mailbox_list(pool, account.id, client).await
        })
    })
    .await?;
    mailboxes::get_by_special_use(pool, account.id, "trash")
        .await?
        .map(|m| m.name)
        .ok_or_else(|| AppError::Imap("未找到废纸篓文件夹".to_string()))
}

/// 删除 = 移到废纸篓（可恢复）。move 成功即逻辑成功；本地 remove 失败仅 warn 返 Ok
/// （服务端权威态已变，宁留极罕见幽灵行也不让用户看到删除回退）。
/// 服务端已无此邮件（move 报 not-exist）同样视为成功，跳过移动直接本地清理。
/// 废纸篓名从本地缓存解析（缺行才强制 LIST），不再每次删除都全量 LIST。
#[tauri::command]
pub async fn message_delete(state: State<'_, AppState>, id: Uuid) -> AppResult<()> {
    let pool = state.pool().await?;
    let msg = messages::get(pool, id)
        .await?
        .ok_or_else(|| AppError::Config(format!("message {id} not found")))?;
    let account = db::accounts::get(pool, msg.account_id)
        .await?
        .ok_or_else(|| AppError::Config(format!("account {} not found", msg.account_id)))?;
    let mailbox = mailboxes::get(pool, msg.mailbox_id)
        .await?
        .ok_or_else(|| AppError::Config(format!("mailbox {} not found", msg.mailbox_id)))?;
    let uid = u32::try_from(msg.imap_uid)
        .map_err(|_| AppError::Imap(format!("invalid imap_uid: {}", msg.imap_uid)))?;

    let auth = get_account_auth(account.id).await?;
    let trash = resolve_trash_name(pool, &state.imap, &account, &auth).await?;

    let mailbox_name = mailbox.name.as_str();
    let trash_name = trash.as_str();
    let move_result = state
        .imap
        .run(&account, &auth, |mut lease| {
            Box::pin(async move {
                let client = lease.client()?;
                client.select(mailbox_name).await?;
                client.uid_move(uid, trash_name).await
            })
        })
        .await;
    // 服务端已删除该邮件时 uid_move 报 "Mails not exist!"——容错为已删除，继续本地清理。
    if let Err(e) = move_result {
        if imap_msg_already_gone(&e) {
            tracing::warn!(message_id = %id, error = %e, "邮件在服务端已不存在，视为已删除，跳过移动");
        } else {
            return Err(e);
        }
    }

    if let Err(e) = messages::remove(pool, id).await {
        tracing::warn!(message_id = %id, error = %e, "local remove after trash-move failed (non-fatal)");
    }
    Ok(())
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemovedBlockView {
    pub kind: String, // 'signature' | 'quote' | 'repeat'
    pub text: String,
    pub reason: String,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageFilterPreview {
    pub net: String,
    pub removed: Vec<RemovedBlockView>,
    /// 当前 filter_disabled 态：true=AI 收完整原文（预览仍展示"若启用会剥成"）。
    pub disabled: bool,
    /// 原文（disabled 时 AI 实收；启用时供对照）。
    pub original: String,
}

fn target_label(t: crate::ai::extract::Target) -> &'static str {
    match t {
        crate::ai::extract::Target::Signature => "signature",
        crate::ai::extract::Target::Quote => "quote",
        crate::ai::extract::Target::Repeat => "repeat",
    }
}

fn preview_from_result(
    result: crate::ai::extract::ExtractResult,
    disabled: bool,
    original: String,
) -> MessageFilterPreview {
    MessageFilterPreview {
        net: result.net,
        removed: result
            .removed
            .into_iter()
            .map(|b| RemovedBlockView {
                kind: target_label(b.kind).to_string(),
                text: b.text,
                reason: b.reason,
            })
            .collect(),
        disabled,
        original,
    }
}

/// 从邮件的 text_plain / html 字段中取纯文本原文，供过滤剥离管道使用。
///
/// 优先级：text_plain → html（先转纯文本）→ 空串。
fn original_text(text_plain: &Option<String>, html: &Option<String>) -> String {
    match (text_plain.as_deref(), html.as_deref()) {
        (Some(t), _) => t.to_string(),
        (None, Some(h)) => crate::imap::html_text::html_to_text(h),
        (None, None) => String::new(),
    }
}

/// 实时预览当前规则会如何剥离该封邮件。固定用 summary 默认（三类全剥）展示最大剥离效果；
/// disabled 态时仍实时重算（预览态：让用户看到"若启用会剥成什么"），disabled 字段仅供 UI 标注。
#[tauri::command]
pub async fn message_filter_preview(
    state: State<'_, AppState>,
    message_id: Uuid,
) -> AppResult<MessageFilterPreview> {
    let pool = state.pool().await?;
    let ctx = crate::ai::context::load_thread_context(pool, &state.imap, message_id).await?;
    let current = ctx
        .members
        .get(ctx.current_index)
        .ok_or_else(|| AppError::Ai("会话上下文缺当前封".into()))?;
    let original = original_text(&current.text_plain, &current.html);
    let msg = messages::get(pool, message_id).await?;
    let disabled = msg.as_ref().map(|m| m.filter_disabled).unwrap_or(false);

    let prior: Vec<Option<String>> = ctx.members[..ctx.current_index]
        .iter()
        .map(|m| m.text_plain.clone())
        .collect();
    let resolved =
        crate::db::filter_rules::resolve_for(pool, current.header.from_addr.as_deref()).await?;
    // 预览固定用 summary 默认（三类全剥）展示最大剥离效果；disabled 态仍实时重算（预览态）。
    let actions = crate::ai::extract::resolve_target_actions(
        "summary",
        resolved.signature,
        resolved.quote,
        resolved.repeat,
        resolved.signature_pattern.as_deref(),
    );
    let result = crate::ai::extract::extract_increment(&original, &prior, current.is_own, &actions);
    Ok(preview_from_result(result, disabled, original))
}

/// 切 per-message 过滤开关。disabled=true 时 AI 管道跳过剥离直接用原文。
#[tauri::command]
pub async fn message_set_filter_disabled(
    state: State<'_, AppState>,
    message_id: Uuid,
    disabled: bool,
) -> AppResult<()> {
    messages::set_filter_disabled(state.pool().await?, message_id, disabled).await
}

const SETTABLE_CATEGORIES: &[&str] = &["personal", "work", "notification", "promotion", "spam"];

/// 手动设置邮件类别并锁定，AI 后续不再覆写。
/// 消息已删（0 行）时记 warn 后正常返回，与既有删除容错惯例一致。
#[tauri::command]
pub async fn message_set_category(
    state: State<'_, AppState>,
    message_id: Uuid,
    category: String,
) -> Result<(), AppError> {
    if !SETTABLE_CATEGORIES.contains(&category.as_str()) {
        return Err(AppError::Config(format!("非法类别: {category}")));
    }
    let pool = state.pool().await?;
    let rows = crate::db::messages::set_category_locked(pool, message_id, &category).await?;
    if rows == 0 {
        tracing::warn!(%message_id, "set_category 目标已删（0 行），跳过");
    }
    Ok(())
}

// ── 用户标签 ─────────────────────────────────────────────────────────────

/// 单个标签长度上限（字符数，中文友好）。
const MAX_TAG_CHARS: usize = 30;
/// 每封邮件标签总数上限（AI + 用户合计），防无界堆积。
const MAX_TAGS_PER_MESSAGE: i64 = 8;

/// 纯函数：校验并规范化用户输入的标签名（trim；非空且 ≤ MAX_TAG_CHARS 字符）。
fn normalize_tag(raw: &str) -> AppResult<String> {
    let tag = raw.trim();
    if tag.is_empty() {
        return Err(AppError::Config("标签不能为空".into()));
    }
    if tag.chars().count() > MAX_TAG_CHARS {
        return Err(AppError::Config(format!(
            "标签过长（最多 {MAX_TAG_CHARS} 字符）"
        )));
    }
    Ok(tag.to_string())
}

/// 给邮件添加用户标签。同名 AI 标签会被升级为用户标签（重新分类不再清除）。
#[tauri::command]
pub async fn message_add_tag(
    state: State<'_, AppState>,
    message_id: Uuid,
    tag: String,
) -> Result<(), AppError> {
    let tag = normalize_tag(&tag)?;
    let pool = state.pool().await?;
    if crate::db::message_tags::count_tags(pool, message_id).await? >= MAX_TAGS_PER_MESSAGE {
        return Err(AppError::Config(format!(
            "标签数已达上限（{MAX_TAGS_PER_MESSAGE}），请先删除部分标签"
        )));
    }
    let rows = crate::db::message_tags::set_user_tag(pool, message_id, &tag).await?;
    if rows == 0 {
        tracing::warn!(%message_id, "add_tag 目标已删（0 行），跳过");
    }
    Ok(())
}

/// 删除邮件的一个标签（无论 AI 还是用户来源）。
#[tauri::command]
pub async fn message_remove_tag(
    state: State<'_, AppState>,
    message_id: Uuid,
    tag: String,
) -> Result<(), AppError> {
    let tag = normalize_tag(&tag)?;
    let pool = state.pool().await?;
    crate::db::message_tags::remove_tag(pool, message_id, &tag).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::{HashMap, HashSet};
    use std::sync::Mutex as StdMutex;
    use std::time::Duration;

    use tokio::sync::{watch, Mutex};
    use uuid::Uuid;

    use super::{
        normalize_page_limit, normalize_page_offset, try_acquire_account_sync,
        validate_bulk_seen_ids, MAX_BULK_SEEN_IDS, MAX_PAGE_LIMIT,
    };
    use crate::error::AppError;

    #[test]
    fn tag_normalization_trims_limits_and_rejects_blank() {
        assert_eq!(super::normalize_tag("  报销  ").unwrap(), "报销");
        assert!(super::normalize_tag("   ").is_err(), "空白应拒绝");
        assert!(super::normalize_tag("").is_err());
        // 31 个字符超限；30 个恰好通过；中文按字符计数（非字节）。
        let long = "a".repeat(super::MAX_TAG_CHARS + 1);
        assert!(super::normalize_tag(&long).is_err());
        let ok = "a".repeat(super::MAX_TAG_CHARS);
        assert!(super::normalize_tag(&ok).is_ok());
        let cjk = "标".repeat(super::MAX_TAG_CHARS);
        assert!(super::normalize_tag(&cjk).is_ok(), "中文 30 字应在限内");
    }

    #[test]
    fn imap_already_gone_detects_not_exist() {
        let cases = [
            "no response: Mails not exist!",
            "NO Such message",
            "Message does not exist",
        ];
        for c in cases {
            assert!(
                super::imap_msg_already_gone(&AppError::Imap(c.into())),
                "should detect already-gone: {c}"
            );
        }
    }

    #[test]
    fn imap_already_gone_false_for_other_errors() {
        assert!(!super::imap_msg_already_gone(&AppError::Imap(
            "connection timeout".into()
        )));
        assert!(!super::imap_msg_already_gone(&AppError::Config(
            "not found".into()
        )));
    }

    #[test]
    fn cached_body_with_cid_image_is_stale() {
        let body = crate::db::bodies::MessageBody {
            message_id: Uuid::new_v4(),
            text_plain: None,
            html: Some(r#"<img src="cid:logo">"#.into()),
            fetched_at: time::OffsetDateTime::UNIX_EPOCH,
        };

        assert!(super::body_cache_needs_refetch(&body));
    }

    #[test]
    fn cached_body_with_non_image_cid_text_is_not_stale() {
        let body = crate::db::bodies::MessageBody {
            message_id: Uuid::new_v4(),
            text_plain: None,
            html: Some(r#"<a href="cid:note">cid link</a>"#.into()),
            fetched_at: time::OffsetDateTime::UNIX_EPOCH,
        };

        assert!(!super::body_cache_needs_refetch(&body));
    }

    /// 测试专用内存 SQLite pool（单连接、迁移已跑、外键启用）。
    async fn test_pool() -> crate::db::Pool {
        let opts = sqlx::sqlite::SqliteConnectOptions::new()
            .filename(":memory:")
            .foreign_keys(true);
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(opts)
            .await
            .unwrap();
        crate::db::MIGRATOR.run(&pool).await.unwrap();
        pool
    }

    /// #41 专项：精确复现"leader 先完成、迟到者后等待"的竞态窗口。
    ///
    /// 场景：迟到者在锁内克隆了 receiver（map 中的 rx），但释放锁到调用 borrow()/changed()
    /// 之间有调度间隙；leader 恰好在此间隙内完成 fetch、send(true) 并从 map 移除条目。
    /// 迟到者随后调用 borrow() 读到已为 true 的值，直接跳过 changed().await。
    /// 断言：迟到者不永久挂起。
    ///
    /// 与旧 Notify 机制的对比：旧机制 notify_waiters() 不为未注册者存 permit，
    /// 迟到者此后调用 notified().await 会永久阻塞；watch 持有最新值，无论先后均能拿到通知。
    #[tokio::test]
    async fn latecomer_does_not_hang_when_leader_finishes_first() {
        // 直接模拟 message_body 的 watch 机制，不走真实 IMAP。
        let (tx, rx) = watch::channel(false);

        // Step 1：迟到者在 leader 完成之前就克隆了 receiver（真实代码在锁内克隆后释放锁）。
        let mut latecomer_rx = rx.clone();

        // Step 2：leader 完成——先 send(true)，再从 map 移除（RAII guard 的顺序）。
        // 此时迟到者持有的 latecomer_rx 尚未调用 borrow()/changed()，复现竞态窗口。
        let _ = tx.send(true);
        drop(rx); // 模拟 leader 从 map 移除条目（原始 rx drop）

        // Step 3：迟到者在 leader 完成后才进入等待路径，先读 borrow()，值已为 true，
        // 直接跳过 changed().await，不永久挂起。
        let result = tokio::time::timeout(Duration::from_secs(1), async {
            if !*latecomer_rx.borrow() {
                let _ = latecomer_rx.changed().await;
            }
            *latecomer_rx.borrow()
        })
        .await;

        assert!(result.is_ok(), "迟到者在 leader 先完成后永久挂起（超时）");
        assert!(result.unwrap(), "watch 值应为 true（leader 已完成）");
    }

    /// #41 专项：leader 完成后 watch 值持久为 true，多个迟到者均能正确唤醒（不丢通知）。
    #[tokio::test]
    async fn multiple_latecomers_all_wake_after_leader_finishes() {
        let (tx, rx) = watch::channel(false);

        // 三个迟到者在 leader 完成前克隆 receiver（模拟在竞态窗口内注册）
        let rx1 = rx.clone();
        let rx2 = rx.clone();
        let rx3 = rx;

        // leader 稍后发送完成信号（给迟到者一点注册时间，但这只是为了让测试更贴近真实场景；
        // 即使 send 先于 changed() 调用，watch 持状态也能保证所有迟到者拿到通知）
        let leader = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(5)).await;
            tx.send(true).unwrap();
        });

        // 三个迟到者并发等待
        let wait = |mut rx: watch::Receiver<bool>| async move {
            if !*rx.borrow() {
                let _ = rx.changed().await;
            }
            *rx.borrow()
        };

        let result = tokio::time::timeout(Duration::from_secs(1), async {
            let (r1, r2, r3, _) = tokio::join!(wait(rx1), wait(rx2), wait(rx3), leader);
            (r1, r2, r3)
        })
        .await;

        assert!(result.is_ok(), "多个迟到者中有人永久挂起（超时）");
        let (r1, r2, r3) = result.unwrap();
        assert!(r1 && r2 && r3, "所有迟到者都应拿到 true");
    }

    /// #41 专项：RAII drop guard 在 leader panic 时也能清理 map + 通知等待者。
    #[tokio::test]
    async fn drop_guard_notifies_on_panic() {
        use super::BodyInFlightGuard;

        let map: Mutex<HashMap<Uuid, watch::Receiver<bool>>> = Mutex::new(HashMap::new());
        let id = Uuid::new_v4();

        let (tx, rx) = watch::channel(false);
        map.lock().await.insert(id, rx.clone());

        // 在独立 task 中放 guard，然后直接 drop（模拟 panic/早返回）
        let _map_ref = &map; // 借用检查：在测试内不跨 await 传引用
                             // 直接 drop guard，触发 send(true)
        {
            let _guard = BodyInFlightGuard {
                id,
                body_in_flight: &map,
                tx,
            };
            // _guard 在此离开作用域触发 drop
        }

        // map 条目应已被移除
        assert!(
            map.lock().await.get(&id).is_none(),
            "drop guard 应移除 map 条目"
        );

        // rx 的值应已为 true
        assert!(*rx.borrow(), "drop guard 应 send(true) 通知等待者");
    }

    /// 迟到者修复：leader 取正文失败（未写缓存）后，迟到者不应再返回误导性的
    /// "not in cache after in-flight fetch"，而应回退自取、透传真实失败原因。
    ///
    /// 判别力：旧实现迟到者直接 `bodies::get → None → Config("... not in cache ...")`，
    /// 下方第一个断言失败；新实现回退 `fetch_and_cache_body`，对不存在的 message 返回
    /// "message {id} not found"（真实原因），两个断言都通过。
    #[tokio::test]
    async fn latecomer_falls_back_and_surfaces_real_error_not_not_in_cache() {
        let pool = test_pool().await;
        let imap = crate::imap::manager::ImapManager::new();
        // 该 message 不存在 → 回退自取会在 messages::get 处得到 "message not found"。
        let id = Uuid::new_v4();

        // 预置 single-flight 条目并标记 leader 已完成（watch=true），复现「迟到者」分支。
        let body_in_flight: Mutex<HashMap<Uuid, watch::Receiver<bool>>> =
            Mutex::new(HashMap::new());
        let (_tx, rx) = watch::channel(true);
        body_in_flight.lock().await.insert(id, rx);

        let err = super::message_body_impl(&pool, &imap, &body_in_flight, id)
            .await
            .expect_err("不存在的 message 应返回错误");
        let msg = err.to_string();

        assert!(
            !msg.contains("not in cache"),
            "迟到者应透传真实原因而非误导性 not-in-cache，实际：{msg}"
        );
        assert!(
            msg.contains("not found"),
            "应透传回退自取的真实失败原因（message not found），实际：{msg}"
        );
    }

    #[test]
    fn page_limit_is_clamped_and_offset_is_validated() {
        assert_eq!(normalize_page_limit(50).unwrap(), 50);
        assert_eq!(normalize_page_limit(10_000).unwrap(), MAX_PAGE_LIMIT);
        assert!(matches!(normalize_page_limit(0), Err(AppError::Config(_))));
        assert_eq!(normalize_page_offset(0).unwrap(), 0);
        assert!(matches!(
            normalize_page_offset(-1),
            Err(AppError::Config(_))
        ));
    }

    #[test]
    fn bulk_seen_ids_have_a_command_layer_cap() {
        let ids: Vec<Uuid> = (0..=MAX_BULK_SEEN_IDS).map(|_| Uuid::new_v4()).collect();
        assert!(matches!(
            validate_bulk_seen_ids(&ids),
            Err(AppError::Config(_))
        ));
    }

    #[tokio::test]
    async fn account_sync_guard_rejects_same_account_until_released() {
        let in_flight: StdMutex<HashSet<Uuid>> = StdMutex::new(HashSet::new());
        let account_id = Uuid::new_v4();

        let first = try_acquire_account_sync(&in_flight, account_id)
            .expect("first sync should acquire guard");
        let same = try_acquire_account_sync(&in_flight, account_id)
            .expect_err("same account should be rejected while sync is in flight");
        assert!(
            same.to_string().contains("正在同步"),
            "error should be user-actionable, got: {same}"
        );

        let other = try_acquire_account_sync(&in_flight, Uuid::new_v4())
            .expect("different account should not be blocked");
        drop(other);
        drop(first);

        let reacquired = try_acquire_account_sync(&in_flight, account_id)
            .expect("guard drop should release the account");
        drop(reacquired);
    }

    #[test]
    fn attachment_default_filename_strips_path_segments_and_blanks() {
        assert_eq!(
            super::attachment_default_filename(Some("../secret.pdf")),
            "secret.pdf"
        );
        assert_eq!(
            super::attachment_default_filename(Some("C:\\Users\\alice\\report.xlsx")),
            "report.xlsx"
        );
        assert_eq!(
            super::attachment_default_filename(Some("   ")),
            "attachment.bin"
        );
        assert_eq!(super::attachment_default_filename(None), "attachment.bin");
    }

    #[test]
    fn preview_maps_removed_blocks_and_flags() {
        use crate::ai::extract::{ExtractResult, RemovedBlock, Target};
        let result = ExtractResult {
            net: "净增量".into(),
            removed: vec![RemovedBlock {
                kind: Target::Signature,
                text: "-- \nAlice".into(),
                reason: "签名".into(),
            }],
        };
        let p = super::preview_from_result(result, true, "完整原文".into());
        assert_eq!(p.net, "净增量");
        assert_eq!(p.removed.len(), 1);
        assert_eq!(p.removed[0].kind, "signature");
        assert!(p.disabled);
        assert_eq!(p.original, "完整原文");
    }

    /// filter_preview_html: 纯 HTML 邮件（text_plain=None）喂入 original_text 后
    /// 不应含 HTML 标签，且应保留正文中文字。
    #[test]
    fn filter_preview_html_strips_tags_keeps_content() {
        let text_plain: Option<String> = None;
        let html: Option<String> = Some("<div>正文<br>签名分隔</div>".into());
        let result = super::original_text(&text_plain, &html);
        assert!(
            !result.contains('<'),
            "原文不应含 HTML 标签，实际：{result:?}"
        );
        assert!(
            result.contains("正文"),
            "原文应保留正文内容，实际：{result:?}"
        );
    }

    /// filter_preview_html: text_plain 有值时直接用之，不解析 html。
    #[test]
    fn filter_preview_html_prefers_text_plain() {
        let text_plain: Option<String> = Some("纯文本正文".into());
        let html: Option<String> = Some("<div>HTML正文</div>".into());
        let result = super::original_text(&text_plain, &html);
        assert_eq!(result, "纯文本正文");
    }

    /// filter_preview_html: 两者均为 None 返回空串。
    #[test]
    fn filter_preview_html_both_none_returns_empty() {
        let text_plain: Option<String> = None;
        let html: Option<String> = None;
        let result = super::original_text(&text_plain, &html);
        assert!(result.is_empty(), "两者均无时应返回空串，实际：{result:?}");
    }
}

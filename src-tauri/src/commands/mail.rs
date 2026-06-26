//! Mail commands: sync, listing, detail, lazy body fetch.
//!
//! AI commands move into separate command modules in Sprints 2+.

use std::sync::Arc;

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
pub async fn inbox_sync(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    account_id: Uuid,
) -> AppResult<SyncReport> {
    let pool = state.pool().await?;
    let account = db::accounts::get(pool, account_id)
        .await?
        .ok_or_else(|| AppError::Config(format!("account {account_id} not found")))?;

    let auth = tokio::task::spawn_blocking(move || keychain::get_auth_code(account_id))
        .await
        .map_err(|e| AppError::Other(anyhow::anyhow!(e)))??;

    sync::sync_inbox(
        pool,
        &account,
        &auth,
        state.cancel.clone(),
        Arc::clone(&state.account_tokens),
        app,
    )
    .await
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
#[tauri::command]
pub async fn mailbox_sync(
    state: State<'_, AppState>,
    account_id: Uuid,
    mailbox_name: String,
) -> AppResult<SyncReport> {
    let pool = state.pool().await?;
    let account = db::accounts::get(pool, account_id)
        .await?
        .ok_or_else(|| AppError::Config(format!("account {account_id} not found")))?;

    let auth = tokio::task::spawn_blocking(move || keychain::get_auth_code(account_id))
        .await
        .map_err(|e| AppError::Other(anyhow::anyhow!(e)))??;

    sync::sync_mailbox(pool, &account, &auth, &mailbox_name).await
}

#[tauri::command]
pub async fn messages_list(
    state: State<'_, AppState>,
    mailbox_id: Uuid,
    limit: i64,
    offset: i64,
) -> AppResult<Vec<MessageHeader>> {
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
    message_body_impl(state.pool().await?, &state.body_in_flight, id).await
}

/// `message_body` 的可测内核：把 `State` 依赖拆成显式参数（pool + single-flight map），
/// 便于在单元测试里注入并复现「迟到者」分支，无需构造完整 `AppState`。
async fn message_body_impl(
    pool: &db::Pool,
    body_in_flight: &tokio::sync::Mutex<
        std::collections::HashMap<Uuid, tokio::sync::watch::Receiver<bool>>,
    >,
    id: Uuid,
) -> AppResult<MessageBody> {
    // 快路径：缓存命中直接返回。
    if let Some(body) = bodies::get(pool, id).await? {
        return Ok(body);
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

            return fetch_and_cache_body(pool, id).await;
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
    fetch_and_cache_body(pool, id).await
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

/// IMAP 连接 → select → `UID FETCH BODY[]`，返回该消息的完整 RFC822 原文。
/// 供 `fetch_and_cache_body`（正文）与附件命令（列附件 / 取附件字节）复用。
async fn fetch_raw_body(db: &db::Pool, id: Uuid) -> AppResult<Vec<u8>> {
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
    let raw = client.uid_fetch_body(uid).await?;
    if let Err(e) = client.logout().await {
        tracing::warn!(error = ?e, "imap logout failed (non-fatal)");
    }
    Ok(raw)
}

/// IMAP 取 body 并持久化到 DB，供 `message_body` 调用。
async fn fetch_and_cache_body(db: &db::Pool, id: Uuid) -> AppResult<MessageBody> {
    let msg = messages::get(db, id)
        .await?
        .ok_or_else(|| AppError::Config(format!("message {id} not found")))?;
    let account = db::accounts::get(db, msg.account_id)
        .await?
        .ok_or_else(|| AppError::Config(format!("account {} not found", msg.account_id)))?;
    let account_id = account.id;
    let auth = tokio::task::spawn_blocking(move || keychain::get_auth_code(account_id))
        .await
        .map_err(|e| AppError::Other(anyhow::anyhow!(e)))??;
    crate::imap::materialize::materialize_one(db, &account, &auth, id).await?;
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
    let raw = fetch_raw_body(state.pool().await?, id).await?;
    Ok(parse::parse_attachments(&raw))
}

/// 把某邮件第 `index` 个附件写入用户「另存为」选定的 `dest` 路径。用户主动下载到自选位置
/// （dialog 已授权），故允许写出 app 数据目录之外。越界返回错误。
#[tauri::command]
pub async fn message_attachment_save(
    state: State<'_, AppState>,
    id: Uuid,
    index: usize,
    dest: String,
) -> AppResult<()> {
    let raw = fetch_raw_body(state.pool().await?, id).await?;
    let bytes = parse::extract_attachment_bytes(&raw, index)
        .ok_or_else(|| AppError::Config(format!("attachment {index} not found")))?;
    tokio::task::spawn_blocking(move || std::fs::write(&dest, bytes))
        .await
        .map_err(|e| AppError::Other(anyhow::anyhow!(e)))?
        .map_err(|e| AppError::Config(format!("写入附件失败：{e}")))?;
    Ok(())
}

#[tauri::command]
pub async fn smtp_send(state: State<'_, AppState>, draft: SendDraft) -> AppResult<SendReceipt> {
    smtp::send_draft(state.pool().await?, &draft).await
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

    // 原子更新本地 flags：IMAP 往返成功后直接在 DB 内做单 flag add/remove，
    // 避免读-改-写并发竞争（#30）。
    messages::update_flag_atomic(db, id, flag, add).await
}

#[tauri::command]
pub async fn message_set_seen(state: State<'_, AppState>, id: Uuid, seen: bool) -> AppResult<()> {
    set_flag_impl(state.pool().await?, id, "\\Seen", seen).await
}

#[tauri::command]
pub async fn message_set_flagged(
    state: State<'_, AppState>,
    id: Uuid,
    flagged: bool,
) -> AppResult<()> {
    set_flag_impl(state.pool().await?, id, "\\Flagged", flagged).await
}

/// 批量标 `\Seen`（「全部已读」）。`ids` 可跨账户/信箱：按 (account, mailbox) 分组，每组一次
/// IMAP `UID STORE +FLAGS \Seen`，成功后本地批量标。某组失败即返回错误（已成功组的本地 flags
/// 已更新，前端 reload 会反映真实状态）。空 `ids` 直接返回。
#[tauri::command]
pub async fn messages_mark_seen_bulk(state: State<'_, AppState>, ids: Vec<Uuid>) -> AppResult<()> {
    use std::collections::HashMap;

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
        let auth = tokio::task::spawn_blocking(move || keychain::get_auth_code(account_id))
            .await
            .map_err(|e| AppError::Other(anyhow::anyhow!(e)))??;
        let port = u16::try_from(account.imap_port)
            .map_err(|_| AppError::Imap(format!("invalid imap_port: {}", account.imap_port)))?;

        let mut client =
            ImapClient::connect(&account.imap_host, port, &account.email, &auth).await?;
        client.select(&mailbox.name).await?;
        client.uid_set_flag_bulk(&uids, "\\Seen", true).await?;
        if let Err(e) = client.logout().await {
            tracing::warn!(error = ?e, "imap logout failed (non-fatal)");
        }

        // IMAP 成功 → 本地批量标 \Seen（持久化）。
        for id in msg_ids {
            messages::update_flag_atomic(pool, id, "\\Seen", true).await?;
        }
    }
    Ok(())
}

/// 删除 = 移到废纸篓（可恢复）。move 成功即逻辑成功；本地 remove 失败仅 warn 返 Ok
/// （服务端权威态已变，宁留极罕见幽灵行也不让用户看到删除回退）。
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

    if let Err(e) = messages::remove(pool, id).await {
        tracing::warn!(message_id = %id, error = ?e, "local remove after trash-move failed (non-fatal)");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::time::Duration;

    use tokio::sync::{watch, Mutex};
    use uuid::Uuid;

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
        // 该 message 不存在 → 回退自取会在 messages::get 处得到 "message not found"。
        let id = Uuid::new_v4();

        // 预置 single-flight 条目并标记 leader 已完成（watch=true），复现「迟到者」分支。
        let body_in_flight: Mutex<HashMap<Uuid, watch::Receiver<bool>>> =
            Mutex::new(HashMap::new());
        let (_tx, rx) = watch::channel(true);
        body_in_flight.lock().await.insert(id, rx);

        let err = super::message_body_impl(&pool, &body_in_flight, id)
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
}

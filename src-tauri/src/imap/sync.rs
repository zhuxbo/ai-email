//! Orchestrates IMAP mailbox sync.
//!
//! Flow (for any mailbox, parameterized via `sync_mailbox`):
//!   1. IMAPS connect → login → ID
//!   2. LIST every folder + upsert into `mailboxes` (with special_use detection)
//!   3. SELECT <mailbox> → grab `exists` / `uid_next` / `uid_validity`
//!   4. FETCH range:
//!      - First sync (no prior `uid_next`): seq range `(exists-49):*` — guarantees ≤50 rows.
//!      - Incremental: `UID FETCH <prev_uid_next>:*` — gets everything new.
//!   5. Parse headers + INSERT … ON CONFLICT DO NOTHING (idempotent on retries)
//!   6. Bookkeeping: `mailboxes.uid_next` / `last_synced_at`, `accounts.last_synced_at`
//!   7. LOGOUT (best-effort — already-committed sync isn't undone by logout failure)
//!
//! `sync_inbox` is a convenience wrapper around `sync_mailbox("INBOX")`.
//! Non-INBOX syncs (Sent, Drafts, Trash, etc.) are triggered on-demand when the user
//! navigates to that mailbox — we do not bulk-sync all mailboxes automatically.
//!
//! After returning the SyncReport (INBOX only), new messages are classified and
//! auto-reply rules are evaluated in a detached tokio::spawn:
//!   - `mail://classified`    — background classify finished (payload: ClassifiedPayload)
//!   - `autoreply://updated`  — evaluate_rules finished (payload: AutoReplyPayload)
//!
//! Body, snippet, `has_attachment`, `internal_date` are left blank — Sprint 1.4.

use secrecy::SecretString;
use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tokio_util::sync::CancellationToken;

use crate::db::accounts::Account;
use crate::db::messages::MessageInsert;
use crate::db::{accounts, mailboxes, messages, Pool};
use crate::error::{AppError, AppResult};
use crate::imap::client::ImapClient;
use crate::imap::parse;

/// Payload for the `mail://classified` event.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClassifiedPayload {
    pub account_id: uuid::Uuid,
    pub count: usize,
}

/// Payload for the `autoreply://updated` event.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoReplyPayload {
    pub account_id: uuid::Uuid,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncReport {
    pub new_message_count: i32,
    pub total_in_mailbox: i64,
}

/// How this sync should fetch, decided purely from stored vs. server UIDVALIDITY.
///
/// Pure so it can be unit-tested without touching the network — see `decide_sync_mode`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncMode {
    /// No prior `uid_next` stored — fetch the most recent window by sequence number.
    FirstSync,
    /// Stored UIDVALIDITY matches the server: incremental `UID FETCH <prev>:*`.
    Incremental { prev_uid_next: i64 },
    /// Stored UIDVALIDITY differs from the server (RFC 3501 §2.3.1.1): the mailbox was
    /// rebuilt and every cached UID is now invalid. Drop local rows and re-fetch from scratch.
    ResetRefetch,
}

/// Decide the fetch strategy for one mailbox sync.
///
/// Inputs are the locally-stored `uid_validity` / `uid_next` and the freshly-SELECTed server
/// `uid_validity`. Rules (RFC 3501):
///   - No stored `uid_next` → first sync (regardless of validity).
///   - Stored validity present, server validity present, and they differ → mailbox reset.
///   - Otherwise (validities equal, or either side absent) → incremental from `prev_uid_next`.
///
/// We only treat it as a reset when *both* sides carry a validity and they disagree; a missing
/// server validity (minimal SELECT response) must never nuke the local cache.
pub fn decide_sync_mode(
    local_uid_validity: Option<i64>,
    local_uid_next: Option<i64>,
    server_uid_validity: Option<i64>,
) -> SyncMode {
    let Some(prev_uid_next) = local_uid_next else {
        return SyncMode::FirstSync;
    };
    if let (Some(local), Some(server)) = (local_uid_validity, server_uid_validity) {
        if local != server {
            return SyncMode::ResetRefetch;
        }
    }
    SyncMode::Incremental { prev_uid_next }
}

/// Sync a specific mailbox by name. Reusable core shared by `sync_inbox` and the
/// on-demand `mailbox_sync` command.
///
/// - Lists all mailboxes and upserts them (with special_use) on every call.
/// - Applies UIDVALIDITY change detection and incremental sync logic.
/// - Returns `(SyncReport, new_message_ids)` so the caller can decide whether
///   to kick off background AI tasks.
async fn sync_mailbox_inner(
    pool: &Pool,
    account: &Account,
    auth_code: &SecretString,
    mailbox_name: &str,
) -> AppResult<(SyncReport, Vec<uuid::Uuid>)> {
    let port = u16::try_from(account.imap_port)
        .map_err(|_| AppError::Imap(format!("invalid imap_port: {}", account.imap_port)))?;

    tracing::info!(
        account_id = %account.id,
        mailbox = mailbox_name,
        "mailbox sync starting"
    );
    let mut client =
        ImapClient::connect(&account.imap_host, port, &account.email, auth_code).await?;

    for info in client.list_mailboxes().await? {
        mailboxes::upsert(pool, account.id, &info).await?;
    }

    let selected = client.select(mailbox_name).await?;
    let mailbox = mailboxes::get_by_name(pool, account.id, mailbox_name)
        .await?
        .ok_or_else(|| AppError::Imap(format!("{mailbox_name} not found after upsert")))?;

    // Decide fetch strategy based on stored vs. server UIDVALIDITY (audit #2).
    let mode = decide_sync_mode(
        mailbox.uid_validity,
        mailbox.uid_next,
        selected.uid_validity.map(i64::from),
    );
    tracing::debug!(account_id = %account.id, mailbox = mailbox_name, ?mode, "sync mode decided");

    // UIDVALIDITY mismatch: drop stale local rows and reset bookkeeping before re-fetching.
    if mode == SyncMode::ResetRefetch {
        let new_validity = selected
            .uid_validity
            .map(i64::from)
            .ok_or_else(|| AppError::Imap("server reported no UIDVALIDITY on reset path".into()))?;
        tracing::warn!(
            account_id = %account.id,
            mailbox = mailbox_name,
            old_validity = ?mailbox.uid_validity,
            new_validity,
            "UIDVALIDITY changed — resetting local cache"
        );
        mailboxes::reset_mailbox_for_uidvalidity_change(pool, mailbox.id, new_validity).await?;
    }

    let fetched = if selected.exists == 0 {
        Vec::new()
    } else {
        match mode {
            SyncMode::Incremental { prev_uid_next } => {
                client
                    .uid_fetch_headers(&format!("{prev_uid_next}:*"))
                    .await?
            }
            SyncMode::FirstSync | SyncMode::ResetRefetch => {
                let lower = selected.exists.saturating_sub(49).max(1);
                client.fetch_headers(&format!("{lower}:*")).await?
            }
        }
    };

    // Pre-parse all headers so the transaction holds the DB lock for as short as possible
    // (parsing is CPU-only, no I/O).
    let parsed: Vec<_> = fetched
        .iter()
        .map(|fh| (fh, parse::parse_headers(&fh.header_bytes)))
        .collect();

    // Collect which existing UIDs need flag refreshes — resolved outside the transaction
    // so we don't hold the write lock during the incremental flag-update UPDATEs.
    let mut flag_updates: Vec<(i64, &[String])> = Vec::new();

    let mut inserted = 0_i32;
    let mut new_ids: Vec<uuid::Uuid> = Vec::with_capacity(fetched.len());

    // Wrap all inserts in a single transaction (audit #66): eliminates N+1 auto-commits and
    // makes the batch atomic — a partial sync failure rolls back cleanly.
    {
        let mut tx = pool.begin().await?;
        for (fh, h) in &parsed {
            let new_id = messages::insert_tx(
                &mut *tx,
                &MessageInsert {
                    account_id: account.id,
                    mailbox_id: mailbox.id,
                    imap_uid: i64::from(fh.uid),
                    rfc_message_id: h.rfc_message_id.clone(),
                    thread_id: h.thread_id.clone(),
                    subject: h.subject.clone(),
                    from_addr: h.from_addr.clone(),
                    to_addrs: h.to_addrs.clone(),
                    cc_addrs: h.cc_addrs.clone(),
                    sent_at: h.sent_at,
                    internal_date: None,
                    flags: fh.flags.clone(),
                    size_bytes: fh.size_bytes,
                    has_attachment: false,
                    snippet: None,
                    references_header: h.references_header.clone(),
                },
            )
            .await?;
            if let Some(id) = new_id {
                inserted += 1;
                new_ids.push(id);
            } else if matches!(mode, SyncMode::Incremental { .. }) {
                flag_updates.push((i64::from(fh.uid), &fh.flags));
            }
        }
        tx.commit().await?;
    }

    // Apply flag refreshes for already-present UIDs after the insert transaction commits.
    // These are independent UPDATEs — they don't need to be atomic with the inserts.
    for (uid, flags) in flag_updates {
        // Existing UID seen again in an incremental window: refresh its flags so
        // read/starred state stays in sync with the server (audit #64).
        messages::update_flags_by_uid(pool, mailbox.id, uid, flags).await?;
    }

    mailboxes::update_after_sync(
        pool,
        mailbox.id,
        selected.uid_next.map(i64::from),
        selected.uid_validity.map(i64::from),
    )
    .await?;
    accounts::update_last_synced(pool, account.id).await?;

    if let Err(e) = client.logout().await {
        tracing::warn!(error = ?e, "imap logout failed (non-fatal)");
    }

    tracing::info!(
        account_id = %account.id,
        mailbox = mailbox_name,
        inserted,
        total_in_mailbox = selected.exists,
        "mailbox sync done"
    );

    Ok((
        SyncReport {
            new_message_count: inserted,
            total_in_mailbox: i64::from(selected.exists),
        },
        new_ids,
    ))
}

/// Sync INBOX and kick off background AI classification + auto-reply evaluation.
pub async fn sync_inbox(
    pool: &Pool,
    account: &Account,
    auth_code: &SecretString,
    cancel: CancellationToken,
    app_handle: AppHandle,
) -> AppResult<SyncReport> {
    let (report, new_ids) = sync_mailbox_inner(pool, account, auth_code, "INBOX").await?;

    // Kick off background classification for the freshly-landed rows. We don't await — UI
    // gets the sync report immediately and is notified via Tauri events when background
    // work finishes (see mail://classified and autoreply://updated).
    // Errors here are non-fatal: log and move on. The classifier checks `ai_role_defaults`
    // and silently no-ops when the role isn't configured yet.
    //
    // #29: child_token 继承自应用级 cancel，退出时自动取消，停止进行中的付费 AI 调用。
    // #23: app_handle move 进闭包，完成节点 emit 事件通知前端，取代固定计时器盲轮询。
    if !new_ids.is_empty() {
        let pool_clone = pool.clone();
        let account_id = account.id;
        let child_token = cancel.child_token();
        tokio::spawn(async move {
            // 在任何 AI 调用前检查取消：若应用已退出则跳过整批任务。
            if child_token.is_cancelled() {
                tracing::debug!(account_id = %account_id, "classify cancelled before start");
                return;
            }

            let classify_result = tokio::select! {
                result = crate::ai::classify::classify_message_ids(&pool_clone, &new_ids) => {
                    Some(result)
                }
                _ = child_token.cancelled() => {
                    tracing::info!(account_id = %account_id, "background classify cancelled (app exit)");
                    None
                }
            };

            match classify_result {
                Some(Ok(results)) => {
                    tracing::info!(
                        account_id = %account_id,
                        classified = results.len(),
                        "background classify finished"
                    );
                    // 通知前端刷新邮件列表（category/priority 已写回）。
                    // emit 失败不影响业务流程，只记 warn。
                    if let Err(e) = app_handle.emit(
                        "mail://classified",
                        ClassifiedPayload {
                            account_id,
                            count: results.len(),
                        },
                    ) {
                        tracing::warn!(
                            account_id = %account_id,
                            error = %e,
                            "emit mail://classified failed (non-fatal)"
                        );
                    }
                }
                Some(Err(e)) => {
                    // Most common cause: user hasn't configured a classify model yet.
                    // We don't surface to UI — the next sync will retry, and the user can
                    // see uncategorised messages and pick a model when they're ready.
                    tracing::warn!(account_id = %account_id, error = %e, "background classify failed");
                    return; // classify 失败则不再评估规则（依赖 category 写回）
                }
                None => return, // 已取消，跳过后续 evaluate_rules
            }

            // classify 之后顺序评估自动回复规则（须在同一 spawn 内、await 之后——
            // 否则读不到刚写回的 category/priority）。失败仅 warn，不影响同步主流程。
            let eval_result = tokio::select! {
                result = crate::auto_reply::evaluate_rules(&pool_clone, account_id, &new_ids) => {
                    Some(result)
                }
                _ = child_token.cancelled() => {
                    tracing::info!(account_id = %account_id, "evaluate_rules cancelled (app exit)");
                    None
                }
            };
            match eval_result {
                Some(Ok(())) => {
                    // 通知前端刷新建议回复队列。
                    if let Err(e) =
                        app_handle.emit("autoreply://updated", AutoReplyPayload { account_id })
                    {
                        tracing::warn!(
                            account_id = %account_id,
                            error = %e,
                            "emit autoreply://updated failed (non-fatal)"
                        );
                    }
                }
                Some(Err(e)) => {
                    tracing::warn!(account_id = %account_id, error = %e, "auto-reply rule eval failed");
                }
                None => {} // 已取消
            }
        });
    }

    Ok(report)
}

/// Sync a specific mailbox on demand (called when the user navigates to a non-INBOX folder).
/// Does NOT kick off AI classification or auto-reply evaluation — those are INBOX-only.
/// The frontend refresh is handled by the caller via the `mailboxSync(...).then(reload)` promise
/// chain in `selectMailbox` — no event emission needed here.
pub async fn sync_mailbox(
    pool: &Pool,
    account: &Account,
    auth_code: &SecretString,
    mailbox_name: &str,
) -> AppResult<SyncReport> {
    let (report, _new_ids) = sync_mailbox_inner(pool, account, auth_code, mailbox_name).await?;
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::{decide_sync_mode, AutoReplyPayload, ClassifiedPayload, SyncMode};

    #[test]
    fn first_sync_when_no_local_uid_next() {
        // 本地从未同步过：无 uid_next → 首次同步，validity 无论如何都不影响判定。
        assert_eq!(decide_sync_mode(None, None, Some(1)), SyncMode::FirstSync);
        assert_eq!(
            decide_sync_mode(Some(1), None, Some(1)),
            SyncMode::FirstSync
        );
    }

    #[test]
    fn incremental_when_validity_matches() {
        assert_eq!(
            decide_sync_mode(Some(42), Some(100), Some(42)),
            SyncMode::Incremental { prev_uid_next: 100 }
        );
    }

    #[test]
    fn reset_when_validity_differs() {
        // 服务端 mailbox 重建：validity 变了 → 必须丢弃旧 UID 重拉，绝不能用旧 uid_next 增量。
        assert_eq!(
            decide_sync_mode(Some(42), Some(100), Some(99)),
            SyncMode::ResetRefetch
        );
    }

    #[test]
    fn incremental_when_server_validity_absent() {
        // 极简 SELECT 响应缺 UIDVALIDITY：不得据此判定重置，退回增量（保留本地缓存）。
        assert_eq!(
            decide_sync_mode(Some(42), Some(100), None),
            SyncMode::Incremental { prev_uid_next: 100 }
        );
    }

    #[test]
    fn incremental_when_local_validity_absent_but_uid_next_present() {
        // 历史数据：有 uid_next 但 validity 曾为空。无从比较 → 增量，下次写回补上 validity。
        assert_eq!(
            decide_sync_mode(None, Some(100), Some(42)),
            SyncMode::Incremental { prev_uid_next: 100 }
        );
    }

    /// #21 payload serde 契约：前端依赖 camelCase 字段名，rename_all 不得被静默移除。
    #[test]
    fn classified_payload_serializes_camel_case() {
        let id = uuid::Uuid::new_v4();
        let payload = ClassifiedPayload {
            account_id: id,
            count: 3,
        };
        let v = serde_json::to_value(&payload).expect("serialize ClassifiedPayload");
        // 前端读 accountId（camelCase）——若字段名变成 account_id 则事件回调静默收不到数据
        assert!(
            v.get("accountId").is_some(),
            "accountId must be present (camelCase)"
        );
        assert!(v.get("account_id").is_none(), "snake_case must NOT appear");
        assert_eq!(v["accountId"], id.to_string());
        assert_eq!(v["count"], 3);
    }

    #[test]
    fn autoreply_payload_serializes_camel_case() {
        let id = uuid::Uuid::new_v4();
        let payload = AutoReplyPayload { account_id: id };
        let v = serde_json::to_value(&payload).expect("serialize AutoReplyPayload");
        assert!(
            v.get("accountId").is_some(),
            "accountId must be present (camelCase)"
        );
        assert!(v.get("account_id").is_none(), "snake_case must NOT appear");
        assert_eq!(v["accountId"], id.to_string());
    }

    /// #29: 预先取消的 child token 会跳过后台任务体，select! 分支立即走取消路径。
    #[tokio::test]
    async fn cancelled_token_skips_background_work() {
        use std::sync::{
            atomic::{AtomicUsize, Ordering},
            Arc,
        };
        use tokio_util::sync::CancellationToken;

        let parent = CancellationToken::new();
        let child = parent.child_token();
        // 先取消父令牌，child_token 同步取消
        parent.cancel();

        let call_count = Arc::new(AtomicUsize::new(0));
        let call_count_clone = Arc::clone(&call_count);

        // 模拟 spawn 内部的 tokio::select! 逻辑：已取消时跳过 AI 调用
        let handle = tokio::spawn(async move {
            if child.is_cancelled() {
                // 早检：退出前立即跳过，不增加计数
                return;
            }
            // 若无早检，select! 也会在取消分支胜出
            tokio::select! {
                _ = async { call_count_clone.fetch_add(1, Ordering::SeqCst); } => {}
                _ = child.cancelled() => {}
            }
        });
        handle.await.unwrap();

        // 令牌在任务开始前已取消，call_count 应为 0（work 被跳过）
        assert_eq!(
            call_count.load(Ordering::SeqCst),
            0,
            "已取消的 token 应跳过后台工作"
        );
    }
}

//! Orchestrates one full INBOX sync.
//!
//! Flow:
//!   1. IMAPS connect → login → ID
//!   2. LIST every folder + upsert into `mailboxes` (UI will need this for non-INBOX later)
//!   3. SELECT INBOX → grab `exists` / `uid_next` / `uid_validity`
//!   4. FETCH range:
//!      - First sync (no prior `uid_next`): seq range `(exists-49):*` — guarantees ≤50 rows.
//!      - Incremental: `UID FETCH <prev_uid_next>:*` — gets everything new.
//!   5. Parse headers + INSERT … ON CONFLICT DO NOTHING (idempotent on retries)
//!   6. Bookkeeping: `mailboxes.uid_next` / `last_synced_at`, `accounts.last_synced_at`
//!   7. LOGOUT (best-effort — already-committed sync isn't undone by logout failure)
//!
//! Body, snippet, `has_attachment`, `internal_date` are left blank — Sprint 1.4.

use secrecy::SecretString;
use serde::Serialize;

use crate::db::accounts::Account;
use crate::db::messages::MessageInsert;
use crate::db::{accounts, mailboxes, messages, Pool};
use crate::error::{AppError, AppResult};
use crate::imap::client::ImapClient;
use crate::imap::parse;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncReport {
    pub new_message_count: i32,
    pub total_in_mailbox: i64,
}

pub async fn sync_inbox(
    pool: &Pool,
    account: &Account,
    auth_code: &SecretString,
) -> AppResult<SyncReport> {
    let port = u16::try_from(account.imap_port)
        .map_err(|_| AppError::Imap(format!("invalid imap_port: {}", account.imap_port)))?;

    tracing::info!(account_id = %account.id, host = %account.imap_host, "inbox sync starting");
    let mut client =
        ImapClient::connect(&account.imap_host, port, &account.email, auth_code).await?;

    for info in client.list_mailboxes().await? {
        mailboxes::upsert(pool, account.id, &info).await?;
    }

    let selected = client.select("INBOX").await?;
    let inbox = mailboxes::get_by_name(pool, account.id, "INBOX")
        .await?
        .ok_or_else(|| AppError::Imap("INBOX not found after upsert".into()))?;

    let fetched = if selected.exists == 0 {
        Vec::new()
    } else if let Some(prev) = inbox.uid_next {
        client.uid_fetch_headers(&format!("{prev}:*")).await?
    } else {
        let lower = selected.exists.saturating_sub(49).max(1);
        client.fetch_headers(&format!("{lower}:*")).await?
    };

    let mut inserted = 0_i32;
    for fh in &fetched {
        let h = parse::parse_headers(&fh.header_bytes);
        let new_row = messages::insert(
            pool,
            &MessageInsert {
                account_id: account.id,
                mailbox_id: inbox.id,
                imap_uid: i64::from(fh.uid),
                rfc_message_id: h.rfc_message_id,
                thread_id: h.thread_id,
                subject: h.subject,
                from_addr: h.from_addr,
                to_addrs: h.to_addrs,
                cc_addrs: h.cc_addrs,
                sent_at: h.sent_at,
                internal_date: None,
                flags: fh.flags.clone(),
                size_bytes: fh.size_bytes,
                has_attachment: false,
                snippet: None,
            },
        )
        .await?;
        if new_row {
            inserted += 1;
        }
    }

    mailboxes::update_after_sync(
        pool,
        inbox.id,
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
        inserted,
        total_in_mailbox = selected.exists,
        "inbox sync done"
    );
    Ok(SyncReport {
        new_message_count: inserted,
        total_in_mailbox: i64::from(selected.exists),
    })
}

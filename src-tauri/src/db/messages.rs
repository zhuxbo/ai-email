//! Repository for the `messages` table — header-only rows. Bodies live in `message_bodies`
//! and are populated lazily on the first detail-view click (Sprint 1.4).
//!
//! IMAP UIDs are u32 on the wire but stored as BIGINT in PG (room for higher-UID providers).
//! The `i64` ↔ `u32` cast at the boundary is widening so always safe.

use serde::Serialize;
use sqlx::FromRow;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::db::Pool;
use crate::error::AppResult;

#[derive(Debug, Clone, Serialize, FromRow)]
#[serde(rename_all = "camelCase")]
pub struct MessageHeader {
    pub id: Uuid,
    pub account_id: Uuid,
    pub mailbox_id: Uuid,
    pub imap_uid: i64,
    pub rfc_message_id: Option<String>,
    pub thread_id: Option<String>,
    pub subject: Option<String>,
    pub from_addr: Option<String>,
    pub to_addrs: Vec<String>,
    pub cc_addrs: Vec<String>,
    #[serde(with = "time::serde::rfc3339::option")]
    pub sent_at: Option<OffsetDateTime>,
    #[serde(with = "time::serde::rfc3339::option")]
    pub internal_date: Option<OffsetDateTime>,
    pub flags: Vec<String>,
    pub size_bytes: Option<i32>,
    pub has_attachment: bool,
    pub snippet: Option<String>,
    pub priority: Option<i32>,
    #[serde(with = "time::serde::rfc3339::option")]
    pub body_fetched_at: Option<OffsetDateTime>,
}

/// Owned struct passed to `insert`. Separate from `MessageHeader` so callers don't need to
/// invent values for the DB-generated columns (`id`, `priority`, `body_fetched_at`).
#[derive(Debug, Clone)]
pub struct MessageInsert {
    pub account_id: Uuid,
    pub mailbox_id: Uuid,
    pub imap_uid: i64,
    pub rfc_message_id: Option<String>,
    pub thread_id: Option<String>,
    pub subject: Option<String>,
    pub from_addr: Option<String>,
    pub to_addrs: Vec<String>,
    pub cc_addrs: Vec<String>,
    pub sent_at: Option<OffsetDateTime>,
    pub internal_date: Option<OffsetDateTime>,
    pub flags: Vec<String>,
    pub size_bytes: Option<i32>,
    pub has_attachment: bool,
    pub snippet: Option<String>,
}

/// `INSERT … ON CONFLICT DO NOTHING`. Returns true iff a new row landed — sync uses the bool
/// to count "new messages" for the SyncReport.
pub async fn insert(pool: &Pool, m: &MessageInsert) -> AppResult<bool> {
    let result = sqlx::query(
        r#"
        INSERT INTO messages (
            account_id, mailbox_id, imap_uid, rfc_message_id, thread_id,
            subject, from_addr, to_addrs, cc_addrs, sent_at,
            internal_date, flags, size_bytes, has_attachment, snippet
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15)
        ON CONFLICT (account_id, mailbox_id, imap_uid) DO NOTHING
        "#,
    )
    .bind(m.account_id)
    .bind(m.mailbox_id)
    .bind(m.imap_uid)
    .bind(&m.rfc_message_id)
    .bind(&m.thread_id)
    .bind(&m.subject)
    .bind(&m.from_addr)
    .bind(&m.to_addrs)
    .bind(&m.cc_addrs)
    .bind(m.sent_at)
    .bind(m.internal_date)
    .bind(&m.flags)
    .bind(m.size_bytes)
    .bind(m.has_attachment)
    .bind(&m.snippet)
    .execute(pool)
    .await?;
    Ok(result.rows_affected() > 0)
}

pub async fn get(pool: &Pool, id: Uuid) -> AppResult<Option<MessageHeader>> {
    let row = sqlx::query_as::<_, MessageHeader>(
        r#"
        SELECT id, account_id, mailbox_id, imap_uid, rfc_message_id, thread_id,
               subject, from_addr, to_addrs, cc_addrs, sent_at, internal_date,
               flags, size_bytes, has_attachment, snippet, priority, body_fetched_at
        FROM messages
        WHERE id = $1
        "#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// Called after a successful body fetch. `snippet` uses COALESCE so a NULL snippet from this
/// call doesn't wipe an existing one — that way classification can backfill snippet later
/// without losing it on the next body refetch.
pub async fn mark_body_fetched(
    pool: &Pool,
    id: Uuid,
    has_attachment: bool,
    snippet: Option<String>,
) -> AppResult<()> {
    sqlx::query(
        r#"
        UPDATE messages
        SET has_attachment = $2,
            snippet        = COALESCE($3, snippet),
            body_fetched_at = NOW()
        WHERE id = $1
        "#,
    )
    .bind(id)
    .bind(has_attachment)
    .bind(snippet)
    .execute(pool)
    .await?;
    Ok(())
}

/// List by mailbox, most recent first. `sent_at DESC NULLS LAST` keeps undated junk at the
/// bottom; the secondary `imap_uid DESC` makes the order deterministic when timestamps tie.
pub async fn list_in_mailbox(
    pool: &Pool,
    mailbox_id: Uuid,
    limit: i64,
    offset: i64,
) -> AppResult<Vec<MessageHeader>> {
    let rows = sqlx::query_as::<_, MessageHeader>(
        r#"
        SELECT id, account_id, mailbox_id, imap_uid, rfc_message_id, thread_id,
               subject, from_addr, to_addrs, cc_addrs, sent_at, internal_date,
               flags, size_bytes, has_attachment, snippet, priority, body_fetched_at
        FROM messages
        WHERE mailbox_id = $1
        ORDER BY sent_at DESC NULLS LAST, imap_uid DESC
        LIMIT $2 OFFSET $3
        "#,
    )
    .bind(mailbox_id)
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

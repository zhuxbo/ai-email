//! Repository for the `mailboxes` table — per-account IMAP folder metadata.
//!
//! `uid_next` and `uid_validity` drive incremental sync. They're stored as INTEGER in SQLite so
//! we accept and return `i64` here even though IMAP wire UIDs fit in u32.

use serde::Serialize;
use sqlx::FromRow;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::db::Pool;
use crate::error::AppResult;
use crate::imap::client::MailboxInfo;

#[derive(Debug, Clone, Serialize, FromRow)]
#[serde(rename_all = "camelCase")]
pub struct Mailbox {
    pub id: Uuid,
    pub account_id: Uuid,
    pub name: String,
    pub delimiter: Option<String>,
    pub uid_validity: Option<i64>,
    pub uid_next: Option<i64>,
    #[serde(with = "time::serde::rfc3339::option")]
    pub last_synced_at: Option<OffsetDateTime>,
}

/// Insert-or-no-op based on `(account_id, name)`. Refreshes `delimiter` since servers can in
/// principle return a different value (rare, but cheap to handle). A fresh UUID is supplied for
/// the insert path; on conflict the existing row keeps its id.
pub async fn upsert(pool: &Pool, account_id: Uuid, info: &MailboxInfo) -> AppResult<()> {
    sqlx::query(
        r#"
        INSERT INTO mailboxes (id, account_id, name, delimiter)
        VALUES (?1, ?2, ?3, ?4)
        ON CONFLICT (account_id, name) DO UPDATE
        SET delimiter = excluded.delimiter
        "#,
    )
    .bind(Uuid::new_v4())
    .bind(account_id)
    .bind(&info.name)
    .bind(&info.delimiter)
    .execute(pool)
    .await?;
    Ok(())
}

/// Lookup by name, case-insensitively (`COLLATE NOCASE`). IMAP folder names like INBOX are
/// case-insensitive by spec, and servers may advertise mixed case ("Inbox"); without NOCASE the
/// hardcoded "INBOX" lookup in sync would miss such rows. Matches the frontend's
/// `name.toUpperCase() === 'INBOX'` so both sides resolve the same row (audit #27).
pub async fn get_by_name(pool: &Pool, account_id: Uuid, name: &str) -> AppResult<Option<Mailbox>> {
    let row = sqlx::query_as::<_, Mailbox>(
        r#"
        SELECT id, account_id, name, delimiter, uid_validity, uid_next, last_synced_at
        FROM mailboxes
        -- COLLATE NOCASE 让查找大小写不敏感；但 name 列及 UNIQUE(account_id,name) 约束仍是大小写敏感存储
        -- ——服务端按其通告的大小写写入，不会折叠，唯一约束不受 NOCASE 影响。
        WHERE account_id = ?1 AND name = ?2 COLLATE NOCASE
        "#,
    )
    .bind(account_id)
    .bind(name)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

pub async fn get(pool: &Pool, id: Uuid) -> AppResult<Option<Mailbox>> {
    let row = sqlx::query_as::<_, Mailbox>(
        r#"
        SELECT id, account_id, name, delimiter, uid_validity, uid_next, last_synced_at
        FROM mailboxes
        WHERE id = ?1
        "#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

pub async fn list(pool: &Pool, account_id: Uuid) -> AppResult<Vec<Mailbox>> {
    let rows = sqlx::query_as::<_, Mailbox>(
        r#"
        SELECT id, account_id, name, delimiter, uid_validity, uid_next, last_synced_at
        FROM mailboxes
        WHERE account_id = ?1
        ORDER BY name
        "#,
    )
    .bind(account_id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// Update sync bookkeeping after a successful IMAP SELECT + FETCH.
///
/// - `uid_validity`: `COALESCE(new, old)` — new value overwrites when present, server omitting
///   it (minimal SELECT response) preserves the stored value (audit #9).
/// - `uid_next`: monotonic MAX guard — never lets a stale interleaved sync regress the pointer
///   (audit #73). When the stored value is NULL (first-ever write or post-reset), the new value
///   is used unconditionally; otherwise `MAX(stored, new)` applies.
pub async fn update_after_sync(
    pool: &Pool,
    id: Uuid,
    uid_next: Option<i64>,
    uid_validity: Option<i64>,
) -> AppResult<()> {
    sqlx::query(
        r#"
        UPDATE mailboxes
        SET uid_next      = CASE
                                WHEN ?2 IS NULL    THEN uid_next
                                WHEN uid_next IS NULL THEN ?2
                                WHEN ?2 > uid_next THEN ?2
                                ELSE uid_next
                            END,
            uid_validity  = COALESCE(?3, uid_validity),
            last_synced_at = strftime('%Y-%m-%dT%H:%M:%f+00:00', 'now')
        WHERE id = ?1
        "#,
    )
    .bind(id)
    .bind(uid_next)
    .bind(uid_validity)
    .execute(pool)
    .await?;
    Ok(())
}

/// Drop all cached message rows for this mailbox and reset `uid_next` to NULL, then record
/// the new `uid_validity`. Called when the server's UIDVALIDITY differs from the stored one,
/// meaning the mailbox was rebuilt and every local UID is now invalid (RFC 3501 §2.3.1.1,
/// audit #2). Wrapped in a transaction so the reset is atomic.
pub async fn reset_mailbox_for_uidvalidity_change(
    pool: &Pool,
    mailbox_id: Uuid,
    new_uid_validity: i64,
) -> AppResult<()> {
    let mut tx = pool.begin().await?;
    sqlx::query("DELETE FROM messages WHERE mailbox_id = ?1")
        .bind(mailbox_id)
        .execute(&mut *tx)
        .await?;
    sqlx::query(
        r#"
        UPDATE mailboxes
        SET uid_next     = NULL,
            uid_validity = ?2,
            last_synced_at = strftime('%Y-%m-%dT%H:%M:%f+00:00', 'now')
        WHERE id = ?1
        "#,
    )
    .bind(mailbox_id)
    .bind(new_uid_validity)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(())
}

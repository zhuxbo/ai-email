//! Repository for the `mailboxes` table — per-account IMAP folder metadata.
//!
//! `uid_next` and `uid_validity` drive incremental sync. They're stored as `BIGINT` in PG so
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
/// principle return a different value (rare, but cheap to handle).
pub async fn upsert(pool: &Pool, account_id: Uuid, info: &MailboxInfo) -> AppResult<()> {
    sqlx::query(
        r#"
        INSERT INTO mailboxes (account_id, name, delimiter)
        VALUES ($1, $2, $3)
        ON CONFLICT (account_id, name) DO UPDATE
        SET delimiter = EXCLUDED.delimiter
        "#,
    )
    .bind(account_id)
    .bind(&info.name)
    .bind(&info.delimiter)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn get_by_name(pool: &Pool, account_id: Uuid, name: &str) -> AppResult<Option<Mailbox>> {
    let row = sqlx::query_as::<_, Mailbox>(
        r#"
        SELECT id, account_id, name, delimiter, uid_validity, uid_next, last_synced_at
        FROM mailboxes
        WHERE account_id = $1 AND name = $2
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
        WHERE id = $1
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
        WHERE account_id = $1
        ORDER BY name
        "#,
    )
    .bind(account_id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// Update sync bookkeeping. `COALESCE` keeps the old value when the IMAP server omits the
/// field — happens for very minimal SELECT responses.
pub async fn update_after_sync(
    pool: &Pool,
    id: Uuid,
    uid_next: Option<i64>,
    uid_validity: Option<i64>,
) -> AppResult<()> {
    sqlx::query(
        r#"
        UPDATE mailboxes
        SET uid_next = COALESCE($2, uid_next),
            uid_validity = COALESCE($3, uid_validity),
            last_synced_at = NOW()
        WHERE id = $1
        "#,
    )
    .bind(id)
    .bind(uid_next)
    .bind(uid_validity)
    .execute(pool)
    .await?;
    Ok(())
}

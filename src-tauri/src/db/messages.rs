//! Repository for the `messages` table — header-only rows. Bodies live in `message_bodies`
//! and are populated lazily on the first detail-view click (Sprint 1.4). AI-derived tags
//! live in `message_tags` and are joined into the returned `MessageHeader` (Sprint 3).
//!
//! IMAP UIDs are u32 on the wire but stored as INTEGER in SQLite (room for higher-UID
//! providers). The `i64` ↔ `u32` cast at the boundary is widening so always safe.
//!
//! Array columns (to_addrs / cc_addrs / flags) and the aggregated `tags` are JSON TEXT,
//! decoded with `#[sqlx(json)]` — SQLite has no native array type.

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
    #[sqlx(json)]
    pub to_addrs: Vec<String>,
    #[sqlx(json)]
    pub cc_addrs: Vec<String>,
    #[serde(with = "time::serde::rfc3339::option")]
    pub sent_at: Option<OffsetDateTime>,
    #[serde(with = "time::serde::rfc3339::option")]
    pub internal_date: Option<OffsetDateTime>,
    #[sqlx(json)]
    pub flags: Vec<String>,
    pub size_bytes: Option<i32>,
    pub has_attachment: bool,
    pub snippet: Option<String>,
    pub priority: Option<i32>,
    /// AI-assigned bucket: 'personal' | 'work' | 'notification' | 'promotion' | 'spam'.
    pub category: Option<String>,
    /// All user + AI tags. Populated via `LEFT JOIN message_tags` in every SELECT.
    #[sqlx(json)]
    pub tags: Vec<String>,
    #[serde(with = "time::serde::rfc3339::option")]
    pub body_fetched_at: Option<OffsetDateTime>,
}

/// Owned struct passed to `insert`. Separate from `MessageHeader` so callers don't need to
/// invent values for the DB-generated columns.
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

/// `SELECT ... LEFT JOIN message_tags ...` projection used by both `get` and
/// `list_in_mailbox`. Keeping the column order identical across both saves repeating the
/// projection logic and lets `FromRow` derive Just Work. `json_group_array` aggregates the
/// joined tags into a JSON array (SQLite's equivalent of PG's `array_agg`).
const SELECT_COLUMNS: &str = r#"
    m.id, m.account_id, m.mailbox_id, m.imap_uid, m.rfc_message_id, m.thread_id,
    m.subject, m.from_addr, m.to_addrs, m.cc_addrs, m.sent_at, m.internal_date,
    m.flags, m.size_bytes, m.has_attachment, m.snippet, m.priority, m.category,
    COALESCE(json_group_array(t.tag) FILTER (WHERE t.tag IS NOT NULL), '[]') AS tags,
    m.body_fetched_at
"#;

/// `INSERT … ON CONFLICT DO NOTHING RETURNING id`. Returns `Some(id)` iff a row was
/// inserted, `None` if the (account_id, mailbox_id, imap_uid) row already existed. Sync
/// uses the returned id to bulk-classify newly-landed messages.
pub async fn insert(pool: &Pool, m: &MessageInsert) -> AppResult<Option<Uuid>> {
    let id = sqlx::query_scalar::<_, Uuid>(
        r#"
        INSERT INTO messages (
            id, account_id, mailbox_id, imap_uid, rfc_message_id, thread_id,
            subject, from_addr, to_addrs, cc_addrs, sent_at,
            internal_date, flags, size_bytes, has_attachment, snippet
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)
        ON CONFLICT (account_id, mailbox_id, imap_uid) DO NOTHING
        RETURNING id
        "#,
    )
    .bind(Uuid::new_v4())
    .bind(m.account_id)
    .bind(m.mailbox_id)
    .bind(m.imap_uid)
    .bind(&m.rfc_message_id)
    .bind(&m.thread_id)
    .bind(&m.subject)
    .bind(&m.from_addr)
    .bind(serde_json::to_string(&m.to_addrs)?)
    .bind(serde_json::to_string(&m.cc_addrs)?)
    .bind(m.sent_at)
    .bind(m.internal_date)
    .bind(serde_json::to_string(&m.flags)?)
    .bind(m.size_bytes)
    .bind(m.has_attachment)
    .bind(&m.snippet)
    .fetch_optional(pool)
    .await?;
    Ok(id)
}

pub async fn get(pool: &Pool, id: Uuid) -> AppResult<Option<MessageHeader>> {
    let sql = format!(
        r#"
        SELECT {SELECT_COLUMNS}
        FROM messages m
        LEFT JOIN message_tags t ON t.message_id = m.id
        WHERE m.id = ?1
        GROUP BY m.id
        "#
    );
    let row = sqlx::query_as::<_, MessageHeader>(&sql)
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
        SET has_attachment = ?2,
            snippet        = COALESCE(?3, snippet),
            body_fetched_at = strftime('%Y-%m-%dT%H:%M:%f+00:00', 'now')
        WHERE id = ?1
        "#,
    )
    .bind(id)
    .bind(has_attachment)
    .bind(snippet)
    .execute(pool)
    .await?;
    Ok(())
}

/// Update the priority + category fields the classifier produced. `category` is stored as a
/// plain TEXT so we don't have to enforce the closed set at schema level (Sprint 3 spec uses
/// 5 values; future could refine).
pub async fn update_classification(
    pool: &Pool,
    id: Uuid,
    priority: i32,
    category: &str,
) -> AppResult<()> {
    sqlx::query(
        r#"
        UPDATE messages
        SET priority = ?2,
            category = ?3
        WHERE id = ?1
        "#,
    )
    .bind(id)
    .bind(priority)
    .bind(category)
    .execute(pool)
    .await?;
    Ok(())
}

/// 覆盖式更新本地 flags（JSON TEXT 列）。IMAP 写成功后调用，保持本地与服务端一致。
pub async fn update_flags(pool: &Pool, id: Uuid, flags: &[String]) -> AppResult<()> {
    sqlx::query("UPDATE messages SET flags = ?1 WHERE id = ?2")
        .bind(serde_json::to_string(flags)?)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

/// 删除本地消息行。bodies 等子表对 messages 均 ON DELETE CASCADE + 连接启用 foreign_keys，
/// 故单删 messages 即级联清理，无需显式删 body。
pub async fn remove(pool: &Pool, id: Uuid) -> AppResult<()> {
    sqlx::query("DELETE FROM messages WHERE id = ?1")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Header subset used by the classifier prompt builder. Pulls just what the prompt sees
/// (id + subject + from + snippet) so we don't waste tokens on internal_date / flags.
#[derive(Debug, Clone, FromRow)]
pub struct ClassifyInput {
    pub id: Uuid,
    pub subject: Option<String>,
    pub from_addr: Option<String>,
    pub snippet: Option<String>,
}

pub async fn fetch_for_classify(pool: &Pool, ids: &[Uuid]) -> AppResult<Vec<ClassifyInput>> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    // SQLite has no `= ANY($1)`; build an `IN (?, ?, …)` list and bind each id in turn.
    let placeholders = vec!["?"; ids.len()].join(", ");
    let sql = format!(
        r#"
        SELECT id, subject, from_addr, snippet
        FROM messages
        WHERE id IN ({placeholders})
        "#
    );
    let mut query = sqlx::query_as::<_, ClassifyInput>(&sql);
    for id in ids {
        query = query.bind(id);
    }
    let rows = query.fetch_all(pool).await?;
    Ok(rows)
}

/// List by mailbox, most recent first. SQLite sorts NULLs last under `DESC` by default, so
/// undated junk falls to the bottom; the secondary `imap_uid DESC` makes the order
/// deterministic when timestamps tie.
pub async fn list_in_mailbox(
    pool: &Pool,
    mailbox_id: Uuid,
    limit: i64,
    offset: i64,
) -> AppResult<Vec<MessageHeader>> {
    let sql = format!(
        r#"
        SELECT {SELECT_COLUMNS}
        FROM messages m
        LEFT JOIN message_tags t ON t.message_id = m.id
        WHERE m.mailbox_id = ?1
        GROUP BY m.id
        ORDER BY m.sent_at DESC, m.imap_uid DESC
        LIMIT ?2 OFFSET ?3
        "#
    );
    let rows = sqlx::query_as::<_, MessageHeader>(&sql)
        .bind(mailbox_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(pool)
        .await?;
    Ok(rows)
}

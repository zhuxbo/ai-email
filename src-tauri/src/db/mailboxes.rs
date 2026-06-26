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
    /// Detected special-use role: 'inbox' | 'sent' | 'drafts' | 'trash' | 'junk' | NULL.
    pub special_use: Option<String>,
}

/// Insert-or-update based on `(account_id, name)`. Refreshes `delimiter` and `special_use`
/// on conflict since these can change between syncs (rare, but cheap to handle).
/// A fresh UUID is supplied for the insert path; on conflict the existing row keeps its id.
pub async fn upsert(pool: &Pool, account_id: Uuid, info: &MailboxInfo) -> AppResult<()> {
    let special_use = info.special_use.as_ref().map(|su| su.as_str());
    sqlx::query(
        r#"
        INSERT INTO mailboxes (id, account_id, name, delimiter, special_use)
        VALUES (?1, ?2, ?3, ?4, ?5)
        ON CONFLICT (account_id, name) DO UPDATE
        SET delimiter   = excluded.delimiter,
            special_use = excluded.special_use
        "#,
    )
    .bind(Uuid::new_v4())
    .bind(account_id)
    .bind(&info.name)
    .bind(&info.delimiter)
    .bind(special_use)
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
        SELECT id, account_id, name, delimiter, uid_validity, uid_next, last_synced_at, special_use
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

/// 按 special_use 定位信箱（'inbox'|'sent'|'drafts'|'trash'|'junk'）。Sent 名各异，必须靠 special_use。
pub async fn get_by_special_use(
    pool: &Pool,
    account_id: Uuid,
    special_use: &str,
) -> AppResult<Option<Mailbox>> {
    let row = sqlx::query_as::<_, Mailbox>(
        r#"
        SELECT id, account_id, name, delimiter, uid_validity, uid_next, last_synced_at, special_use
        FROM mailboxes
        WHERE account_id = ?1 AND special_use = ?2
        "#,
    )
    .bind(account_id)
    .bind(special_use)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

pub async fn get(pool: &Pool, id: Uuid) -> AppResult<Option<Mailbox>> {
    let row = sqlx::query_as::<_, Mailbox>(
        r#"
        SELECT id, account_id, name, delimiter, uid_validity, uid_next, last_synced_at, special_use
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
        SELECT id, account_id, name, delimiter, uid_validity, uid_next, last_synced_at, special_use
        FROM mailboxes
        WHERE account_id = ?1
        ORDER BY
            CASE special_use
                WHEN 'inbox'  THEN 0
                WHEN 'sent'   THEN 1
                WHEN 'drafts' THEN 2
                WHEN 'trash'  THEN 3
                WHEN 'junk'   THEN 4
                ELSE 5
            END,
            name
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
            -- COALESCE 仅兜服务端返回 NULL 的情况（保留本地旧值）；
            -- validity 实际变更时已由 decide_sync_mode → ResetRefetch →
            -- reset_mailbox_for_uidvalidity_change 在更早的阶段前置处理，
            -- 此处不会静默丢弃 validity 变更。
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use crate::imap::client::SpecialUse;

    async fn test_pool() -> Pool {
        let opts = sqlx::sqlite::SqliteConnectOptions::new()
            .filename(":memory:")
            .foreign_keys(true);
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(opts)
            .await
            .unwrap();
        db::MIGRATOR.run(&pool).await.unwrap();
        pool
    }

    fn make_info(name: &str, special_use: Option<SpecialUse>) -> MailboxInfo {
        MailboxInfo {
            name: name.to_string(),
            delimiter: Some("/".to_string()),
            special_use,
        }
    }

    async fn insert_account(pool: &Pool) -> Uuid {
        let id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO accounts (id, email, provider, imap_host, smtp_host) \
             VALUES (?1, ?2, 'imap', 'imap.test', 'smtp.test')",
        )
        .bind(id)
        .bind(format!("user-{}@test.invalid", id))
        .execute(pool)
        .await
        .unwrap();
        id
    }

    #[tokio::test]
    async fn upsert_stores_special_use() {
        let pool = test_pool().await;
        let account_id = insert_account(&pool).await;

        upsert(
            &pool,
            account_id,
            &make_info("Sent Messages", Some(SpecialUse::Sent)),
        )
        .await
        .unwrap();

        let mb = get_by_name(&pool, account_id, "Sent Messages")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(mb.special_use.as_deref(), Some("sent"));
    }

    #[tokio::test]
    async fn upsert_updates_special_use_on_conflict() {
        let pool = test_pool().await;
        let account_id = insert_account(&pool).await;

        // First insert without special_use
        upsert(&pool, account_id, &make_info("Drafts", None))
            .await
            .unwrap();
        let mb = get_by_name(&pool, account_id, "Drafts")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(mb.special_use, None);

        // Update with special_use
        upsert(
            &pool,
            account_id,
            &make_info("Drafts", Some(SpecialUse::Drafts)),
        )
        .await
        .unwrap();
        let mb = get_by_name(&pool, account_id, "Drafts")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(mb.special_use.as_deref(), Some("drafts"));
    }

    #[tokio::test]
    async fn list_orders_inbox_first_then_special_use_then_alpha() {
        let pool = test_pool().await;
        let account_id = insert_account(&pool).await;

        // Insert in non-natural order
        upsert(
            &pool,
            account_id,
            &make_info("Trash", Some(SpecialUse::Trash)),
        )
        .await
        .unwrap();
        upsert(&pool, account_id, &make_info("Alpha Custom", None))
            .await
            .unwrap();
        upsert(
            &pool,
            account_id,
            &make_info("INBOX", Some(SpecialUse::Inbox)),
        )
        .await
        .unwrap();
        upsert(
            &pool,
            account_id,
            &make_info("Sent", Some(SpecialUse::Sent)),
        )
        .await
        .unwrap();
        upsert(
            &pool,
            account_id,
            &make_info("Drafts", Some(SpecialUse::Drafts)),
        )
        .await
        .unwrap();

        let names: Vec<_> = list(&pool, account_id)
            .await
            .unwrap()
            .into_iter()
            .map(|m| m.name)
            .collect();

        // inbox first, then special-use in order (sent=2, drafts=3, trash=4), then regular alpha
        assert_eq!(
            names,
            vec!["INBOX", "Sent", "Drafts", "Trash", "Alpha Custom"]
        );
    }
}

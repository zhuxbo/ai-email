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
    /// Space-separated RFC 5322 References header of this message, stored verbatim.
    /// Used by the sender to extend the thread chain on reply.
    pub references_header: Option<String>,
    /// 若为 true，AI 管道跳过签名/引用剥离，直接用原文。由用户或规则引擎设置。
    pub filter_disabled: bool,
    /// 若为 true，用户已手动锁定分类，AI 分类/自动回复候选跳过此消息。
    pub category_locked: bool,
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
    /// Space-separated RFC 5322 References header, stored verbatim for reply chain extension.
    pub references_header: Option<String>,
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
    m.body_fetched_at, m.references_header, m.filter_disabled, m.category_locked
"#;

/// `INSERT … ON CONFLICT DO NOTHING RETURNING id`. Returns `Some(id)` iff a row was
/// inserted, `None` if the (account_id, mailbox_id, imap_uid) row already existed. Sync
/// uses the returned id to bulk-classify newly-landed messages.
pub async fn insert(pool: &Pool, m: &MessageInsert) -> AppResult<Option<Uuid>> {
    insert_tx(pool, m).await
}

/// Same as [`insert`] but executes within an existing transaction, enabling the sync loop
/// to commit all inserts atomically in a single write (eliminates N+1 auto-commits).
///
/// Pass `&mut *tx` where `tx: sqlx::Transaction<'_, sqlx::Sqlite>`.
pub async fn insert_tx<'e, E>(executor: E, m: &MessageInsert) -> AppResult<Option<Uuid>>
where
    E: sqlx::Executor<'e, Database = sqlx::Sqlite>,
{
    let id = sqlx::query_scalar::<_, Uuid>(
        r#"
        INSERT INTO messages (
            id, account_id, mailbox_id, imap_uid, rfc_message_id, thread_id,
            subject, from_addr, to_addrs, cc_addrs, sent_at,
            internal_date, flags, size_bytes, has_attachment, snippet, references_header
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)
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
    .bind(&m.references_header)
    .fetch_optional(executor)
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

/// 切 per-message 过滤开关。disabled=true 时 AI 管道跳过签名/引用剥离，直接用原文。
pub async fn set_filter_disabled(pool: &Pool, id: Uuid, disabled: bool) -> AppResult<()> {
    sqlx::query("UPDATE messages SET filter_disabled = ?2 WHERE id = ?1")
        .bind(id)
        .bind(disabled)
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
) -> AppResult<u64> {
    let result = sqlx::query(
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
    Ok(result.rows_affected())
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

/// 原子地对单个 flag 做 add/remove，避免并发读-改-写冲突。
///
/// - `add = true`：将 `flag` 插入数组（若已存在则 no-op，UNION 自动去重）。
/// - `add = false`：将 `flag` 从数组中移除（若不存在则 no-op）。
///
/// 全程在单条 SQL 内完成，不需要先读出 flags 再写回。
///
/// 注意：`json_group_array` + `UNION` 会按值字典序重排 flags，不保留原 IMAP 顺序。
/// IMAP flags 是无序集合（RFC 3501），语义上无害；但勿在此之外依赖 flags 数组的顺序。
pub async fn update_flag_atomic(pool: &Pool, id: Uuid, flag: &str, add: bool) -> AppResult<()> {
    if add {
        sqlx::query(
            r#"
            UPDATE messages
            SET flags = (
                SELECT json_group_array(value)
                FROM (
                    SELECT value FROM json_each(flags)
                    UNION SELECT ?2
                )
            )
            WHERE id = ?1
            "#,
        )
        .bind(id)
        .bind(flag)
        .execute(pool)
        .await?;
    } else {
        sqlx::query(
            r#"
            UPDATE messages
            SET flags = (
                SELECT json_group_array(value)
                FROM json_each(flags)
                WHERE value != ?2
            )
            WHERE id = ?1
            "#,
        )
        .bind(id)
        .bind(flag)
        .execute(pool)
        .await?;
    }
    Ok(())
}

/// 按 `(mailbox_id, imap_uid)` 刷新 flags，用于增量同步时对已存在 UID 的 flags 更新（audit #64）。
/// 若该 UID 尚未落库（正常不会发生）则是 no-op。
pub async fn update_flags_by_uid(
    pool: &Pool,
    mailbox_id: Uuid,
    imap_uid: i64,
    flags: &[String],
) -> AppResult<()> {
    sqlx::query("UPDATE messages SET flags = ?1 WHERE mailbox_id = ?2 AND imap_uid = ?3")
        .bind(serde_json::to_string(flags)?)
        .bind(mailbox_id)
        .bind(imap_uid)
        .execute(pool)
        .await?;
    Ok(())
}

/// 给范围内所有消息的 flags 批量加 `\Seen`（去重，已读的不重复加，幂等）。
///
/// 复用 `update_flag_atomic` add 分支的 json 写法（`json_each(flags) UNION SELECT '\Seen'`），
/// 在单条 SQL 内对 `scope_pred` 命中的每行各自去重合并。`scope_pred` 用 `messages` 列名
/// （无表别名）。
///
/// **纯本地、不打 IMAP**：对齐前端 markAllSeen「乐观本地 + 失败 reload」语义，避免 N 封逐个连
/// IMAP（账户收件箱可能上千封）。已知局限：服务端 \Seen 未写，下次 sync 可能把未读拉回。
async fn mark_seen_where(pool: &Pool, scope_pred: &str, scope_id: Uuid) -> AppResult<()> {
    let sql = format!(
        r#"
        UPDATE messages
        SET flags = (
            SELECT json_group_array(value)
            FROM (
                SELECT value FROM json_each(flags)
                UNION SELECT '\Seen'
            )
        )
        WHERE {scope_pred}
        "#
    );
    sqlx::query(&sql).bind(scope_id).execute(pool).await?;
    Ok(())
}

/// 单信箱「全部已读」：把该信箱内所有消息标 `\Seen`（纯本地，见 [`mark_seen_where`]）。
pub async fn mailbox_mark_seen(pool: &Pool, mailbox_id: Uuid) -> AppResult<()> {
    mark_seen_where(pool, "mailbox_id = ?1", mailbox_id).await
}

/// 账户级收件箱「全部已读」：把账户的真正 INBOX（不含服务端自定义归类目录、Sent 等）内的消息标
/// `\Seen`（纯本地，见 [`mark_seen_where`]）。
/// scope 谓词与折叠列表共享 [`crate::db::INBOX_KIND_PRED`]，保证「聚合收件箱范围」口径一致。
pub async fn account_inbox_mark_seen(pool: &Pool, account_id: Uuid) -> AppResult<()> {
    let pred = format!(
        "mailbox_id IN (SELECT id FROM mailboxes WHERE account_id = ?1 AND {})",
        crate::db::INBOX_KIND_PRED
    );
    mark_seen_where(pool, &pred, account_id).await
}

/// 同发件人组成员：收件箱范围内、`from_addr = ?` 且 `category IN ('notification','promotion')`
/// 的消息头，按 `sent_at DESC, imap_uid DESC` 取最多 `limit` 条。
///
/// 供 `sender_group_thread` 命令选成员（与折叠列表的 sender 组同口径：仅通知/推广类按发件人
/// 聚合）。纯 db 查询，不打 IMAP，故可单测。
pub async fn sender_group_members(
    pool: &Pool,
    account_id: Uuid,
    from_addr: &str,
    limit: i64,
) -> AppResult<Vec<MessageHeader>> {
    let sql = format!(
        r#"
        SELECT {SELECT_COLUMNS}
        FROM messages m
        LEFT JOIN message_tags t ON t.message_id = m.id
        WHERE m.account_id = ?1
          AND m.from_addr = ?2
          AND m.category IN ('notification','promotion')
          AND m.mailbox_id IN (
              SELECT id FROM mailboxes WHERE account_id = ?1 AND {}
          )
        GROUP BY m.id
        ORDER BY m.sent_at DESC, m.imap_uid DESC
        LIMIT ?3
        "#,
        crate::db::INBOX_KIND_PRED
    );
    let rows = sqlx::query_as::<_, MessageHeader>(&sql)
        .bind(account_id)
        .bind(from_addr)
        .bind(limit)
        .fetch_all(pool)
        .await?;
    Ok(rows)
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

/// 手动设置消息分类并锁定（category_locked = 1），AI 后续不再覆写。
/// 返回受影响行数：1 = 成功，0 = 消息已删（调用方可按需 warn）。
pub async fn set_category_locked(pool: &Pool, id: Uuid, category: &str) -> AppResult<u64> {
    let res = sqlx::query("UPDATE messages SET category = ?2, category_locked = 1 WHERE id = ?1")
        .bind(id)
        .bind(category)
        .execute(pool)
        .await?;
    Ok(res.rows_affected())
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

/// 分块 IN 查询的批大小。SQLite SQLITE_MAX_VARIABLE_NUMBER 默认 32766，500 远低于上限，
/// 且与调用方的 AI 批大小（BATCH_SIZE=20）解耦，使 DB 层自身安全。
pub const IN_CHUNK_SIZE: usize = 500;

/// 对 ids 按 `IN_CHUNK_SIZE` 分块执行多次查询并合并结果，避免超过 SQLite 绑定变量上限。
pub async fn fetch_for_classify(pool: &Pool, ids: &[Uuid]) -> AppResult<Vec<ClassifyInput>> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let mut out = Vec::with_capacity(ids.len());
    for chunk in ids.chunks(IN_CHUNK_SIZE) {
        let placeholders = vec!["?"; chunk.len()].join(", ");
        let sql = format!(
            "SELECT id, subject, from_addr, snippet FROM messages WHERE id IN ({placeholders}) AND category_locked = 0"
        );
        let mut query = sqlx::query_as::<_, ClassifyInput>(&sql);
        for id in chunk {
            query = query.bind(id);
        }
        out.extend(query.fetch_all(pool).await?);
    }
    Ok(out)
}

/// 测试专用：读取单条消息的 `category`（外层 Option = 行是否存在，内层 = 列是否为 NULL）。
#[cfg(test)]
pub(crate) async fn category_of(pool: &Pool, id: Uuid) -> AppResult<Option<String>> {
    let row: Option<(Option<String>,)> =
        sqlx::query_as("SELECT category FROM messages WHERE id = ?1")
            .bind(id)
            .fetch_optional(pool)
            .await?;
    Ok(row.and_then(|r| r.0))
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

/// 返回存量中需要重跑分类的消息 ID 列表。
///
/// 条件：已有分类（category IS NOT NULL）、未被用户锁定（category_locked=0）、
/// 位于收件箱或服务端自定义归类目录（`special_use IS NULL OR ='inbox'`）。按 sent_at DESC，最多 `cap` 条。
///
/// 注意：此谓词**刻意比** [`crate::db::INBOX_KIND_PRED`]（仅真正 INBOX）**更宽**——后台重分类也覆盖
/// 自定义归类目录（单选时可见、需保持分类新鲜），故仍含 `special_use IS NULL`。**勿与 INBOX_KIND_PRED 对齐统一**。
pub async fn reclassify_candidates(pool: &Pool, cap: i64) -> AppResult<Vec<Uuid>> {
    let rows: Vec<(Uuid,)> = sqlx::query_as(
        "SELECT m.id FROM messages m JOIN mailboxes b ON m.mailbox_id=b.id \
         WHERE (b.special_use IS NULL OR b.special_use='inbox') \
           AND m.category IS NOT NULL AND m.category_locked=0 \
         ORDER BY m.sent_at DESC LIMIT ?1",
    )
    .bind(cap)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(|r| r.0).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    /// 构造测试专用内存 SQLite pool（单连接，迁移已跑完，外键启用）。
    ///
    /// `:memory:` 的每个连接是独立实例；必须用 `max_connections(1)` 保证所有操作
    /// 走同一连接，否则迁移建的表对其他连接不可见。
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

    /// 插入一条最简消息行（含合法的父行 accounts + mailboxes），返回其 id。
    async fn insert_minimal(pool: &Pool) -> Uuid {
        let account_id = Uuid::new_v4();
        let mailbox_id = Uuid::new_v4();
        let msg_id = Uuid::new_v4();

        sqlx::query(
            "INSERT INTO accounts (id, email, provider, imap_host, smtp_host) \
             VALUES (?1, ?2, 'imap', 'imap.test', 'smtp.test')",
        )
        .bind(account_id)
        .bind(format!("test-{}@test.invalid", account_id))
        .execute(pool)
        .await
        .unwrap();

        sqlx::query("INSERT INTO mailboxes (id, account_id, name) VALUES (?1, ?2, 'INBOX')")
            .bind(mailbox_id)
            .bind(account_id)
            .execute(pool)
            .await
            .unwrap();

        sqlx::query(
            "INSERT INTO messages (id, account_id, mailbox_id, imap_uid, flags) \
             VALUES (?1, ?2, ?3, 1, '[]')",
        )
        .bind(msg_id)
        .bind(account_id)
        .bind(mailbox_id)
        .execute(pool)
        .await
        .unwrap();

        msg_id
    }

    // ── #30: update_flag_atomic 原子 add/remove ──────────────────────────────

    #[tokio::test]
    async fn flag_add_is_idempotent() {
        let pool = test_pool().await;
        let id = insert_minimal(&pool).await;

        update_flag_atomic(&pool, id, "\\Seen", true).await.unwrap();
        update_flag_atomic(&pool, id, "\\Seen", true).await.unwrap(); // 第二次 no-op

        let row: (String,) = sqlx::query_as("SELECT flags FROM messages WHERE id = ?1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
        let flags: Vec<String> = serde_json::from_str(&row.0).unwrap();
        assert_eq!(flags, vec!["\\Seen"]);
    }

    #[tokio::test]
    async fn flag_remove_is_idempotent() {
        let pool = test_pool().await;
        let id = insert_minimal(&pool).await;

        update_flag_atomic(&pool, id, "\\Seen", true).await.unwrap();
        update_flag_atomic(&pool, id, "\\Seen", false)
            .await
            .unwrap();
        update_flag_atomic(&pool, id, "\\Seen", false)
            .await
            .unwrap(); // 第二次 no-op

        let row: (String,) = sqlx::query_as("SELECT flags FROM messages WHERE id = ?1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
        let flags: Vec<String> = serde_json::from_str(&row.0).unwrap();
        assert!(flags.is_empty());
    }

    /// 并发加 \\Seen 与 \\Flagged，验证两个 flag 都保留（不丢失）。
    #[tokio::test]
    async fn concurrent_flag_adds_do_not_lose_updates() {
        let pool = test_pool().await;
        let id = insert_minimal(&pool).await;

        // 两个原子写并发执行，互不干扰
        let (r1, r2) = tokio::join!(
            update_flag_atomic(&pool, id, "\\Seen", true),
            update_flag_atomic(&pool, id, "\\Flagged", true),
        );
        r1.unwrap();
        r2.unwrap();

        let row: (String,) = sqlx::query_as("SELECT flags FROM messages WHERE id = ?1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
        let mut flags: Vec<String> = serde_json::from_str(&row.0).unwrap();
        flags.sort();
        // 两个 flag 均须存在
        assert!(
            flags.contains(&"\\Seen".to_string()),
            "\\Seen missing: {flags:?}"
        );
        assert!(
            flags.contains(&"\\Flagged".to_string()),
            "\\Flagged missing: {flags:?}"
        );
    }

    #[tokio::test]
    async fn fetch_for_classify_excludes_locked() {
        use crate::db::test_seed::*;
        let pool = crate::db::test_pool().await;
        let acc = seed_account(&pool).await;
        let mb = seed_mailbox(&pool, acc, "INBOX", None).await;
        let unlocked = seed_msg(
            &pool,
            acc,
            mb,
            1,
            "a@x.com",
            None,
            Some("spam"),
            false,
            "[]",
        )
        .await;
        let locked = seed_msg(&pool, acc, mb, 2, "b@x.com", None, Some("spam"), true, "[]").await;
        let got = fetch_for_classify(&pool, &[unlocked, locked])
            .await
            .unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].id, unlocked);
    }

    #[tokio::test]
    async fn set_category_locked_updates_and_locks() {
        use crate::db::test_seed::*;
        let pool = crate::db::test_pool().await;
        let acc = seed_account(&pool).await;
        let mb = seed_mailbox(&pool, acc, "INBOX", None).await;
        let id = seed_msg(
            &pool,
            acc,
            mb,
            1,
            "a@x.com",
            None,
            Some("spam"),
            false,
            "[]",
        )
        .await;
        assert_eq!(set_category_locked(&pool, id, "personal").await.unwrap(), 1);
        assert_eq!(
            category_of(&pool, id).await.unwrap().as_deref(),
            Some("personal")
        );
        // 锁定后不进分类
        assert!(fetch_for_classify(&pool, &[id]).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn set_category_locked_deleted_is_zero_rows() {
        use crate::db::test_seed::*;
        let pool = crate::db::test_pool().await;
        let acc = seed_account(&pool).await;
        let mb = seed_mailbox(&pool, acc, "INBOX", None).await;
        let id = seed_msg(
            &pool,
            acc,
            mb,
            1,
            "a@x.com",
            None,
            Some("spam"),
            false,
            "[]",
        )
        .await;
        sqlx::query("DELETE FROM messages WHERE id=?1")
            .bind(id)
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(set_category_locked(&pool, id, "work").await.unwrap(), 0);
    }

    /// 读取一封消息的 flags 数组（测试辅助）。
    async fn flags_of(pool: &Pool, id: Uuid) -> Vec<String> {
        let row: (String,) = sqlx::query_as("SELECT flags FROM messages WHERE id = ?1")
            .bind(id)
            .fetch_one(pool)
            .await
            .unwrap();
        serde_json::from_str(&row.0).unwrap()
    }

    // 批量已读（账户级收件箱）：两封未读 → 两封都含 \Seen；再调一次幂等不重复加。
    #[tokio::test]
    async fn account_inbox_mark_seen_marks_and_is_idempotent() {
        use crate::db::test_seed::*;
        let pool = crate::db::test_pool().await;
        let acc = seed_account(&pool).await;
        let inbox = seed_mailbox(&pool, acc, "INBOX", None).await;
        let a = seed_msg(&pool, acc, inbox, 1, "p@x.com", None, None, false, "[]").await;
        let b = seed_msg(&pool, acc, inbox, 2, "q@x.com", None, None, false, "[]").await;

        account_inbox_mark_seen(&pool, acc).await.unwrap();
        assert_eq!(flags_of(&pool, a).await, vec!["\\Seen"]);
        assert_eq!(flags_of(&pool, b).await, vec!["\\Seen"]);

        // 幂等：再调一次不重复加。
        account_inbox_mark_seen(&pool, acc).await.unwrap();
        assert_eq!(flags_of(&pool, a).await, vec!["\\Seen"]);
        assert_eq!(flags_of(&pool, b).await, vec!["\\Seen"]);
    }

    // 账户级 mark_seen 不碰 Sent（special_use='sent'）。
    #[tokio::test]
    async fn account_inbox_mark_seen_excludes_sent() {
        use crate::db::test_seed::*;
        let pool = crate::db::test_pool().await;
        let acc = seed_account(&pool).await;
        let inbox = seed_mailbox(&pool, acc, "INBOX", None).await;
        let sent = seed_mailbox(&pool, acc, "Sent", Some("sent")).await;
        let inbox_msg = seed_msg(&pool, acc, inbox, 1, "p@x.com", None, None, false, "[]").await;
        let sent_msg = seed_msg(&pool, acc, sent, 2, "me@x.com", None, None, false, "[]").await;

        account_inbox_mark_seen(&pool, acc).await.unwrap();
        assert_eq!(flags_of(&pool, inbox_msg).await, vec!["\\Seen"]);
        assert!(
            flags_of(&pool, sent_msg).await.is_empty(),
            "Sent 不应被标已读"
        );
    }

    // 单信箱 mark_seen：只标该信箱内的消息。
    #[tokio::test]
    async fn mailbox_mark_seen_scopes_to_one_mailbox() {
        use crate::db::test_seed::*;
        let pool = crate::db::test_pool().await;
        let acc = seed_account(&pool).await;
        let inbox = seed_mailbox(&pool, acc, "INBOX", None).await;
        let other = seed_mailbox(&pool, acc, "Other", None).await;
        let in_msg = seed_msg(&pool, acc, inbox, 1, "p@x.com", None, None, false, "[]").await;
        let other_msg = seed_msg(&pool, acc, other, 2, "q@x.com", None, None, false, "[]").await;

        mailbox_mark_seen(&pool, inbox).await.unwrap();
        assert_eq!(flags_of(&pool, in_msg).await, vec!["\\Seen"]);
        assert!(
            flags_of(&pool, other_msg).await.is_empty(),
            "别的信箱不应被标"
        );
    }

    // sender_group_members：同发件人 3 封 notification → 返回 3 条、按 sent_at DESC、limit 生效、
    // 只含 notification/promotion 类别。
    #[tokio::test]
    async fn sender_group_members_filters_and_orders() {
        use crate::db::test_seed::*;
        let pool = crate::db::test_pool().await;
        let acc = seed_account(&pool).await;
        let inbox = seed_mailbox(&pool, acc, "INBOX", None).await;
        let m1 = seed_msg(&pool, acc, inbox, 1, "ad@x.com", None, NOTIF, false, "[]").await;
        let m2 = seed_msg(&pool, acc, inbox, 2, "ad@x.com", None, NOTIF, false, "[]").await;
        let m3 = seed_msg(&pool, acc, inbox, 3, "ad@x.com", None, NOTIF, false, "[]").await;
        // 干扰项：同发件人但 personal → 不应入组。
        let _personal = seed_msg(
            &pool, acc, inbox, 4, "ad@x.com", None, PERSONAL, false, "[]",
        )
        .await;
        // 干扰项：别的发件人 notification → 不应入组。
        let _other = seed_msg(&pool, acc, inbox, 5, "zz@x.com", None, NOTIF, false, "[]").await;

        let got = sender_group_members(&pool, acc, "ad@x.com", 50)
            .await
            .unwrap();
        assert_eq!(got.len(), 3, "{got:#?}");
        // sent_at DESC：uid 越大越新 → m3, m2, m1
        assert_eq!(
            got.iter().map(|h| h.id).collect::<Vec<_>>(),
            vec![m3, m2, m1]
        );

        // limit 生效：取最新 2 条。
        let capped = sender_group_members(&pool, acc, "ad@x.com", 2)
            .await
            .unwrap();
        assert_eq!(
            capped.iter().map(|h| h.id).collect::<Vec<_>>(),
            vec![m3, m2]
        );
    }

    // sender_group_members 只在收件箱范围内选（不含 Sent）。
    #[tokio::test]
    async fn sender_group_members_excludes_sent() {
        use crate::db::test_seed::*;
        let pool = crate::db::test_pool().await;
        let acc = seed_account(&pool).await;
        let inbox = seed_mailbox(&pool, acc, "INBOX", None).await;
        let sent = seed_mailbox(&pool, acc, "Sent", Some("sent")).await;
        let in_msg = seed_msg(&pool, acc, inbox, 1, "ad@x.com", None, NOTIF, false, "[]").await;
        let _sent_msg = seed_msg(&pool, acc, sent, 2, "ad@x.com", None, NOTIF, false, "[]").await;

        let got = sender_group_members(&pool, acc, "ad@x.com", 50)
            .await
            .unwrap();
        assert_eq!(got.iter().map(|h| h.id).collect::<Vec<_>>(), vec![in_msg]);
    }

    const NOTIF: Option<&str> = Some("notification");
    const PERSONAL: Option<&str> = Some("personal");

    #[tokio::test]
    async fn reclassify_candidates_excludes_locked_and_unclassified() {
        use crate::db::test_seed::*;
        let pool = crate::db::test_pool().await;
        let acc = seed_account(&pool).await;
        let mb = seed_mailbox(&pool, acc, "INBOX", None).await;
        let classified = seed_msg(
            &pool,
            acc,
            mb,
            1,
            "a@x.com",
            None,
            Some("spam"),
            false,
            "[]",
        )
        .await;
        let _locked = seed_msg(&pool, acc, mb, 2, "b@x.com", None, Some("spam"), true, "[]").await;
        let _raw = seed_msg(&pool, acc, mb, 3, "c@x.com", None, None, false, "[]").await;
        let ids = reclassify_candidates(&pool, 1000).await.unwrap();
        assert_eq!(ids, vec![classified]);
    }
}

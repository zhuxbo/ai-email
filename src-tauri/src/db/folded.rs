//! 列表折叠查询：把 scope 内的消息折叠成「折叠行」，会话/孤立同发件人各成一组。
//!
//! 折叠在后端 SQL 上算（对全量数据比前端窗口更准），按账户、不跨账户。
//!
//! 折叠键（`fold_key`）：
//! - `category IN ('notification','promotion')` 且 `from_addr` 非空，且该消息「孤立」
//!   （`thread_id IS NULL` 或 **账户级** `thread_size <= 1`）→ `'sender:'||from_addr`。
//! - 否则 → `'thread:'||COALESCE(thread_id,'msg:'||hex(id))`。
//!
//! 关键不变量：`thread_size` 必须是**账户级**（跨全信箱），与详情 `conversations::list_conversation`
//! 口径一致。这样「孤立通知」（账户内无同 thread 伙伴）才折成 sender 组；与别的邮件共享 thread
//! 的通知留在 thread 组。

use serde::Serialize;
use sqlx::FromRow;
use uuid::Uuid;

use crate::db::messages::MessageHeader;
use crate::db::Pool;
use crate::error::AppResult;

/// 折叠行的种类。`count == 1` → `Single`；否则按 bucket 落 `Thread` / `Sender`。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum FoldKind {
    Single,
    Thread,
    Sender,
}

/// 一条折叠行：代表消息 + 折叠元信息。`count` 是组大小（thread 组=账户级成员数，
/// sender 组=scope 内分区大小），`has_unread` 表示组内任一未读。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FoldedItem {
    pub representative: MessageHeader,
    pub fold_kind: FoldKind,
    pub fold_key: String,
    pub count: i64,
    pub has_unread: bool,
}

/// 未读谓词：flags JSON 数组里不含 `\Seen`。**用裸 `flags`（无表别名）**——`ranked` 层 FROM 是
/// keyed CTE（已 SELECT 出 flags 列），加表别名会找不到列。
const UNREAD_PRED: &str = r#"NOT EXISTS (SELECT 1 FROM json_each(flags) WHERE value = '\Seen')"#;

/// `scoped` CTE 的列投影，与 `messages::SELECT_COLUMNS` / `conversations` 逐字符一致
/// （MessageHeader 加列时三处同步）。用 `m.` 别名，故 `scoped` 必须 JOIN message_tags + GROUP BY m.id。
const SCOPED_COLUMNS: &str = r#"
    m.id, m.account_id, m.mailbox_id, m.imap_uid, m.rfc_message_id, m.thread_id,
    m.subject, m.from_addr, m.to_addrs, m.cc_addrs, m.sent_at, m.internal_date,
    m.flags, m.size_bytes, m.has_attachment, m.snippet, m.priority, m.category,
    COALESCE(json_group_array(t.tag) FILTER (WHERE t.tag IS NOT NULL), '[]') AS tags,
    m.body_fetched_at, m.references_header, m.filter_disabled, m.category_locked
"#;

/// 中间行：`#[sqlx(flatten)]` 把 `MessageHeader` 的列吃进来，余下三列单取。
/// `SELECT *` 带出的 `thread_size` / `rn` 等多余列被 sqlx 忽略。
#[derive(FromRow)]
struct FoldedRow {
    #[sqlx(flatten)]
    representative: MessageHeader,
    fold_key: String,
    grp_count: i64,
    grp_unread: i64,
}

impl From<FoldedRow> for FoldedItem {
    fn from(r: FoldedRow) -> Self {
        let fold_kind = if r.grp_count >= 2 {
            if r.fold_key.starts_with("thread:") {
                FoldKind::Thread
            } else {
                FoldKind::Sender
            }
        } else {
            FoldKind::Single
        };
        FoldedItem {
            representative: r.representative,
            fold_kind,
            fold_key: r.fold_key,
            count: r.grp_count,
            has_unread: r.grp_unread != 0,
        }
    }
}

/// 构造四层 CTE。`scope_pred` 限定 `scoped` 的消息范围（用 `m.` 别名），随后绑定 `?1 = acct`
/// （thread_counts 账户级聚合）。两命令 CTE 同构，仅 scope 谓词与绑定不同。
fn folded_sql(scope_pred: &str) -> String {
    format!(
        r#"
WITH scoped AS (
  SELECT {SCOPED_COLUMNS}
  FROM messages m
  LEFT JOIN message_tags t ON t.message_id = m.id
  WHERE {scope_pred}
  GROUP BY m.id ),
thread_counts AS (
  SELECT thread_id, COUNT(*) AS thread_size FROM messages
  WHERE account_id = ?1 AND thread_id IS NOT NULL GROUP BY thread_id ),
keyed AS (
  SELECT s.*, tc.thread_size,
    CASE WHEN s.category IN ('notification','promotion') AND s.from_addr IS NOT NULL
              AND (s.thread_id IS NULL OR COALESCE(tc.thread_size,1) <= 1)
         THEN 'sender:'||s.from_addr
         ELSE 'thread:'||COALESCE(s.thread_id,'msg:'||hex(s.id)) END AS fold_key
  FROM scoped s LEFT JOIN thread_counts tc ON s.thread_id = tc.thread_id ),
ranked AS (
  SELECT *,
    ROW_NUMBER() OVER (PARTITION BY fold_key ORDER BY sent_at DESC, imap_uid DESC) AS rn,
    CASE WHEN fold_key LIKE 'thread:%' THEN COALESCE(thread_size,1)
         ELSE COUNT(*) OVER (PARTITION BY fold_key) END AS grp_count,
    MAX(CASE WHEN {UNREAD_PRED} THEN 1 ELSE 0 END) OVER (PARTITION BY fold_key) AS grp_unread
  FROM keyed )
SELECT * FROM ranked WHERE rn=1 ORDER BY sent_at DESC, imap_uid DESC LIMIT ?2
"#
    )
}

/// 账户级收件箱折叠：汇聚账户下所有 inbox 类信箱（`special_use IS NULL` 或 `'inbox'`），排除 Sent。
/// `thread_counts` 与 scope 同账户，故跨信箱 thread 的成员数仍正确。
pub async fn account_inbox_folded(
    pool: &Pool,
    account_id: Uuid,
    limit: i64,
) -> AppResult<Vec<FoldedItem>> {
    let sql = folded_sql(
        "m.mailbox_id IN (SELECT id FROM mailboxes \
         WHERE account_id = ?1 AND (special_use IS NULL OR special_use = 'inbox'))",
    );
    let rows = sqlx::query_as::<_, FoldedRow>(&sql)
        .bind(account_id)
        .bind(limit)
        .fetch_all(pool)
        .await?;
    Ok(rows.into_iter().map(FoldedItem::from).collect())
}

/// 单信箱折叠：scope = 该信箱内的消息。`thread_counts` 用该信箱所属账户聚合（账户级），
/// 故与别的信箱共享 thread 的消息其 count 仍是账户级成员数（class 1 不变量）。
pub async fn mailbox_folded(
    pool: &Pool,
    mailbox_id: Uuid,
    limit: i64,
) -> AppResult<Vec<FoldedItem>> {
    let acct: Uuid = sqlx::query_scalar("SELECT account_id FROM mailboxes WHERE id = ?1")
        .bind(mailbox_id)
        .fetch_one(pool)
        .await?;
    let sql = folded_sql("m.mailbox_id = ?3");
    let rows = sqlx::query_as::<_, FoldedRow>(&sql)
        .bind(acct)
        .bind(limit)
        .bind(mailbox_id)
        .fetch_all(pool)
        .await?;
    Ok(rows.into_iter().map(FoldedItem::from).collect())
}

#[cfg(test)]
mod tests {
    use crate::db::folded::*;
    use crate::db::test_seed::*;

    const NOTIF: Option<&str> = Some("notification");
    const PROMO: Option<&str> = Some("promotion");
    const PERSONAL: Option<&str> = Some("personal");

    /// 找出某 fold_key 的折叠行。
    fn find<'a>(items: &'a [FoldedItem], key: &str) -> &'a FoldedItem {
        items
            .iter()
            .find(|i| i.fold_key == key)
            .unwrap_or_else(|| panic!("fold_key {key} not found in {items:#?}"))
    }

    // 类 1：跨信箱 thread 不被抽离。INBOX 1 封 notification + Sent 1 封同 thread →
    // 账户级 thread_size=2，notification 留 thread 组、count=2、foldKind=thread。
    #[tokio::test]
    async fn cross_mailbox_thread_keeps_thread_group() {
        let pool = crate::db::test_pool().await;
        let acc = seed_account(&pool).await;
        let inbox = seed_mailbox(&pool, acc, "INBOX", None).await;
        let sent = seed_mailbox(&pool, acc, "Sent", Some("sent")).await;
        seed_msg(
            &pool,
            acc,
            inbox,
            1,
            "n@x.com",
            Some("T1"),
            NOTIF,
            false,
            "[]",
        )
        .await;
        seed_msg(
            &pool,
            acc,
            sent,
            2,
            "me@x.com",
            Some("T1"),
            PERSONAL,
            false,
            "[]",
        )
        .await;

        let items = mailbox_folded(&pool, inbox, 50).await.unwrap();
        assert_eq!(items.len(), 1, "{items:#?}");
        let it = find(&items, "thread:T1");
        assert!(
            matches!(it.fold_kind, FoldKind::Thread),
            "{:?}",
            it.fold_kind
        );
        assert_eq!(it.count, 2, "thread badge = 账户级成员数（含 Sent）");
    }

    // 类 2：两封无 thread_id、同发件人、promotion → 折成一个 sender 组、count=2、foldKind=sender。
    #[tokio::test]
    async fn null_thread_same_sender_folds_sender() {
        let pool = crate::db::test_pool().await;
        let acc = seed_account(&pool).await;
        let inbox = seed_mailbox(&pool, acc, "INBOX", None).await;
        seed_msg(&pool, acc, inbox, 3, "ad@x.com", None, PROMO, false, "[]").await;
        seed_msg(&pool, acc, inbox, 4, "ad@x.com", None, PROMO, false, "[]").await;

        let items = mailbox_folded(&pool, inbox, 50).await.unwrap();
        assert_eq!(items.len(), 1, "{items:#?}");
        let it = find(&items, "sender:ad@x.com");
        assert!(
            matches!(it.fold_kind, FoldKind::Sender),
            "{:?}",
            it.fold_kind
        );
        assert_eq!(it.count, 2);
        // 代表 = 最新（uid 越大越新）
        assert_eq!(it.representative.imap_uid, 4);
    }

    // 类 3：NULL-thread 不同发件人 → 两个独立 sender 组各 count=1、single；
    // 证明 NULL thread_id 不会被 thread_size 污染合并成一组。
    #[tokio::test]
    async fn null_thread_distinct_senders_not_merged() {
        let pool = crate::db::test_pool().await;
        let acc = seed_account(&pool).await;
        let inbox = seed_mailbox(&pool, acc, "INBOX", None).await;
        seed_msg(&pool, acc, inbox, 5, "ad1@x.com", None, PROMO, false, "[]").await;
        seed_msg(&pool, acc, inbox, 6, "ad2@x.com", None, PROMO, false, "[]").await;

        let items = mailbox_folded(&pool, inbox, 50).await.unwrap();
        assert_eq!(items.len(), 2, "{items:#?}");
        for key in ["sender:ad1@x.com", "sender:ad2@x.com"] {
            let it = find(&items, key);
            assert_eq!(it.count, 1, "{key}");
            assert!(
                matches!(it.fold_kind, FoldKind::Single),
                "{key} {:?}",
                it.fold_kind
            );
        }
    }

    // 类 4：notification + personal 共享同一 thread → 同 thread 组、count=2。
    #[tokio::test]
    async fn shared_thread_notification_and_personal_group_together() {
        let pool = crate::db::test_pool().await;
        let acc = seed_account(&pool).await;
        let inbox = seed_mailbox(&pool, acc, "INBOX", None).await;
        seed_msg(
            &pool,
            acc,
            inbox,
            7,
            "n@x.com",
            Some("T2"),
            NOTIF,
            false,
            "[]",
        )
        .await;
        seed_msg(
            &pool,
            acc,
            inbox,
            8,
            "p@x.com",
            Some("T2"),
            PERSONAL,
            false,
            "[]",
        )
        .await;

        let items = mailbox_folded(&pool, inbox, 50).await.unwrap();
        assert_eq!(items.len(), 1, "{items:#?}");
        let it = find(&items, "thread:T2");
        assert!(matches!(it.fold_kind, FoldKind::Thread));
        assert_eq!(it.count, 2);
    }

    // 类 5：纯 Sent thread（INBOX 无成员）→ 不出现在 mailbox_folded(INBOX)。
    #[tokio::test]
    async fn sent_only_thread_absent_from_inbox() {
        let pool = crate::db::test_pool().await;
        let acc = seed_account(&pool).await;
        let inbox = seed_mailbox(&pool, acc, "INBOX", None).await;
        let sent = seed_mailbox(&pool, acc, "Sent", Some("sent")).await;
        seed_msg(
            &pool,
            acc,
            sent,
            9,
            "me@x.com",
            Some("T3"),
            PERSONAL,
            false,
            "[]",
        )
        .await;

        let items = mailbox_folded(&pool, inbox, 50).await.unwrap();
        assert!(items.is_empty(), "{items:#?}");
    }

    // 类 6：单封普通邮件（personal）→ single、count=1。flags=[] 即未读 → has_unread=true。
    #[tokio::test]
    async fn single_plain_message() {
        let pool = crate::db::test_pool().await;
        let acc = seed_account(&pool).await;
        let inbox = seed_mailbox(&pool, acc, "INBOX", None).await;
        let id = seed_msg(
            &pool, acc, inbox, 10, "p@x.com", None, PERSONAL, false, "[]",
        )
        .await;

        let items = mailbox_folded(&pool, inbox, 50).await.unwrap();
        assert_eq!(items.len(), 1, "{items:#?}");
        let it = &items[0];
        assert_eq!(it.count, 1);
        assert!(matches!(it.fold_kind, FoldKind::Single));
        assert_eq!(it.representative.id, id);
        assert!(it.has_unread, "flags=[] 无 \\Seen 即未读");
    }

    // 类 7：has_unread。全 \\Seen 的组 → false；含一封未读（flags=[]）的组 → true。
    #[tokio::test]
    async fn has_unread_reflects_group_membership() {
        let pool = crate::db::test_pool().await;
        let acc = seed_account(&pool).await;
        let inbox = seed_mailbox(&pool, acc, "INBOX", None).await;
        // 组 A：sender 组两封都已读 → has_unread = false
        seed_msg(
            &pool,
            acc,
            inbox,
            11,
            "seen@x.com",
            None,
            PROMO,
            false,
            r#"["\\Seen"]"#,
        )
        .await;
        seed_msg(
            &pool,
            acc,
            inbox,
            12,
            "seen@x.com",
            None,
            PROMO,
            false,
            r#"["\\Seen"]"#,
        )
        .await;
        // 组 B：sender 组一封已读 + 一封未读 → has_unread = true
        seed_msg(
            &pool,
            acc,
            inbox,
            13,
            "mix@x.com",
            None,
            PROMO,
            false,
            r#"["\\Seen"]"#,
        )
        .await;
        seed_msg(&pool, acc, inbox, 14, "mix@x.com", None, PROMO, false, "[]").await;

        let items = mailbox_folded(&pool, inbox, 50).await.unwrap();
        assert!(
            !find(&items, "sender:seen@x.com").has_unread,
            "全已读应 false"
        );
        assert!(find(&items, "sender:mix@x.com").has_unread, "含未读应 true");
    }

    // 账户级命令：account_inbox_folded 汇聚所有 inbox 类信箱（special_use NULL 或 'inbox'），
    // 排除 Sent。沿用类 1 场景：thread_size 仍账户级 = 2。
    #[tokio::test]
    async fn account_inbox_folded_excludes_sent_but_counts_account_thread() {
        let pool = crate::db::test_pool().await;
        let acc = seed_account(&pool).await;
        let inbox = seed_mailbox(&pool, acc, "INBOX", None).await;
        let sent = seed_mailbox(&pool, acc, "Sent", Some("sent")).await;
        seed_msg(
            &pool,
            acc,
            inbox,
            1,
            "n@x.com",
            Some("T1"),
            NOTIF,
            false,
            "[]",
        )
        .await;
        seed_msg(
            &pool,
            acc,
            sent,
            2,
            "me@x.com",
            Some("T1"),
            PERSONAL,
            false,
            "[]",
        )
        .await;

        let items = account_inbox_folded(&pool, acc, 50).await.unwrap();
        assert_eq!(items.len(), 1, "Sent 成员不应单独成行 {items:#?}");
        let it = find(&items, "thread:T1");
        assert!(matches!(it.fold_kind, FoldKind::Thread));
        assert_eq!(it.count, 2);
    }
}

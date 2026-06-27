//! 跨 mailbox 的会话查询：同账户内同 thread_id 的全部邮件，按 sent_at 升序。
use uuid::Uuid;

use crate::db::messages::MessageHeader;
use crate::db::Pool;
use crate::error::AppResult;

/// 同账户、同 thread_id 的全部邮件（跨 INBOX/Sent），sent_at 升序、imap_uid 次级升序。
pub async fn list_conversation(
    pool: &Pool,
    account_id: Uuid,
    thread_id: &str,
) -> AppResult<Vec<MessageHeader>> {
    // 列清单与 messages::SELECT_COLUMNS 逐字符一致；MessageHeader 加列时两处同步。
    let sql = r#"
        SELECT
            m.id, m.account_id, m.mailbox_id, m.imap_uid, m.rfc_message_id, m.thread_id,
            m.subject, m.from_addr, m.to_addrs, m.cc_addrs, m.sent_at, m.internal_date,
            m.flags, m.size_bytes, m.has_attachment, m.snippet, m.priority, m.category,
            COALESCE(json_group_array(t.tag) FILTER (WHERE t.tag IS NOT NULL), '[]') AS tags,
            m.body_fetched_at, m.references_header, m.filter_disabled, m.category_locked
        FROM messages m
        LEFT JOIN message_tags t ON t.message_id = m.id
        WHERE m.account_id = ?1 AND m.thread_id = ?2
        GROUP BY m.id
        ORDER BY m.sent_at ASC, m.imap_uid ASC
    "#;
    let rows = sqlx::query_as::<_, MessageHeader>(sql)
        .bind(account_id)
        .bind(thread_id)
        .fetch_all(pool)
        .await?;
    Ok(rows)
}

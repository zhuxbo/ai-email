//! Repository for `suggested_replies` — 物化的建议回复队列。
//! 列队 DTO 的 account_id/subject/from_addr/snippet/sent_at/category/priority 全部 JOIN messages 派生
//! （本表不存这些列）。「已回复」不落库：列队查询排除 send_log.in_reply_to 命中的邮件。

use serde::Serialize;
use sqlx::FromRow;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::db::Pool;
use crate::error::AppResult;

#[derive(Debug, Clone, Serialize, FromRow)]
#[serde(rename_all = "camelCase")]
pub struct SuggestedReply {
    pub id: Uuid,
    pub message_id: Uuid,
    pub account_id: Uuid,
    pub rule_name_snapshot: String,
    pub intent_snapshot: String,
    pub subject: Option<String>,
    pub from_addr: Option<String>,
    pub snippet: Option<String>,
    #[serde(with = "time::serde::rfc3339::option")]
    pub sent_at: Option<OffsetDateTime>,
    pub category: Option<String>,
    pub priority: Option<i32>,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
}

/// 命中入队。UNIQUE(message_id) + INSERT OR IGNORE → 同邮件已有建议（含 dismissed）则静默跳过。
pub async fn insert_if_absent(
    pool: &Pool,
    message_id: Uuid,
    rule_id: Uuid,
    intent_snapshot: &str,
    rule_name_snapshot: &str,
) -> AppResult<()> {
    sqlx::query(
        "INSERT OR IGNORE INTO suggested_replies
           (id, message_id, rule_id, intent_snapshot, rule_name_snapshot, status)
         VALUES (?1, ?2, ?3, ?4, ?5, 'pending')",
    )
    .bind(Uuid::new_v4())
    .bind(message_id)
    .bind(rule_id)
    .bind(intent_snapshot)
    .bind(rule_name_snapshot)
    .execute(pool)
    .await?;
    Ok(())
}

/// 聚合队列（跨账户）：仅 pending，且排除已回复（send_log.in_reply_to 派生）。
pub async fn list_pending(pool: &Pool) -> AppResult<Vec<SuggestedReply>> {
    let rows = sqlx::query_as::<_, SuggestedReply>(
        "SELECT s.id, s.message_id, m.account_id, s.rule_name_snapshot, s.intent_snapshot,
                m.subject, m.from_addr, m.snippet, m.sent_at, m.category, m.priority, s.created_at
         FROM suggested_replies s
         JOIN messages m ON m.id = s.message_id
         WHERE s.status = 'pending'
           AND s.message_id NOT IN
               (SELECT in_reply_to FROM send_log WHERE in_reply_to IS NOT NULL)
         ORDER BY s.created_at DESC",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn dismiss(pool: &Pool, id: Uuid) -> AppResult<()> {
    sqlx::query("UPDATE suggested_replies SET status = 'dismissed' WHERE id = ?1")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

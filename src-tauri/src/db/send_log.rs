//! Repository for the `send_log` table — mandatory audit log of every outbound SMTP send.
//!
//! Per SPEC § 9: "every SMTP send" is logged; rows never deleted. The smtp module always
//! writes a row, success OR failure, so the audit covers attempted-and-blocked too.

use serde::Serialize;
use sqlx::FromRow;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::db::Pool;
use crate::error::AppResult;

#[derive(Debug, Clone, Serialize, FromRow)]
#[serde(rename_all = "camelCase")]
pub struct SendLog {
    pub id: Uuid,
    pub account_id: Uuid,
    pub in_reply_to: Option<Uuid>,
    pub to_addrs: Vec<String>,
    pub subject: String,
    pub ai_assisted: bool,
    #[serde(with = "time::serde::rfc3339")]
    pub sent_at: OffsetDateTime,
    pub smtp_response: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SendLogInsert {
    pub account_id: Uuid,
    pub in_reply_to: Option<Uuid>,
    pub to_addrs: Vec<String>,
    pub subject: String,
    pub ai_assisted: bool,
    pub smtp_response: Option<String>,
}

pub async fn insert(pool: &Pool, row: &SendLogInsert) -> AppResult<SendLog> {
    let stored = sqlx::query_as::<_, SendLog>(
        r#"
        INSERT INTO send_log (
            account_id, in_reply_to, to_addrs, subject, ai_assisted, smtp_response
        )
        VALUES ($1, $2, $3, $4, $5, $6)
        RETURNING id, account_id, in_reply_to, to_addrs, subject, ai_assisted,
                  sent_at, smtp_response
        "#,
    )
    .bind(row.account_id)
    .bind(row.in_reply_to)
    .bind(&row.to_addrs)
    .bind(&row.subject)
    .bind(row.ai_assisted)
    .bind(&row.smtp_response)
    .fetch_one(pool)
    .await?;
    Ok(stored)
}

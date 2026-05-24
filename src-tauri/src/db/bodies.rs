//! Repository for the `message_bodies` table — lazy-fetched plain-text and HTML payloads.
//!
//! One row per `messages.id`. Rows here exist only after the first detail-view click that
//! triggered an IMAP `BODY[]` fetch.

use serde::Serialize;
use sqlx::FromRow;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::db::Pool;
use crate::error::AppResult;
use crate::imap::parse::ParsedBody;

#[derive(Debug, Clone, Serialize, FromRow)]
#[serde(rename_all = "camelCase")]
pub struct MessageBody {
    pub message_id: Uuid,
    pub text_plain: Option<String>,
    pub html: Option<String>,
    #[serde(with = "time::serde::rfc3339")]
    pub fetched_at: OffsetDateTime,
}

pub async fn get(pool: &Pool, message_id: Uuid) -> AppResult<Option<MessageBody>> {
    let row = sqlx::query_as::<_, MessageBody>(
        r#"
        SELECT message_id, text_plain, html, fetched_at
        FROM message_bodies
        WHERE message_id = $1
        "#,
    )
    .bind(message_id)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// Insert or refresh. RETURNING avoids a second roundtrip when the caller wants the new row.
pub async fn upsert(pool: &Pool, message_id: Uuid, body: &ParsedBody) -> AppResult<MessageBody> {
    let row = sqlx::query_as::<_, MessageBody>(
        r#"
        INSERT INTO message_bodies (message_id, text_plain, html)
        VALUES ($1, $2, $3)
        ON CONFLICT (message_id) DO UPDATE
        SET text_plain = EXCLUDED.text_plain,
            html       = EXCLUDED.html,
            fetched_at = NOW()
        RETURNING message_id, text_plain, html, fetched_at
        "#,
    )
    .bind(message_id)
    .bind(&body.text_plain)
    .bind(&body.html)
    .fetch_one(pool)
    .await?;
    Ok(row)
}

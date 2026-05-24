//! Repository for the `accounts` table.
//!
//! Returns only the public-facing [`Account`]; the auth code lives in the OS keychain via
//! [`crate::keychain`] and never appears in this module.

use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::db::Pool;
use crate::error::AppResult;

#[derive(Debug, Clone, Serialize, FromRow)]
#[serde(rename_all = "camelCase")]
pub struct Account {
    pub id: Uuid,
    pub email: String,
    pub display_name: Option<String>,
    pub provider: String,
    pub imap_host: String,
    pub imap_port: i32,
    pub smtp_host: String,
    pub smtp_port: i32,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339::option")]
    pub last_synced_at: Option<OffsetDateTime>,
}

/// What `account_add` writes to the table. The auth code is handled separately.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountInput {
    pub email: String,
    pub display_name: Option<String>,
    pub provider: String,
    pub imap_host: String,
    pub imap_port: i32,
    pub smtp_host: String,
    pub smtp_port: i32,
}

pub async fn insert(pool: &Pool, input: &AccountInput) -> AppResult<Account> {
    let row = sqlx::query_as::<_, Account>(
        r#"
        INSERT INTO accounts (
            email, display_name, provider,
            imap_host, imap_port, smtp_host, smtp_port
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        RETURNING
            id, email, display_name, provider,
            imap_host, imap_port, smtp_host, smtp_port,
            created_at, last_synced_at
        "#,
    )
    .bind(&input.email)
    .bind(&input.display_name)
    .bind(&input.provider)
    .bind(&input.imap_host)
    .bind(input.imap_port)
    .bind(&input.smtp_host)
    .bind(input.smtp_port)
    .fetch_one(pool)
    .await?;
    Ok(row)
}

pub async fn list(pool: &Pool) -> AppResult<Vec<Account>> {
    let rows = sqlx::query_as::<_, Account>(
        r#"
        SELECT
            id, email, display_name, provider,
            imap_host, imap_port, smtp_host, smtp_port,
            created_at, last_synced_at
        FROM accounts
        ORDER BY created_at
        "#,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn delete(pool: &Pool, id: Uuid) -> AppResult<()> {
    sqlx::query("DELETE FROM accounts WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

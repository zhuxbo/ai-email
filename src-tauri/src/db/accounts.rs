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
            id, email, display_name, provider,
            imap_host, imap_port, smtp_host, smtp_port
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
        RETURNING
            id, email, display_name, provider,
            imap_host, imap_port, smtp_host, smtp_port,
            created_at, last_synced_at
        "#,
    )
    .bind(Uuid::new_v4())
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

pub async fn get(pool: &Pool, id: Uuid) -> AppResult<Option<Account>> {
    let row = sqlx::query_as::<_, Account>(
        r#"
        SELECT
            id, email, display_name, provider,
            imap_host, imap_port, smtp_host, smtp_port,
            created_at, last_synced_at
        FROM accounts
        WHERE id = ?1
        "#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

pub async fn update_last_synced(pool: &Pool, id: Uuid) -> AppResult<()> {
    sqlx::query(
        "UPDATE accounts SET last_synced_at = strftime('%Y-%m-%dT%H:%M:%f+00:00', 'now') WHERE id = ?1",
    )
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
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

/// Editable fields for [`update`]. `email` and `provider` are intentionally absent —
/// changing the address is effectively a different account, and the provider drives the
/// host presets only at add-time. The auth code lives in the keychain, not this row.
#[derive(Debug, Clone)]
pub struct AccountUpdate {
    pub display_name: Option<String>,
    pub imap_host: String,
    pub imap_port: i32,
    pub smtp_host: String,
    pub smtp_port: i32,
}

pub async fn update(pool: &Pool, id: Uuid, input: &AccountUpdate) -> AppResult<Account> {
    let row = sqlx::query_as::<_, Account>(
        r#"
        UPDATE accounts
        SET display_name = ?2, imap_host = ?3, imap_port = ?4, smtp_host = ?5, smtp_port = ?6
        WHERE id = ?1
        RETURNING
            id, email, display_name, provider,
            imap_host, imap_port, smtp_host, smtp_port,
            created_at, last_synced_at
        "#,
    )
    .bind(id)
    .bind(&input.display_name)
    .bind(&input.imap_host)
    .bind(input.imap_port)
    .bind(&input.smtp_host)
    .bind(input.smtp_port)
    .fetch_one(pool)
    .await?;
    Ok(row)
}

pub async fn delete(pool: &Pool, id: Uuid) -> AppResult<()> {
    sqlx::query("DELETE FROM accounts WHERE id = ?1")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

/// 轻量存活检查：账户是否仍存在于 DB。用于后台任务在执行付费 AI 调用前确认账户未被删除。
pub async fn account_exists(pool: &Pool, id: Uuid) -> AppResult<bool> {
    let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM accounts WHERE id = ?1)")
        .bind(id)
        .fetch_one(pool)
        .await?;
    Ok(exists)
}

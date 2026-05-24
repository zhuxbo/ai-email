//! PostgreSQL access layer.
//!
//! [`Pool`] is a thin alias around [`sqlx::PgPool`]; the rest of the crate depends only on this
//! re-export so we can swap the underlying driver if needed. [`init`] reads `DATABASE_URL`, opens
//! the pool, and runs every migration in `src-tauri/migrations/` exactly once via
//! [`sqlx::migrate!`].
//!
//! Per-table repositories live in submodules (e.g. [`accounts`]). Each owns its own queries and
//! returns the public-facing struct for that table.

pub mod accounts;

use std::env;
use std::time::Duration;

use sqlx::postgres::PgPoolOptions;

use crate::error::{AppError, AppResult};

/// Shared connection pool. Cloning is cheap (it's an `Arc`).
pub type Pool = sqlx::PgPool;

/// Embeds every `.sql` file under `src-tauri/migrations/` at compile time, then applies any that
/// the target DB hasn't seen. Idempotent.
pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

/// Connect to Postgres and run pending migrations.
///
/// Reads `DATABASE_URL` from the process environment (populated from `.env` by the caller for dev).
/// Returns an [`AppError::Config`] if it's unset so the UI surfaces a clear message rather than a
/// generic connection failure.
pub async fn init() -> AppResult<Pool> {
    let url =
        env::var("DATABASE_URL").map_err(|_| AppError::Config("DATABASE_URL not set".into()))?;

    tracing::info!("connecting to database");
    let pool = PgPoolOptions::new()
        .max_connections(10)
        .min_connections(1)
        .acquire_timeout(Duration::from_secs(5))
        .connect(&url)
        .await?;

    tracing::info!("running migrations");
    MIGRATOR.run(&pool).await?;

    Ok(pool)
}

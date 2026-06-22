//! SQLite access layer.
//!
//! [`Pool`] is a thin alias around [`sqlx::SqlitePool`]; the rest of the crate depends only on
//! this re-export. [`connect`] opens the on-disk database (creating the file + parent dir if
//! missing), enables foreign keys, and runs every migration in `migrations/` exactly once.
//!
//! The DB file lives in the OS app-data dir (path resolved by the Tauri layer in `lib.rs`), so
//! the app is zero-config: first launch creates and migrates the database automatically — no
//! external server, no `DATABASE_URL`.
//!
//! Per-table repositories live in submodules (e.g. [`accounts`]). Each owns its own queries and
//! returns the public-facing struct for that table.

pub mod accounts;
pub mod ai_models;
pub mod ai_results;
pub mod ai_role_defaults;
pub mod auto_reply_rules;
pub mod bodies;
pub mod mailboxes;
pub mod message_tags;
pub mod messages;
pub mod send_log;
pub mod suggested_replies;

use std::path::Path;

use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};

use crate::error::AppResult;

/// Shared connection pool. Cloning is cheap (it's an `Arc`).
pub type Pool = sqlx::SqlitePool;

/// Embeds every `.sql` file under `migrations/` at compile time, then applies any that the
/// target DB hasn't seen. Idempotent.
pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

/// Open the SQLite database at `db_path`, creating the file (and its parent directory) if
/// missing, then run pending migrations. Foreign keys are enforced and WAL is enabled so the
/// UI can read while a sync writes.
pub async fn connect(db_path: &Path) -> AppResult<Pool> {
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let opts = SqliteConnectOptions::new()
        .filename(db_path)
        .create_if_missing(true)
        .foreign_keys(true)
        .journal_mode(SqliteJournalMode::Wal);

    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(opts)
        .await?;

    MIGRATOR.run(&pool).await?;
    Ok(pool)
}

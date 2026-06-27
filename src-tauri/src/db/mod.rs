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
pub mod app_meta;
pub mod auto_reply_rules;
pub mod bodies;
pub mod conversations;
pub mod filter_rules;
pub mod folded;
pub mod mailboxes;
pub mod message_tags;
pub mod messages;
pub mod send_log;
pub mod sender_filters;
pub mod suggested_replies;

use std::path::Path;

use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};

use crate::error::AppResult;

/// Shared connection pool. Cloning is cheap (it's an `Arc`).
pub type Pool = sqlx::SqlitePool;

/// 收件箱类信箱谓词（对 `mailboxes` 行）：`special_use IS NULL`（普通收件箱）或 `'inbox'`。
/// 排除 Sent/Drafts/Trash 等。`folded`（折叠列表）与 `messages`（全部已读）共享，
/// 保证「收件箱范围」口径一致。用在子查询里时以 `mailboxes` 列名直接出现，无表别名。
pub(crate) const INBOX_KIND_PRED: &str = "(special_use IS NULL OR special_use = 'inbox')";

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

    // WAL + NORMAL：在 WAL 模式下 NORMAL 是官方推荐值，断电最多丢最后一笔已提交事务但绝不损坏数据库，
    // 同时显著减少 fsync 次数，降低后台 classify 批量写与前台命令并发时的写锁持有时长。
    let opts = SqliteConnectOptions::new()
        .filename(db_path)
        .create_if_missing(true)
        .foreign_keys(true)
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Normal);

    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(opts)
        .await?;

    MIGRATOR.run(&pool).await?;
    Ok(pool)
}

/// 判断 sqlx 错误是否为 SQLite FOREIGN KEY 约束失败。
///
/// 使用扩展错误码 787（稳定，sqlx 0.8+），不依赖内部英文文案字符串匹配。
/// 并发删除父行时可触发此错误；调用方应 warn 后跳过，而非传播。
pub(crate) fn is_fk_violation(e: &sqlx::Error) -> bool {
    matches!(e, sqlx::Error::Database(db) if db.is_foreign_key_violation())
}

pub(crate) fn is_unique_violation(e: &sqlx::Error) -> bool {
    matches!(e, sqlx::Error::Database(db) if db.is_unique_violation())
}

#[cfg(test)]
pub(crate) mod test_seed;

#[cfg(test)]
pub(crate) async fn test_pool() -> Pool {
    let opts = sqlx::sqlite::SqliteConnectOptions::new()
        .filename(":memory:")
        .foreign_keys(true);
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(opts)
        .await
        .unwrap();
    MIGRATOR.run(&pool).await.unwrap();
    pool
}

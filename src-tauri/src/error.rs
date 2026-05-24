//! Crate-wide error type. Every `#[tauri::command]` returns `Result<T, AppError>` so the
//! frontend gets a single stable shape. Serializes as a plain string for now — switch to a
//! tagged variant once the UI needs to discriminate on error kind.

use serde::{Serialize, Serializer};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("database error: {0}")]
    Db(#[from] sqlx::Error),

    #[error("migration error: {0}")]
    Migrate(#[from] sqlx::migrate::MigrateError),

    #[error("config error: {0}")]
    Config(String),

    #[error("keychain error: {0}")]
    Keychain(String),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

impl Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

pub type AppResult<T> = Result<T, AppError>;

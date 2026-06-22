//! Crate-wide error type. Every `#[tauri::command]` returns `Result<T, AppError>` so the
//! frontend gets a single stable shape.
//!
//! Serialization strategy: variants that wrap third-party error types (sqlx, reqwest, io)
//! emit only a user-friendly category label — internal details (SQL fragments, file paths,
//! OS error codes) are logged via `tracing` and never cross the FFI boundary.
//! String-wrapped variants (Config, Imap, Smtp, Ai, …) are manually constructed with
//! user-facing messages and are passed through as-is.

use serde::{Serialize, Serializer};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("数据库错误")]
    Db(#[from] sqlx::Error),

    #[error("数据库迁移错误")]
    Migrate(#[from] sqlx::migrate::MigrateError),

    #[error("config error: {0}")]
    Config(String),

    #[error("keychain error: {0}")]
    Keychain(String),

    #[error("imap error: {0}")]
    Imap(String),

    #[error("smtp error: {0}")]
    Smtp(String),

    #[error("mail parse error: {0}")]
    MailParse(String),

    #[error("ai provider error: {0}")]
    Ai(String),

    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("http error")]
    Http(#[from] reqwest::Error),

    #[error("io error")]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

impl Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        // Log internal detail before stripping it.
        match self {
            AppError::Db(inner) => tracing::error!(detail = %inner, "database error"),
            AppError::Migrate(inner) => tracing::error!(detail = %inner, "migration error"),
            AppError::Http(inner) => tracing::error!(detail = %inner, "http error"),
            AppError::Io(inner) => tracing::error!(detail = %inner, "io error"),
            _ => {}
        }
        serializer.serialize_str(&self.to_string())
    }
}

pub type AppResult<T> = Result<T, AppError>;

#[cfg(test)]
mod tests {
    use serde_json;

    use super::*;

    // #45: 序列化给前端的消息只返回归类摘要，不暴露底层 sqlx / reqwest 原文
    #[test]
    fn db_error_serializes_category_only_no_internal_detail() {
        // sqlx::Error::RowNotFound 的 Display 是：
        //   "no rows returned by a query that expected to return at least one row"
        // 这是 sqlx 内部实现细节，不应原样出现在前端
        let err = AppError::Db(sqlx::Error::RowNotFound);
        let serialized = serde_json::to_string(&err).unwrap();
        assert!(
            !serialized.contains("no rows returned"),
            "sqlx RowNotFound 底层原文不应暴露给前端: {serialized}"
        );
        // 序列化结果应只是用户友好的归类标识（中文）
        assert!(
            serialized.contains("数据库"),
            "应为中文用户友好摘要: {serialized}"
        );
    }

    #[test]
    fn io_error_serializes_category_only_no_internal_detail() {
        use std::io;
        // io::Error 的 Display 可能包含 OS 错误细节如 "No such file or directory (os error 2)"
        let err = AppError::Io(io::Error::new(
            io::ErrorKind::NotFound,
            "secret/path/to/key",
        ));
        let serialized = serde_json::to_string(&err).unwrap();
        // 不应暴露文件路径或 OS 错误原文
        assert!(
            !serialized.contains("secret/path/to/key"),
            "IO 错误内部路径不应暴露给前端: {serialized}"
        );
        assert!(
            serialized.contains("系统") || serialized.contains("io"),
            "应包含用户友好的错误类别标识: {serialized}"
        );
    }

    #[test]
    fn user_facing_string_errors_pass_through_for_display() {
        // Config/Imap/Smtp/Ai 等 String 包装变体已是人工构造的用户友好消息，应完整保留
        let err = AppError::Config("请先配置账户".into());
        let serialized = serde_json::to_string(&err).unwrap();
        assert!(
            serialized.contains("请先配置账户"),
            "Config 错误的用户友好消息应保留: {serialized}"
        );
    }
}

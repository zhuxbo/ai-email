//! Tauri command handlers. Every cross-FFI call surface lives here.
//!
//! Naming: `snake_case` in Rust → automatically `camelCase` on the TS side.
//! Every command returns `Result<T, crate::error::AppError>`; the error serializes to a string
//! so the UI gets one shape to render.

pub mod accounts;
pub mod ai;
pub mod ai_config;
pub mod auto_reply;
pub mod mail;

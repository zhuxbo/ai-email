//! Tauri 2 entrypoint. Wires up tracing, env loading, the Postgres pool, and command handlers.
//!
//! The frontend NEVER speaks to IMAP / SMTP / Anthropic / PG directly — every cross-process call
//! lands on a `#[tauri::command]` exported from `crate::commands` (added per sprint).

pub mod ai;
pub mod commands;
pub mod db;
pub mod error;
pub mod imap;
pub mod keychain;

use tauri::Manager;
use tracing_subscriber::EnvFilter;

use crate::db::Pool;

/// Shared state attached to the Tauri app handle. Cloning the pool is cheap (`Arc` inside).
pub struct AppState {
    pub db: Pool,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    init_tracing();
    // Dev convenience: load .env so `DATABASE_URL` etc. are available. Failure is non-fatal —
    // in release builds env vars come from the OS / launchd.
    if let Err(e) = dotenvy::dotenv() {
        if !e.not_found() {
            tracing::warn!(error = %e, "failed to load .env");
        }
    }

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let pool = tauri::async_runtime::block_on(db::init())?;
            app.manage(AppState { db: pool });
            tracing::info!("app state initialized");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::accounts::accounts_list,
            commands::accounts::account_add,
            commands::accounts::account_remove,
            commands::mail::inbox_sync,
            commands::mail::mailboxes_list,
            commands::mail::messages_list,
            commands::mail::message_get,
            commands::mail::message_body,
            commands::ai::ai_summarize,
            commands::ai_config::models_list,
            commands::ai_config::model_add,
            commands::ai_config::model_remove,
            commands::ai_config::role_defaults_list,
            commands::ai_config::role_default_set,
            commands::ai_config::role_default_clear,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .try_init();
}

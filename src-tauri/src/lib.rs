//! Tauri 2 entrypoint. Wires up tracing, env loading, the Postgres pool, and command handlers.
//!
//! The frontend NEVER speaks to IMAP / SMTP / Anthropic / PG directly — every cross-process call
//! lands on a `#[tauri::command]` exported from `crate::commands` (added per sprint).

pub mod ai;
pub mod auto_reply;
pub mod commands;
pub mod db;
pub mod error;
pub mod imap;
pub mod keychain;
pub mod smtp;

use tauri::Manager;
use tracing_subscriber::EnvFilter;

use crate::db::Pool;

/// Shared state attached to the Tauri app handle. Cloning the pool is cheap (`Arc` inside).
pub struct AppState {
    pub db: Pool,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Android has no NDK-reachable OS keychain, so `keyring` ships no Android backend.
    // `android-keyring` bridges to the Android KeyStore over JNI and registers itself as
    // keyring's default credential builder, so `crate::keychain` works unchanged on-device.
    #[cfg(target_os = "android")]
    android_keyring::set_android_keyring_credential_builder()
        .expect("failed to register Android keyring credential builder");

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
            // SQLite lives in the OS app-data dir — created + migrated on first launch, so the
            // app is zero-config (no DATABASE_URL, no external server).
            let db_path = app.path().app_data_dir()?.join("ai-email.db");
            let pool = tauri::async_runtime::block_on(db::connect(&db_path))?;
            app.manage(AppState { db: pool });
            tracing::info!(db_path = %db_path.display(), "app state initialized");
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
            commands::ai::ai_classify,
            commands::ai::ai_translate,
            commands::ai::ai_translate_text,
            commands::ai::ai_draft_reply,
            commands::mail::smtp_send,
            commands::mail::message_set_seen,
            commands::mail::message_set_flagged,
            commands::mail::message_delete,
            commands::ai_config::models_list,
            commands::ai_config::model_add,
            commands::ai_config::model_remove,
            commands::ai_config::role_defaults_list,
            commands::ai_config::role_default_set,
            commands::ai_config::role_default_clear,
            commands::auto_reply::auto_reply_rules_list,
            commands::auto_reply::auto_reply_rule_add,
            commands::auto_reply::auto_reply_rule_update,
            commands::auto_reply::auto_reply_rule_remove,
            commands::auto_reply::auto_reply_rule_set_enabled,
            commands::auto_reply::suggested_replies_list,
            commands::auto_reply::suggested_reply_dismiss,
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

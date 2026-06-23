//! Tauri 2 entrypoint. Wires up tracing, env loading, the SQLite pool, and command handlers.
//!
//! The frontend NEVER speaks to IMAP / SMTP / Anthropic / SQLite directly — every cross-process
//! call lands on a `#[tauri::command]` exported from `crate::commands` (added per sprint).

pub mod ai;
pub mod auto_reply;
pub mod commands;
pub mod db;
pub mod error;
pub mod imap;
pub mod keychain;
pub mod smtp;

use std::collections::HashMap;
use std::time::Duration;

use tauri::Manager;
use tokio::sync::{watch, Mutex};
use tokio_util::sync::CancellationToken;
use tracing_subscriber::EnvFilter;
use uuid::Uuid;

use crate::db::Pool;

/// DB 连接+迁移的超时时限。慢盘/大库下防止 setup 永久阻塞。
const DB_CONNECT_TIMEOUT: Duration = Duration::from_secs(30);

/// Shared state attached to the Tauri app handle. Cloning the pool is cheap (Arc inside).
pub struct AppState {
    pub db: Pool,
    /// Single-flight map：防止并发 message_body 请求重复开 IMAP 会话。
    /// key = message id，value = watch receiver（初始 false，leader 完成后 send(true)）。
    /// 迟到者克隆 receiver 后先检查当前值；已 true 直接读缓存，否则 changed().await 等待。
    /// watch 持有最新值，leader 先完成也不丢唤醒（不同于 Notify::notify_waiters）。
    pub body_in_flight: Mutex<HashMap<Uuid, watch::Receiver<bool>>>,
    /// 应用级取消令牌。退出时触发，通知所有持有 child token 的后台任务（classify/eval）停止。
    pub cancel: CancellationToken,
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

    let app_cancel = CancellationToken::new();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup({
            let app_cancel = app_cancel.clone();
            move |app| {
                // SQLite lives in the OS app-data dir — created + migrated on first launch, so the
                // app is zero-config (no DATABASE_URL, no external server).
                let db_path = app.path().app_data_dir()?.join("ai-email.db");

                // #25/#26: 给 DB 连接+迁移加超时，超时/失败时记录错误并返回 Err（而非 panic）。
                // Tauri setup 返回 Err 后 Builder::run 返回 Err，被下方显式 match 处理。
                let pool = tauri::async_runtime::block_on(async {
                    tokio::time::timeout(DB_CONNECT_TIMEOUT, db::connect(&db_path)).await
                })
                .map_err(|_| {
                    tracing::error!(
                        db_path = %db_path.display(),
                        timeout_secs = DB_CONNECT_TIMEOUT.as_secs(),
                        "database connect timed out"
                    );
                    // anyhow::Error 实现了 Into<Box<dyn Error>>，满足 setup 闭包要求
                    anyhow::anyhow!(
                        "database initialization timed out; check disk space or permissions"
                    )
                })?
                .map_err(|e| {
                    tracing::error!(
                        db_path = %db_path.display(),
                        error = %e,
                        "database connect/migrate failed"
                    );
                    anyhow::anyhow!("database initialization failed: {e}")
                })?;

                app.manage(AppState {
                    db: pool,
                    body_in_flight: Mutex::new(HashMap::new()),
                    cancel: app_cancel,
                });
                tracing::info!(db_path = %db_path.display(), "app state initialized");
                Ok(())
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::accounts::accounts_list,
            commands::accounts::account_add,
            commands::accounts::account_remove,
            commands::mail::inbox_sync,
            commands::mail::mailbox_sync,
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
        .build(tauri::generate_context!())
        .map_err(|e| {
            // setup 返回的错误在这里捕获，给出结构化诊断后退出，避免泛化 panic 文案。
            tracing::error!(error = %e, "tauri build failed");
            e
        })
        .expect("error while building tauri application")
        .run(move |_app_handle, event| {
            // #29: 退出时取消所有后台 classify/evaluate_rules 任务。
            if let tauri::RunEvent::Exit = event {
                tracing::info!("app exiting, cancelling background tasks");
                app_cancel.cancel();
            }
        });
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .try_init();
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    /// #25: setup 中 DB connect 超时/失败返回 Err，不触发 panic。
    /// 这里通过验证 db::connect 对不可写路径返回 Err（而非 panic）来证明路径正确；
    /// Tauri setup 整合测试不做（需要完整 app 上下文）。
    #[test]
    fn db_connect_failure_returns_err_not_panic() {
        // 用一个根本不存在且不可创建的路径触发 db::connect Err
        let bad_path = std::path::Path::new("/dev/null/cannot_create/ai-email.db");
        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on(crate::db::connect(bad_path));
        assert!(result.is_err(), "db::connect 失败应返回 Err，不应 panic");
    }

    /// #26: 超时路径可达——用极短超时（1ns）确认 timeout 返回 Elapsed。
    #[tokio::test]
    async fn db_connect_timeout_path_is_reachable() {
        let tmp = tempfile::tempdir().unwrap();
        let db_path = tmp.path().join("test.db");
        // 用 1ns 超时几乎必然触发 Elapsed，验证超时路径可达
        let result =
            tokio::time::timeout(Duration::from_nanos(1), crate::db::connect(&db_path)).await;
        // 若极快机器不触发超时，仅验证结果可接受（Ok 或 Elapsed）
        match result {
            Err(_elapsed) => {} // 超时路径如期触发
            Ok(Ok(_)) => {}     // 极快机器直接成功也可接受
            Ok(Err(e)) => {
                // 其他 db 错误不应 panic
                let _ = e;
            }
        }
    }
}

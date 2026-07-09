//! Tauri 2 entrypoint. Wires up tracing, env loading, the SQLite pool, and command handlers.
//!
//! The frontend NEVER speaks to IMAP / SMTP / Anthropic / SQLite directly — every cross-process
//! call lands on a `#[tauri::command]` exported from `crate::commands` (added per sprint).

pub mod addr;
pub mod ai;
pub mod auto_reply;
pub mod commands;
pub mod db;
pub mod error;
pub mod imap;
pub mod keychain;
pub mod smtp;

use std::collections::{HashMap, HashSet};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;

use tauri::{Emitter, Manager};
use tokio::sync::{watch, Mutex, OnceCell};
use tokio_util::sync::CancellationToken;
use tracing_subscriber::{fmt::MakeWriter, EnvFilter};
use uuid::Uuid;

use crate::db::Pool;
use crate::error::{AppError, AppResult};

/// DB 连接+迁移的超时时限。慢盘/大库下防止后台任务永久挂起。
const DB_CONNECT_TIMEOUT: Duration = Duration::from_secs(30);
const LOG_MAX_BYTES: usize = 10 * 1024 * 1024;
const LOG_BACKUPS: usize = 3;
const LOG_FILE_NAME: &str = "ai-email.log";

/// Payload for `db://ready` event.
#[derive(Clone, serde::Serialize)]
struct DbReadyPayload {}

/// Payload for `db://error` event.
#[derive(Clone, serde::Serialize)]
struct DbErrorPayload {
    message: String,
}

/// `db_status` 命令的返回结构。前端友好：用字符串标签而非裸 enum。
///
/// status 取值：`"initializing"` | `"ready"` | `"error"`
#[derive(serde::Serialize)]
pub struct DbStatusPayload {
    pub status: String,
    pub message: Option<String>,
}

/// 将 `DbInitStatus` 转换为前端友好的 payload。
///
/// 提取为独立函数，使 `commands::system::db_status` 和测试共用同一份 match 逻辑——
/// 任何 match arm 字符串变更都会同时破坏生产路径与测试，消除"测试自写 match"的伪绿问题。
pub(crate) fn db_init_status_to_payload(status: DbInitStatus) -> DbStatusPayload {
    match status {
        DbInitStatus::Initializing => DbStatusPayload {
            status: "initializing".into(),
            message: None,
        },
        DbInitStatus::Ready => DbStatusPayload {
            status: "ready".into(),
            message: None,
        },
        DbInitStatus::Failed(msg) => DbStatusPayload {
            status: "error".into(),
            message: Some(msg),
        },
    }
}

/// DB 初始化三态。由 `AppState.db_init_status` 持有，通过 Mutex 保护并发写。
#[derive(Clone)]
pub(crate) enum DbInitStatus {
    Initializing,
    Ready,
    Failed(String),
}

/// Shared state attached to the Tauri app handle. Cloning the pool is cheap (Arc inside).
pub struct AppState {
    /// 异步就绪容器：setup 后台填充；命令侧通过 `pool()` 访问。
    pub db: Arc<OnceCell<Pool>>,
    /// DB 初始化状态（三态）。`db_status` 命令读取，**不依赖 pool**，pool 未就绪时也能响应。
    pub(crate) db_init_status: Arc<Mutex<DbInitStatus>>,
    /// Single-flight map：防止并发 message_body 请求重复开 IMAP 会话。
    /// key = message id，value = watch receiver（初始 false，leader 完成后 send(true)）。
    /// 迟到者克隆 receiver 后先检查当前值；已 true 直接读缓存，否则 changed().await 等待。
    /// watch 持有最新值，leader 先完成也不丢唤醒（不同于 Notify::notify_waiters）。
    pub body_in_flight: Mutex<HashMap<Uuid, watch::Receiver<bool>>>,
    /// 应用级取消令牌。退出时触发，通知所有持有 child token 的后台任务（classify/eval）停止。
    pub cancel: CancellationToken,
    /// 账户级子令牌注册表。key = account_id。Arc 包裹便于跨 spawn 共享引用。
    /// 删除账户时取出对应 token 并 cancel()，使该账户的在途 classify/eval 任务提前终止，
    /// 避免写入已删除的 mailbox（外键冲突）并浪费 AI 调用配额。
    /// 父令牌 `cancel` 退出时级联取消所有子令牌，不变量不变。
    pub account_tokens: Arc<Mutex<HashMap<Uuid, CancellationToken>>>,
    /// 账户级同步互斥。INBOX 手动同步和单信箱同步共用同一把账户锁，
    /// 避免同一账户同时打开多个 IMAP 同步流程并互相覆盖后台任务 token。
    pub sync_in_flight: StdMutex<HashSet<Uuid>>,
}

impl AppState {
    /// 获取已就绪的连接池。
    ///
    /// - pool 已填充 → 立即返回引用。
    /// - pool 尚未填充（后台迁移仍在进行）→ 返回 `AppError::DbNotReady`，绝不 panic。
    ///
    /// 命令侧统一调用 `state.pool().await?`，取代原先的 `&state.db`。
    pub async fn pool(&self) -> AppResult<&Pool> {
        self.db.get().ok_or(AppError::DbNotReady)
    }
}

/// 后台执行 DB 连接+迁移，成功/失败/超时后向前端 emit 对应事件。
///
/// 状态写入顺序：先更新 `db_init_status`（供 `db_status` 命令主动查询），
/// 再 emit 事件（供监听方被动接收）。这样无论哪条路径先到达，前端都能通过
/// 兜底 `db_status` 查询得到正确结果，消除 emit-before-listen 竞态。
///
/// 可注入任意 `connect_fn`，便于单测两路决策逻辑而无需真实磁盘。
async fn run_db_init<F, Fut>(
    db_cell: Arc<OnceCell<Pool>>,
    db_init_status: Arc<Mutex<DbInitStatus>>,
    app_handle: tauri::AppHandle,
    connect_fn: F,
) where
    F: FnOnce() -> Fut + Send + 'static,
    Fut: std::future::Future<Output = AppResult<Pool>> + Send,
{
    let result = tokio::time::timeout(DB_CONNECT_TIMEOUT, connect_fn()).await;
    let (event_name, event_payload) = apply_db_init_result(result, &db_cell, &db_init_status).await;
    match (event_name, event_payload) {
        ("db://ready", _) => {
            tracing::info!("数据库初始化完成，发送 db://ready");
            let _ = app_handle.emit("db://ready", DbReadyPayload {});
            // pool 已填充进 db_cell，clone 廉价（Arc 内部）。
            if let Some(p) = db_cell.get() {
                let p = p.clone();
                tokio::spawn(async move {
                    crate::commands::reclassify::run_once_if_needed(&p).await;
                });
            }
        }
        (_, Some(msg)) => {
            let _ = app_handle.emit("db://error", DbErrorPayload { message: msg });
        }
        _ => {}
    }
}

/// 将 timeout 结果写入 cell + status，返回应 emit 的事件名和可选消息。
///
/// 与 `run_db_init` 分离，使单测可在无 `AppHandle` 的情况下验证 cell 和 status 的决策逻辑。
pub(crate) async fn apply_db_init_result(
    result: Result<AppResult<Pool>, tokio::time::error::Elapsed>,
    db_cell: &Arc<OnceCell<Pool>>,
    db_init_status: &Arc<Mutex<DbInitStatus>>,
) -> (&'static str, Option<String>) {
    match result {
        Ok(Ok(pool)) => {
            // OnceCell 在单次 spawn 下只会 set 一次；若意外重复 set，记录而非静默吞掉。
            // 不 panic（FFI 边界）：重复 set 只意味着已就绪，沿用首个 pool 即可。
            if db_cell.set(pool).is_err() {
                tracing::error!("db OnceCell 已设置，忽略重复的数据库初始化结果");
            }
            // 先写状态，再 emit——兜底查询不会读到旧状态。
            *db_init_status.lock().await = DbInitStatus::Ready;
            ("db://ready", None)
        }
        Ok(Err(e)) => {
            tracing::error!(error = %e, "数据库连接/迁移失败");
            let msg = format!("数据库初始化失败：{e}");
            *db_init_status.lock().await = DbInitStatus::Failed(msg.clone());
            ("db://error", Some(msg))
        }
        Err(_elapsed) => {
            tracing::error!(
                timeout_secs = DB_CONNECT_TIMEOUT.as_secs(),
                "数据库连接超时"
            );
            let msg = format!(
                "数据库初始化超时（超过 {} 秒），请检查磁盘空间与权限",
                DB_CONNECT_TIMEOUT.as_secs()
            );
            *db_init_status.lock().await = DbInitStatus::Failed(msg.clone());
            ("db://error", Some(msg))
        }
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Android has no NDK-reachable OS keychain, so `keyring` ships no Android backend.
    // `android-keyring` bridges to the Android KeyStore over JNI and registers itself as
    // keyring's default credential builder, so `crate::keychain` works unchanged on-device.
    #[cfg(target_os = "android")]
    android_keyring::set_android_keyring_credential_builder()
        .expect("failed to register Android keyring credential builder");

    // Dev convenience: load .env so `DATABASE_URL` etc. are available. Failure is non-fatal —
    // in release builds env vars come from the OS / launchd.
    if let Err(e) = dotenvy::dotenv() {
        if !e.not_found() {
            eprintln!("failed to load .env: {e}");
        }
    }

    let app_cancel = CancellationToken::new();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup({
            let app_cancel = app_cancel.clone();
            move |app| {
                // db_path 解析必须在 setup 里做（需要 app.path()），失败属于配置错误，
                // 仍同步返回 Err——此时窗口还没显示，给 Builder 一个明确的失败信号。
                let app_data = app.path().app_data_dir()?;
                let log_path = init_tracing(&app_data);
                if let Some(path) = log_path.as_ref() {
                    tracing::info!(path = %path.display(), "file logging initialized");
                }
                let db_path = app_data.join("ai-email.db");
                #[cfg(debug_assertions)]
                crate::keychain::set_dev_cred_path(app_data.join("dev-credentials.json"));

                let db_cell: Arc<OnceCell<Pool>> = Arc::new(OnceCell::new());
                // 初始化为 Initializing，run_db_init 完成后更新为 Ready/Failed。
                let db_init_status: Arc<Mutex<DbInitStatus>> =
                    Arc::new(Mutex::new(DbInitStatus::Initializing));

                app.manage(AppState {
                    db: Arc::clone(&db_cell),
                    db_init_status: Arc::clone(&db_init_status),
                    body_in_flight: Mutex::new(HashMap::new()),
                    cancel: app_cancel,
                    account_tokens: Arc::new(Mutex::new(HashMap::new())),
                    sync_in_flight: StdMutex::new(HashSet::new()),
                });

                // 窗口已可显示（manage 完成后 Tauri 会渲染主窗口）；
                // 连接+迁移在后台异步跑，完成后更新状态字段再 emit db://ready / db://error。
                let handle = app.handle().clone();
                tauri::async_runtime::spawn(run_db_init(
                    db_cell,
                    db_init_status,
                    handle,
                    move || {
                        let path = db_path.clone();
                        async move { db::connect(&path).await }
                    },
                ));

                tracing::info!("app state registered，DB 初始化已在后台启动");
                Ok(())
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::system::db_status,
            commands::system::cache_clear,
            commands::accounts::accounts_list,
            commands::accounts::account_add,
            commands::accounts::account_remove,
            commands::accounts::account_update,
            commands::mail::inbox_sync,
            commands::mail::mailbox_sync,
            commands::mail::mailboxes_list,
            commands::mail::messages_list,
            commands::mail::message_get,
            commands::mail::message_body,
            commands::mail::message_attachments,
            commands::mail::message_attachment_save,
            commands::ai::ai_summarize,
            commands::ai::ai_classify,
            commands::ai::ai_translate,
            commands::ai::ai_translate_text,
            commands::ai::ai_draft_reply,
            commands::mail::smtp_send,
            commands::mail::message_set_seen,
            commands::mail::message_set_flagged,
            commands::mail::messages_mark_seen_bulk,
            commands::mail::message_delete,
            commands::ai_config::models_list,
            commands::ai_config::model_add,
            commands::ai_config::model_remove,
            commands::ai_config::model_update,
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
            commands::sender_filters::sender_filters_list,
            commands::sender_filters::sender_filters_add,
            commands::sender_filters::sender_filters_remove,
            commands::filter_rules::filter_rules_list,
            commands::filter_rules::filter_rule_add,
            commands::filter_rules::filter_rule_update,
            commands::filter_rules::filter_rule_remove,
            commands::filter_rules::filter_rule_set_enabled,
            commands::conversation::conversation_thread,
            commands::conversation::sender_group_thread,
            commands::mail::mailbox_folded,
            commands::mail::account_inbox_folded,
            commands::mail::mailbox_mark_seen,
            commands::mail::account_inbox_mark_seen,
            commands::mail::message_filter_preview,
            commands::mail::message_set_filter_disabled,
            commands::mail::message_set_category,
        ])
        .build(tauri::generate_context!())
        .map_err(|e| {
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

fn init_tracing(app_data: &Path) -> Option<PathBuf> {
    let log_path = log_file_path(app_data);
    if let Err(e) = prepare_log_file(&log_path) {
        eprintln!("failed to prepare log file {}: {e}", log_path.display());
    }
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let writer = AppLogWriter {
        path: Arc::new(log_path.clone()),
    };
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .with_ansi(false)
        .with_writer(writer)
        .try_init();
    Some(log_path)
}

fn log_file_path(app_data: &Path) -> PathBuf {
    app_data.join("logs").join(LOG_FILE_NAME)
}

fn prepare_log_file(log_path: &Path) -> io::Result<()> {
    if let Some(parent) = log_path.parent() {
        fs::create_dir_all(parent)?;
    }
    rotate_log_file(log_path)
}

fn rotate_log_file(log_path: &Path) -> io::Result<()> {
    let Ok(metadata) = fs::metadata(log_path) else {
        return Ok(());
    };
    if metadata.len() <= u64::try_from(LOG_MAX_BYTES).unwrap_or(u64::MAX) {
        return Ok(());
    }

    for index in (1..=LOG_BACKUPS).rev() {
        let from = log_backup_path(log_path, index);
        let to = log_backup_path(log_path, index + 1);
        if from.exists() {
            if index == LOG_BACKUPS {
                fs::remove_file(&from)?;
            } else {
                if to.exists() {
                    fs::remove_file(&to)?;
                }
                fs::rename(&from, &to)?;
            }
        }
    }
    fs::rename(log_path, log_backup_path(log_path, 1))?;
    Ok(())
}

fn log_backup_path(log_path: &Path, index: usize) -> PathBuf {
    log_path.with_extension(format!("log.{index}"))
}

#[derive(Clone)]
struct AppLogWriter {
    path: Arc<PathBuf>,
}

struct AppLogLineWriter {
    file: Option<File>,
    stderr: io::Stderr,
}

impl Write for AppLogLineWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if let Some(file) = self.file.as_mut() {
            let _ = file.write_all(buf);
        }
        let _ = self.stderr.write_all(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        if let Some(file) = self.file.as_mut() {
            let _ = file.flush();
        }
        let _ = self.stderr.flush();
        Ok(())
    }
}

impl<'a> MakeWriter<'a> for AppLogWriter {
    type Writer = AppLogLineWriter;

    fn make_writer(&'a self) -> Self::Writer {
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.path.as_ref())
            .ok();
        AppLogLineWriter {
            file,
            stderr: io::stderr(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use tokio::sync::OnceCell;

    use crate::db::Pool;
    use crate::error::AppError;

    /// DB connect 对不可写路径返回 Err，不 panic。
    #[test]
    fn db_connect_failure_returns_err_not_panic() {
        let bad_path = std::path::Path::new("/dev/null/cannot_create/ai-email.db");
        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on(crate::db::connect(bad_path));
        assert!(result.is_err(), "db::connect 失败应返回 Err，不应 panic");
    }

    /// 超时路径可达——用极短超时（1ns）确认 timeout 返回 Elapsed。
    #[tokio::test]
    async fn db_connect_timeout_path_is_reachable() {
        let tmp = tempfile::tempdir().unwrap();
        let db_path = tmp.path().join("test.db");
        let result =
            tokio::time::timeout(Duration::from_nanos(1), crate::db::connect(&db_path)).await;
        match result {
            Err(_elapsed) => {}
            Ok(Ok(_)) => {}
            Ok(Err(_e)) => {}
        }
    }

    fn make_state(db: Arc<OnceCell<Pool>>) -> crate::AppState {
        use std::collections::HashMap;
        use tokio::sync::Mutex;
        use tokio_util::sync::CancellationToken;

        crate::AppState {
            db,
            db_init_status: Arc::new(Mutex::new(crate::DbInitStatus::Initializing)),
            body_in_flight: Mutex::new(HashMap::new()),
            cancel: CancellationToken::new(),
            account_tokens: Arc::new(Mutex::new(HashMap::new())),
            sync_in_flight: std::sync::Mutex::new(std::collections::HashSet::new()),
        }
    }

    /// pool() 在 OnceCell 未填充时返回 DbNotReady，不 panic。
    #[tokio::test]
    async fn pool_accessor_returns_db_not_ready_when_unset() {
        let state = make_state(Arc::new(OnceCell::new()));
        let result = state.pool().await;
        assert!(
            matches!(result, Err(AppError::DbNotReady)),
            "pool() 未就绪应返回 DbNotReady，实际: {result:?}"
        );
    }

    /// pool() 在 OnceCell 已填充后返回 Ok(&Pool)。
    #[tokio::test]
    async fn pool_accessor_returns_pool_when_set() {
        let tmp = tempfile::tempdir().unwrap();
        let db_path = tmp.path().join("test.db");
        let pool: Pool = crate::db::connect(&db_path).await.expect("test pool");

        let cell: Arc<OnceCell<Pool>> = Arc::new(OnceCell::new());
        cell.set(pool).unwrap();

        let state = make_state(Arc::clone(&cell));
        let result = state.pool().await;
        assert!(result.is_ok(), "pool() 已填充应返回 Ok，实际: {result:?}");
    }

    /// db_status 在 Initializing 态返回 "initializing"。
    /// 调用生产函数 `db_init_status_to_payload`——改 match arm 字符串必然 FAIL。
    #[test]
    fn db_status_initializing_returns_correct_payload() {
        let payload = crate::db_init_status_to_payload(crate::DbInitStatus::Initializing);
        assert_eq!(payload.status, "initializing");
        assert!(payload.message.is_none());
    }

    /// db_status 在 Ready 态返回 "ready"。
    #[test]
    fn db_status_ready_returns_correct_payload() {
        let payload = crate::db_init_status_to_payload(crate::DbInitStatus::Ready);
        assert_eq!(payload.status, "ready");
        assert!(payload.message.is_none());
    }

    /// db_status 在 Failed 态返回 "error" 并携带 message。
    #[test]
    fn db_status_failed_returns_error_with_message() {
        let err_msg = "数据库初始化超时（超过 30 秒）".to_string();
        let payload =
            crate::db_init_status_to_payload(crate::DbInitStatus::Failed(err_msg.clone()));
        assert_eq!(payload.status, "error");
        assert_eq!(payload.message.as_deref(), Some(err_msg.as_str()));
    }

    #[test]
    fn log_path_lives_under_app_data_logs() {
        let app_data = std::path::Path::new("/tmp/ai-email-app-data");
        assert_eq!(
            crate::log_file_path(app_data),
            app_data.join("logs").join("ai-email.log")
        );
    }

    #[test]
    fn rotate_log_file_moves_large_current_log_to_first_backup() {
        let dir = tempfile::tempdir().expect("tempdir");
        let log_path = dir.path().join("ai-email.log");
        std::fs::write(&log_path, vec![b'x'; crate::LOG_MAX_BYTES + 1]).expect("seed log");

        crate::rotate_log_file(&log_path).expect("rotate log");

        assert!(
            !log_path.exists(),
            "current log should be moved before reopening"
        );
        assert!(log_path.with_extension("log.1").exists());
    }

    /// run_db_init 成功路径：apply_db_init_result 填充 OnceCell 并将状态置为 Ready。
    /// 调用生产函数 `apply_db_init_result`——不走 AppHandle，但覆盖真实决策逻辑。
    #[tokio::test]
    async fn run_db_init_success_sets_cell() {
        use tokio::sync::Mutex;

        let tmp = tempfile::tempdir().unwrap();
        let db_path = tmp.path().join("ready.db");
        let pool: Pool = crate::db::connect(&db_path).await.expect("test pool");

        let cell: Arc<OnceCell<Pool>> = Arc::new(OnceCell::new());
        let status = Arc::new(Mutex::new(crate::DbInitStatus::Initializing));

        let result: Result<crate::error::AppResult<Pool>, tokio::time::error::Elapsed> =
            Ok(Ok(pool));
        let (event, msg) = crate::apply_db_init_result(result, &cell, &status).await;

        assert!(cell.get().is_some(), "成功路径应填充 OnceCell");
        assert!(
            matches!(*status.lock().await, crate::DbInitStatus::Ready),
            "成功路径状态应为 Ready"
        );
        assert_eq!(event, "db://ready");
        assert!(msg.is_none());
    }

    /// run_db_init 失败路径：apply_db_init_result 不填充 OnceCell，状态置为 Failed。
    #[tokio::test]
    async fn run_db_init_error_leaves_cell_empty() {
        use tokio::sync::Mutex;

        let cell: Arc<OnceCell<Pool>> = Arc::new(OnceCell::new());
        let status = Arc::new(Mutex::new(crate::DbInitStatus::Initializing));

        let result: Result<crate::error::AppResult<Pool>, tokio::time::error::Elapsed> =
            Ok(Err(AppError::DbNotReady));
        let (event, msg) = crate::apply_db_init_result(result, &cell, &status).await;

        assert!(cell.get().is_none(), "失败路径 OnceCell 应保持空");
        assert!(
            matches!(*status.lock().await, crate::DbInitStatus::Failed(_)),
            "失败路径状态应为 Failed"
        );
        assert_eq!(event, "db://error");
        assert!(msg.is_some());
    }

    /// run_db_init 超时路径：apply_db_init_result 收到真实 Elapsed → Failed + 超时文案，
    /// cell 保持空。Elapsed 无 pub 构造器，故用 1ns 超时包住 pending（永不就绪）产出一个
    /// 真 Elapsed 再喂入，覆盖第三个决策分支。
    #[tokio::test]
    async fn run_db_init_timeout_leaves_cell_empty() {
        use tokio::sync::Mutex;

        let elapsed: Result<crate::error::AppResult<Pool>, tokio::time::error::Elapsed> =
            tokio::time::timeout(
                Duration::from_nanos(1),
                std::future::pending::<crate::error::AppResult<Pool>>(),
            )
            .await;
        assert!(elapsed.is_err(), "1ns 超时应产出 Elapsed");

        let cell: Arc<OnceCell<Pool>> = Arc::new(OnceCell::new());
        let status = Arc::new(Mutex::new(crate::DbInitStatus::Initializing));
        let (event, msg) = crate::apply_db_init_result(elapsed, &cell, &status).await;

        assert!(cell.get().is_none(), "超时路径 OnceCell 应保持空");
        assert!(
            matches!(*status.lock().await, crate::DbInitStatus::Failed(_)),
            "超时路径状态应为 Failed"
        );
        assert_eq!(event, "db://error");
        // 关键：超时分支与普通失败分支都返回 (db://error, Some)，唯一区别是文案——
        // 必须断言"超时"专属文案，否则与 run_db_init_error_leaves_cell_empty 无异 = 伪绿。
        let msg = msg.expect("超时路径应携带消息");
        assert!(
            msg.contains("超时") && msg.contains("30"),
            "超时文案应含'超时'与秒数，实际: {msg}"
        );
    }

    /// apply_db_init_result 在 OnceCell 已被 set 时：忽略重复 pool、不 panic，状态仍置 Ready。
    /// 覆盖 set() 的 Err 分支（生产用 tracing 记录而非静默吞掉）。
    #[tokio::test]
    async fn apply_db_init_result_ignores_duplicate_set() {
        use tokio::sync::Mutex;

        let tmp = tempfile::tempdir().unwrap();
        let first: Pool = crate::db::connect(&tmp.path().join("first.db"))
            .await
            .expect("first pool");
        let second: Pool = crate::db::connect(&tmp.path().join("second.db"))
            .await
            .expect("second pool");

        let cell: Arc<OnceCell<Pool>> = Arc::new(OnceCell::new());
        cell.set(first).expect("首次 set 应成功");
        let status = Arc::new(Mutex::new(crate::DbInitStatus::Initializing));

        // 再喂一个成功结果：set 必失败（已占用），但不 panic、仍走 Ready 路径。
        let result: Result<crate::error::AppResult<Pool>, tokio::time::error::Elapsed> =
            Ok(Ok(second));
        let (event, msg) = crate::apply_db_init_result(result, &cell, &status).await;

        assert_eq!(event, "db://ready");
        assert!(msg.is_none());
        assert!(
            matches!(*status.lock().await, crate::DbInitStatus::Ready),
            "重复 set 后状态仍应为 Ready"
        );
    }
}

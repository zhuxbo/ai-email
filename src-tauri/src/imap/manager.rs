//! 每账户 IMAP 长连接复用管理器。
//!
//! 背景：此前每个操作（同步 / 取正文 / 标旗 / 删除）都新开一条 TCP+TLS+LOGIN+ID 连接，
//! 叠加 5 分钟自动同步后单日对 imap.qq.com 登录数百次，触发服务端登录节流，表现为
//! 频繁超时。本管理器为每个账户维持**一条**复用连接：
//!
//! - 所有 IMAP 操作经 [`ImapManager::run`] 在该账户的连接上串行执行（IMAP 会话本身
//!   不可并发，串行同时天然限制了对服务端的连接速率）。
//! - 操作闭包收到一个**所有权租约** [`ImapLease`]（`OwnedMutexGuard`），future 不借用
//!   `run` 的任何局部——绕开 rustc「HRTB future 不满足 `Send`」的限制（Tauri 命令要求
//!   `Send`）。调用侧写 `|lease| Box::pin(async move { let client = lease.client()?; ... })`。
//! - 操作失败且判定为连接级故障（io / 超时 / 连接被关闭）时丢弃旧连接、重连并重试
//!   **一次**；语义性错误（如 "Mails not exist!"）不动连接原样上抛。
//! - 超时后协议流已错位（响应只读了一半），该连接一律视为不可用——`connection_fatal`
//!   依赖 client.rs 所有超时消息含「超时」这一约定。
//! - 后台保活任务周期对空闲连接发 NOOP（见 [`ImapManager::keepalive_pass`]），防止
//!   服务端/NAT 回收半开连接；NOOP 失败即丢弃，下次操作自动重连。
//! - 账户删除 / 配置变更时调用 [`ImapManager::close`] 丢弃连接（配置可能已指向新主机）。
//!
//! 并发模型：`slots` 用 std Mutex 保护 map 的增删（临界区内无 await）；每账户连接放在
//! `Arc<tokio::sync::Mutex<Option<ImapClient>>>` 里。注意：**禁止在操作闭包内再次调用
//! 同账户的 `run`**（自锁死锁）；各调用点（sync / materialize / 命令层）均为顺序调用，无嵌套。

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;

use secrecy::SecretString;
use tokio::sync::{Mutex, OwnedMutexGuard};
use uuid::Uuid;

use crate::db::accounts::Account;
use crate::error::{AppError, AppResult};
use crate::imap::client::ImapClient;

/// 空闲连接保活间隔。60s 足够防止多数 NAT 超时（常见 2-5 分钟）与服务端空闲回收。
pub const KEEPALIVE_INTERVAL: Duration = Duration::from_secs(60);

/// 初次建连失败后的重试退避：短于服务端限流窗口的惩罚间隔，又给瞬态停滞留出恢复时间。
const CONNECT_RETRY_DELAY: Duration = Duration::from_secs(2);

/// 操作闭包的返回类型：不借用 `run` 局部变量的装箱 future（租约按值移入）。
pub type ImapOp<'a, R> = Pin<Box<dyn Future<Output = AppResult<R>> + Send + 'a>>;

struct Slot {
    conn: Arc<Mutex<Option<ImapClient>>>,
}

impl Slot {
    fn new() -> Self {
        Slot {
            conn: Arc::new(Mutex::new(None)),
        }
    }
}

/// 一次操作对该账户连接的所有权租约。future 结束（或被 drop）即释放连接锁。
pub struct ImapLease {
    guard: OwnedMutexGuard<Option<ImapClient>>,
}

impl ImapLease {
    /// 取底层客户端。None 仅在重连竞态下出现（连接刚被丢弃），按错误处理。
    pub fn client(&mut self) -> AppResult<&mut ImapClient> {
        self.guard
            .as_mut()
            .ok_or_else(|| AppError::Imap("IMAP 连接不可用（正在重连）".into()))
    }
}

/// 判定错误是否为连接级故障（连接不可继续使用，需丢弃重连）。
///
/// - io 错误：TCP 层断开，连接必死。
/// - Imap 错误含「超时」：client.rs 所有超时路径的约定文案；超时后协议流错位，必死。
/// - Imap 错误提及 connection/closed/eof：async-imap 在读流时对断连的典型描述。
///
/// 语义性错误（"Mails not exist!"、"not found" 等）不命中任何规则，连接保留复用。
fn connection_fatal(e: &AppError) -> bool {
    if matches!(e, AppError::Io(_)) {
        return true;
    }
    let AppError::Imap(msg) = e else {
        return false;
    };
    let lower = msg.to_lowercase();
    lower.contains("超时")
        || lower.contains("timed out")
        || lower.contains("connection")
        || lower.contains("closed")
        || lower.contains("eof")
}

#[derive(Default)]
pub struct ImapManager {
    slots: StdMutex<HashMap<Uuid, Arc<Slot>>>,
}

impl ImapManager {
    pub fn new() -> Self {
        Self::default()
    }

    fn slot(&self, account_id: Uuid) -> Arc<Slot> {
        let mut slots = self.slots.lock().expect("imap manager slots lock poisoned");
        slots
            .entry(account_id)
            .or_insert_with(|| Arc::new(Slot::new()))
            .clone()
    }

    async fn connect(account: &Account, auth: &SecretString) -> AppResult<ImapClient> {
        let port = u16::try_from(account.imap_port)
            .map_err(|_| AppError::Imap(format!("invalid imap_port: {}", account.imap_port)))?;
        ImapClient::connect(&account.imap_host, port, &account.email, auth).await
    }

    /// 取一条就绪租约：无连接时建立，有则复用。
    async fn acquire(
        slot: &Arc<Slot>,
        account: &Account,
        auth: &SecretString,
    ) -> AppResult<ImapLease> {
        let mut guard = slot.conn.clone().lock_owned().await;
        if guard.is_none() {
            *guard = Some(Self::connect(account, auth).await?);
        }
        Ok(ImapLease { guard })
    }

    /// 在该账户的复用连接上执行 `op`。无连接时先建立；连接级失败时丢弃重连并重试一次。
    ///
    /// 初次建连失败也重试一次（仅限疑似瞬态错误：超时/io；登录被明确拒绝不重试，
    /// 以免向已限流的服务端加压）——实测 QQ 偶发把单个 LOGIN 挂起 ≥45s，而紧接着的
    /// 下一次连接亚秒级成功；不重试会把这种单次停滞直接暴露成用户可见错误。
    ///
    /// `op` 写成 `|lease| Box::pin(async move { let client = lease.client()?; ... })`；
    /// 重试路径会以**新租约**再次调用同一闭包，闭包内不得依赖上次调用的副作用仍生效
    /// （各操作天然幂等：SELECT 幂等、FETCH 无副作用、同步插入 ON CONFLICT DO NOTHING）。
    pub async fn run<'a, R>(
        &'a self,
        account: &'a Account,
        auth: &'a SecretString,
        mut op: impl FnMut(ImapLease) -> ImapOp<'a, R> + Send + 'a,
    ) -> AppResult<R>
    where
        R: Send + 'a,
    {
        let slot = self.slot(account.id);
        let lease = match Self::acquire(&slot, account, auth).await {
            Ok(lease) => lease,
            Err(e) if connection_fatal(&e) => {
                tracing::warn!(
                    account_id = %account.id,
                    error = %e,
                    "首次建连失败（疑似瞬态），2s 后重试一次"
                );
                tokio::time::sleep(CONNECT_RETRY_DELAY).await;
                Self::acquire(&slot, account, auth).await?
            }
            // 登录被明确拒绝（密码错误/账户异常）等非瞬态错误：直接上抛，不重试。
            Err(e) => return Err(e),
        };
        let result = op(lease).await;
        match result {
            Ok(v) => Ok(v),
            Err(e) if connection_fatal(&e) => {
                tracing::warn!(
                    account_id = %account.id,
                    error = %e,
                    "IMAP 连接疑似断开：丢弃重连并重试一次"
                );
                // 超时/断连后协议流状态不可信，丢弃旧会话（drop 关闭 TCP）再重建。
                {
                    let mut guard = slot.conn.clone().lock_owned().await;
                    *guard = None;
                    *guard = Some(Self::connect(account, auth).await?);
                }
                let lease = Self::acquire(&slot, account, auth).await?;
                let retry = op(lease).await;
                if let Err(retry_err) = &retry {
                    if connection_fatal(retry_err) {
                        // 重试仍失败：连接大概率又不可用，丢弃避免留脏连接给下一个操作。
                        let mut guard = slot.conn.clone().lock_owned().await;
                        *guard = None;
                    }
                }
                retry
            }
            // 语义性错误：连接仍健康，保留复用。
            Err(e) => Err(e),
        }
    }

    /// 丢弃某账户的连接（账户删除 / 配置变更后调用）。尽力 LOGOUT，失败直接丢弃。
    /// 同步函数：用 try_lock 取连接；若有在途操作持有连接（try_lock 失败），slot 已从
    /// map 移除、无新操作会再用它，在途操作结束后连接随 guard drop 自然关闭。
    pub fn close(&self, account_id: Uuid) {
        let slot = {
            let mut slots = self.slots.lock().expect("imap manager slots lock poisoned");
            slots.remove(&account_id)
        };
        let Some(slot) = slot else { return };
        if let Ok(mut conn) = slot.conn.clone().try_lock_owned() {
            if let Some(client) = conn.take() {
                tracing::info!(account_id = %account_id, "closing pooled imap connection");
                // tauri::async_runtime::spawn：close 是同步函数，调用方不一定在 tokio 上下文里。
                tauri::async_runtime::spawn(async move {
                    if let Err(e) = client.logout().await {
                        tracing::debug!(error = %e, "logout on close failed (non-fatal)");
                    }
                });
            }
        }
    }

    /// 后台保活：对所有已建立连接发一次 NOOP。连接正被某个操作持有时（try_lock 失败）
    /// 跳过本轮——操作本身就是活性证明。NOOP 失败/超时即丢弃连接。
    pub async fn keepalive_pass(&self) {
        let slots: Vec<Arc<Slot>> = {
            let slots = self.slots.lock().expect("imap manager slots lock poisoned");
            slots.values().cloned().collect()
        };
        for slot in slots {
            let Ok(mut conn) = slot.conn.try_lock() else {
                continue; // 正被操作持有：跳过本轮
            };
            let Some(client) = conn.as_mut() else {
                continue; // 无连接
            };
            if let Err(e) = client.noop().await {
                tracing::warn!(error = %e, "imap keepalive noop failed; dropping connection");
                *conn = None;
            }
        }
    }
}

/// 后台保活循环：周期调用 [`ImapManager::keepalive_pass`]，直到取消令牌触发。
///
/// 用 `tauri::async_runtime::spawn` 而非 `tokio::spawn`：本函数在 Tauri `setup()`（主线程、
/// 无 tokio runtime 上下文）里被调用，裸 `tokio::spawn` 会 panic（"there is no reactor running"）。
pub fn spawn_keepalive(manager: Arc<ImapManager>, cancel: tokio_util::sync::CancellationToken) {
    tauri::async_runtime::spawn(async move {
        let mut ticker = tokio::time::interval(KEEPALIVE_INTERVAL);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            tokio::select! {
                _ = ticker.tick() => manager.keepalive_pass().await,
                () = cancel.cancelled() => {
                    tracing::debug!("imap keepalive task cancelled");
                    return;
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn io_and_timeout_errors_are_connection_fatal() {
        assert!(connection_fatal(&AppError::Io(std::io::Error::other(
            "reset"
        ))));
        assert!(connection_fatal(&AppError::Imap(
            "IMAP 批量 UID FETCH BODY 超时（2 封 / 240s）".into()
        )));
        assert!(connection_fatal(&AppError::Imap(
            "IMAP TCP 连接超时（15s）：imap.qq.com:993".into()
        )));
        assert!(connection_fatal(&AppError::Imap(
            "connection was closed by the server".into()
        )));
    }

    #[test]
    fn qq_login_rejection_is_not_fatal_so_no_retry() {
        // QQ 明确拒绝登录（密码错/限流）：非瞬态，不应触发建连重试。
        let e = AppError::Imap(
            "no response: code: None, info: Some(\"Login fail. Account is abnormal, service is not open, password is incorrect, login frequency limited, or system is busy. More information at https://help.mail.qq.com/detail/108/1023\")".into(),
        );
        assert!(!connection_fatal(&e));
    }

    #[test]
    fn login_timeout_is_fatal_so_connect_retries() {
        // 单次 LOGIN 挂起超时：瞬态，应触发建连重试（实测下一次亚秒级成功）。
        assert!(connection_fatal(&AppError::Imap(
            "IMAP TLS/登录超时（45s）：imap.qq.com:993".into()
        )));
    }

    #[test]
    fn semantic_errors_keep_connection_alive() {
        // QQ 删除竞态、找不到邮件/信箱等语义错误：连接本身健康，不应触发重连。
        assert!(!connection_fatal(&AppError::Imap(
            "Mails not exist!".into()
        )));
        assert!(!connection_fatal(&AppError::Imap("INBOX not found".into())));
        assert!(!connection_fatal(&AppError::Config("not found".into())));
        assert!(!connection_fatal(&AppError::Db(sqlx::Error::RowNotFound)));
    }

    #[tokio::test]
    async fn close_drops_slot_so_next_run_recreates() {
        // close 后 slot 从 map 移除；再次 slot() 会建新 slot（间接验证：不 panic、map 收缩）。
        let manager = ImapManager::new();
        let id = Uuid::new_v4();
        let s1 = manager.slot(id);
        let s2 = manager.slot(id);
        assert!(Arc::ptr_eq(&s1, &s2), "同账户应命中同一 slot");
        manager.close(id);
        let count = manager.slots.lock().unwrap().len();
        assert_eq!(count, 0, "close 后 map 应为空");
    }
}

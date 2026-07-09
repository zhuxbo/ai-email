//! `account_*` / `accounts_*` Tauri commands.
//!
//! Add-account flow:
//!   1. Validate the form fields (port range, known provider).
//!   2. INSERT into DB → get fresh UUID.
//!   3. Wrap the auth code in [`SecretString`] (so it stops appearing in logs).
//!   4. Store it in the OS keychain on a blocking thread.
//!   5. On keychain failure, DELETE the row so the user can retry from a clean slate.
//!
//! Remove-account flow:
//!   1. Best-effort delete the keychain credential first (warn on failure, never abort).
//!   2. Delete the DB row (cascade removes related data).
//!
//! The keychain is the source of truth for secrets; DB never sees the auth code.

use std::future::Future;

use secrecy::SecretString;
use serde::Deserialize;
use tauri::State;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::addr;
use crate::db::accounts::{self, Account, AccountInput, AccountUpdate};
use crate::db::Pool;
use crate::error::{AppError, AppResult};
use crate::keychain;
use crate::AppState;

/// 取消指定账户的在途后台任务。
///
/// 从注册表中移除该账户的 CancellationToken 并调用 cancel()；若无对应条目则 no-op。
/// 调用方保证在 DB 删除之前调用，以避免任务写入已删除的 mailbox 触发外键冲突。
pub(crate) async fn cancel_account_tasks(
    tokens: &Mutex<std::collections::HashMap<Uuid, CancellationToken>>,
    id: Uuid,
) {
    if let Some(token) = tokens.lock().await.remove(&id) {
        token.cancel();
        tracing::info!(account_id = %id, "cancelled in-flight background tasks for account");
    }
}

/// Known email provider identifiers.
const KNOWN_PROVIDERS: &[&str] = &["qq", "exmail", "gmail", "imap"];

/// What the add-account form sends across the FFI. `authCode` is split out and never
/// round-tripped back to the frontend.
///
/// `Debug` is implemented manually so that `auth_code` never appears in log output.
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddAccountForm {
    pub email: String,
    pub display_name: Option<String>,
    pub provider: String,
    pub imap_host: String,
    pub imap_port: i32,
    pub smtp_host: String,
    pub smtp_port: i32,
    pub auth_code: String,
}

impl std::fmt::Debug for AddAccountForm {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AddAccountForm")
            .field("email", &self.email)
            .field("display_name", &self.display_name)
            .field("provider", &self.provider)
            .field("imap_host", &self.imap_host)
            .field("imap_port", &self.imap_port)
            .field("smtp_host", &self.smtp_host)
            .field("smtp_port", &self.smtp_port)
            .field("auth_code", &"[redacted]")
            .finish()
    }
}

/// Update-account form. `email` and `provider` are fixed (not editable); `authCode` is
/// optional — `None` / empty means "keep the existing credential in the keychain".
///
/// `Debug` is implemented manually so that `auth_code` never appears in log output.
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateAccountForm {
    pub display_name: Option<String>,
    pub imap_host: String,
    pub imap_port: i32,
    pub smtp_host: String,
    pub smtp_port: i32,
    pub auth_code: Option<String>,
}

impl std::fmt::Debug for UpdateAccountForm {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UpdateAccountForm")
            .field("display_name", &self.display_name)
            .field("imap_host", &self.imap_host)
            .field("imap_port", &self.imap_port)
            .field("smtp_host", &self.smtp_host)
            .field("smtp_port", &self.smtp_port)
            .field("auth_code", &self.auth_code.as_ref().map(|_| "[redacted]"))
            .finish()
    }
}

/// Validate port number is in the legal TCP range 1–65535.
fn validate_port(port: i32, field: &str) -> AppResult<()> {
    if !(1..=65535).contains(&port) {
        return Err(AppError::Config(format!(
            "{field} must be between 1 and 65535, got {port}"
        )));
    }
    Ok(())
}

fn validate_required(value: &str, field: &str) -> AppResult<()> {
    if value.trim().is_empty() {
        return Err(AppError::Config(format!("{field} must not be empty")));
    }
    Ok(())
}

fn validate_host(host: &str, field: &str) -> AppResult<()> {
    validate_required(host, field)?;
    if host.trim().chars().any(char::is_whitespace) {
        return Err(AppError::Config(format!(
            "{field} must not contain whitespace"
        )));
    }
    Ok(())
}

fn validate_bare_email(email: &str) -> AppResult<()> {
    let trimmed = email.trim();
    let lower = trimmed.to_ascii_lowercase();
    match addr::extract_email(Some(trimmed)) {
        Some(parsed) if parsed == lower => Ok(()),
        _ => Err(AppError::Config(
            "email must be a valid bare address".into(),
        )),
    }
}

fn validate_add_account_form(form: &AddAccountForm) -> AppResult<()> {
    validate_bare_email(&form.email)?;
    validate_required(&form.provider, "provider")?;
    validate_host(&form.imap_host, "imap_host")?;
    validate_host(&form.smtp_host, "smtp_host")?;
    validate_required(&form.auth_code, "auth_code")?;
    validate_port(form.imap_port, "imap_port")?;
    validate_port(form.smtp_port, "smtp_port")?;
    let provider = form.provider.trim().to_ascii_lowercase();
    if !KNOWN_PROVIDERS.contains(&provider.as_str()) {
        return Err(AppError::Config(format!(
            "unknown provider: {} (must be one of: {})",
            form.provider,
            KNOWN_PROVIDERS.join(", ")
        )));
    }
    Ok(())
}

fn validate_update_account_form(form: &UpdateAccountForm) -> AppResult<()> {
    validate_host(&form.imap_host, "imap_host")?;
    validate_host(&form.smtp_host, "smtp_host")?;
    validate_port(form.imap_port, "imap_port")?;
    validate_port(form.smtp_port, "smtp_port")
}

#[tauri::command]
pub async fn accounts_list(state: State<'_, AppState>) -> AppResult<Vec<Account>> {
    accounts::list(state.pool().await?).await
}

#[tauri::command]
pub async fn account_add(state: State<'_, AppState>, form: AddAccountForm) -> AppResult<Account> {
    validate_add_account_form(&form)?;

    let pool = state.pool().await?;
    let auth = SecretString::from(form.auth_code.trim().to_string());
    let input = AccountInput {
        email: form.email.trim().to_string(),
        display_name: form.display_name,
        provider: form.provider.trim().to_ascii_lowercase(),
        imap_host: form.imap_host.trim().to_string(),
        imap_port: form.imap_port,
        smtp_host: form.smtp_host.trim().to_string(),
        smtp_port: form.smtp_port,
    };

    let account = accounts::insert(pool, &input).await?;
    let id = account.id;

    let stored = tokio::task::spawn_blocking(move || keychain::store_auth_code(id, &auth))
        .await
        .map_err(|e| AppError::Other(anyhow::anyhow!(e)))?;

    if let Err(e) = stored {
        // Roll the DB back so the user isn't stuck with a row whose secret is missing.
        if let Err(cleanup) = accounts::delete(pool, id).await {
            tracing::error!(error = ?cleanup, "failed to roll back account row after keychain failure");
        }
        return Err(e);
    }

    tracing::info!(account_id = %account.id, email = %account.email, "account added");
    Ok(account)
}

#[tauri::command]
pub async fn account_remove(state: State<'_, AppState>, id: Uuid) -> AppResult<()> {
    // 取消该账户的在途后台任务（classify/eval）：先于 DB 删除，避免任务写入已删除的 mailbox
    // 触发外键冲突，同时节省 AI 调用配额。无在途任务时 no-op 不报错。
    cancel_account_tasks(&state.account_tokens, id).await;

    // #11: delete keychain credential first (best-effort — warn on failure, never abort).
    // This avoids leaving an unreachable orphan auth-code if the DB delete succeeds but
    // the keychain delete fails. The delete-warn-not-fail invariant is intentional: a
    // failed keychain cleanup must not block the user from completing account removal.
    //
    // Reverse orphan trade-off: if the keychain delete succeeds but the subsequent DB
    // `delete` call (below) fails and propagates via `?`, the DB row survives but the
    // credential is already gone. This is the more acceptable direction — DB deletes
    // rarely fail and are retryable, and `keychain::delete_auth_code` is idempotent
    // (NoEntry → Ok) so a retry will self-heal the keychain side.
    let keychain_result = tokio::task::spawn_blocking(move || keychain::delete_auth_code(id))
        .await
        .map_err(|e| AppError::Other(anyhow::anyhow!(e)));
    match keychain_result {
        Ok(Ok(())) => {}
        Ok(Err(e)) => {
            tracing::warn!(account_id = %id, error = %e, "keychain delete_auth_code failed; proceeding with DB remove");
        }
        Err(e) => {
            tracing::warn!(account_id = %id, error = %e, "spawn_blocking for keychain delete failed; proceeding with DB remove");
        }
    }

    accounts::delete(state.pool().await?, id).await?;
    tracing::info!(account_id = %id, "account removed");
    Ok(())
}

/// Update an account's editable fields. `email` / `provider` are fixed. The auth code is
/// only re-written to the keychain when `form.auth_code` is a non-empty value, so a blank
/// field preserves the existing credential — mirrors how the edit UI prefills everything
/// except the secret.
#[tauri::command]
pub async fn account_update(
    state: State<'_, AppState>,
    id: Uuid,
    form: UpdateAccountForm,
) -> AppResult<Account> {
    validate_update_account_form(&form)?;

    let input = AccountUpdate {
        display_name: form.display_name,
        imap_host: form.imap_host.trim().to_string(),
        imap_port: form.imap_port,
        smtp_host: form.smtp_host.trim().to_string(),
        smtp_port: form.smtp_port,
    };
    let pool = state.pool().await?;
    // 授权码仅在提供了非空值时覆盖 keychain；留空 = 保持原值不变。keychain 写失败时
    // apply_account_update 会把 DB 回滚到旧值（见其文档），避免「新配置 + 旧凭据」的部分成功。
    let secret = super::secret_to_store(form.auth_code);
    let account = apply_account_update(pool, id, &input, secret, |code| async move {
        let auth = SecretString::from(code);
        tokio::task::spawn_blocking(move || keychain::store_auth_code(id, &auth))
            .await
            .map_err(|e| AppError::Other(anyhow::anyhow!(e)))?
    })
    .await?;

    tracing::info!(account_id = %account.id, "account updated");
    Ok(account)
}

/// 提交一次账户更新：写入新的非密字段，并在 `secret` 为非空值时把新授权码写入 keychain。
///
/// 不变量：keychain 写入失败时，DB 行回滚到更新前的旧值，使账户停留在「旧配置 + 旧凭据」
/// 的一致状态、可安全重试 —— 而不是「新配置 + 旧凭据」的部分成功。`store` 是唯一的副作用
/// 注入点，测试以一个必失败的实现覆盖回滚路径，无需真实 keychain。
async fn apply_account_update<F, Fut>(
    pool: &Pool,
    id: Uuid,
    input: &AccountUpdate,
    secret: Option<String>,
    store: F,
) -> AppResult<Account>
where
    F: FnOnce(String) -> Fut,
    Fut: Future<Output = AppResult<()>>,
{
    // 仅当确实要改授权码时，才先抓旧值用于回滚；无 secret 时不触碰 keychain，无需回滚。
    let prev = if secret.is_some() {
        Some(
            accounts::get(pool, id)
                .await?
                .ok_or_else(|| AppError::Config(format!("account not found: {id}")))?,
        )
    } else {
        None
    };

    let updated = accounts::update(pool, id, input).await?;

    if let Some(code) = secret {
        if let Err(e) = store(code).await {
            // keychain 写失败：把 DB 回滚到旧值，保持「旧配置 + 旧凭据」一致、可安全重试。
            if let Some(prev) = prev {
                let rollback = AccountUpdate {
                    display_name: prev.display_name,
                    imap_host: prev.imap_host,
                    imap_port: prev.imap_port,
                    smtp_host: prev.smtp_host,
                    smtp_port: prev.smtp_port,
                };
                if let Err(re) = accounts::update(pool, id, &rollback).await {
                    tracing::error!(account_id = %id, error = ?re, "keychain 写失败后回滚账户行也失败");
                }
            }
            return Err(e);
        }
    }

    Ok(updated)
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Arc;

    use tokio::sync::Mutex;
    use tokio_util::sync::CancellationToken;
    use uuid::Uuid;

    use super::{
        accounts, apply_account_update, cancel_account_tasks, validate_add_account_form,
        validate_port, validate_update_account_form,
    };
    use super::{AccountInput, AccountUpdate, AddAccountForm, UpdateAccountForm};
    use crate::db::{self, Pool};
    use crate::error::AppError;

    // ---- 账户删除时取消在途后台任务 ----

    /// 注册账户子令牌 → 调用 cancel_account_tasks → 断言该令牌 is_cancelled()，且其他账户不受影响。
    #[tokio::test]
    async fn account_cancel_only_affects_target_account() {
        let tokens: Arc<Mutex<HashMap<Uuid, CancellationToken>>> =
            Arc::new(Mutex::new(HashMap::new()));

        let app_cancel = CancellationToken::new();
        let id_a = Uuid::new_v4();
        let id_b = Uuid::new_v4();

        // 注册两个账户的子令牌（继承自应用级 cancel）
        let token_a = app_cancel.child_token();
        let token_b = app_cancel.child_token();
        tokens.lock().await.insert(id_a, token_a.clone());
        tokens.lock().await.insert(id_b, token_b.clone());

        // 通过生产 helper 取消账户 A
        cancel_account_tasks(&tokens, id_a).await;

        // A 的令牌被取消
        assert!(token_a.is_cancelled(), "账户 A 的令牌应被取消");
        // B 的令牌不受影响（粒度隔离）
        assert!(!token_b.is_cancelled(), "账户 B 的令牌不应被取消");
        // B 的令牌仍在注册表中
        assert!(
            tokens.lock().await.contains_key(&id_b),
            "账户 B 的令牌应保留在注册表中"
        );
    }

    /// 退出路径：app_cancel.cancel() 级联取消所有子令牌（覆盖不变量，与 helper 无关）。
    #[tokio::test]
    async fn app_cancel_cascades_to_account_tokens() {
        let app_cancel = CancellationToken::new();

        let token_a = app_cancel.child_token();
        let token_b = app_cancel.child_token();

        // 两个子令牌都未取消
        assert!(!token_a.is_cancelled());
        assert!(!token_b.is_cancelled());

        // 应用退出：取消父令牌
        app_cancel.cancel();

        // 所有子令牌同步被级联取消
        assert!(token_a.is_cancelled(), "应用退出应级联取消账户 A 的令牌");
        assert!(token_b.is_cancelled(), "应用退出应级联取消账户 B 的令牌");
    }

    /// 无在途任务时（注册表中无对应条目）cancel_account_tasks 应 no-op，不 panic。
    #[tokio::test]
    async fn cancel_nonexistent_account_is_noop() {
        let tokens: Arc<Mutex<HashMap<Uuid, CancellationToken>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let nonexistent_id = Uuid::new_v4();

        // 通过生产 helper 对空表取消，应安全 no-op
        cancel_account_tasks(&tokens, nonexistent_id).await;
    }

    // ---- #39: port validation ----

    #[test]
    fn port_zero_is_rejected() {
        assert!(matches!(
            validate_port(0, "imap_port"),
            Err(AppError::Config(_))
        ));
    }

    #[test]
    fn port_negative_is_rejected() {
        assert!(matches!(
            validate_port(-1, "smtp_port"),
            Err(AppError::Config(_))
        ));
    }

    #[test]
    fn port_too_large_is_rejected() {
        assert!(matches!(
            validate_port(65536, "imap_port"),
            Err(AppError::Config(_))
        ));
    }

    #[test]
    fn port_boundary_min_is_accepted() {
        assert!(validate_port(1, "imap_port").is_ok());
    }

    #[test]
    fn port_boundary_max_is_accepted() {
        assert!(validate_port(65535, "smtp_port").is_ok());
    }

    #[test]
    fn port_common_imap_is_accepted() {
        assert!(validate_port(993, "imap_port").is_ok());
    }

    #[test]
    fn port_common_smtp_is_accepted() {
        assert!(validate_port(465, "smtp_port").is_ok());
    }

    #[test]
    fn add_account_validation_rejects_blank_identity_fields() {
        let mut form = AddAccountForm {
            email: "   ".into(),
            display_name: None,
            provider: "qq".into(),
            imap_host: "imap.qq.com".into(),
            imap_port: 993,
            smtp_host: "smtp.qq.com".into(),
            smtp_port: 465,
            auth_code: "secret".into(),
        };
        assert!(matches!(
            validate_add_account_form(&form),
            Err(AppError::Config(_))
        ));

        form.email = "user@qq.com".into();
        form.imap_host = "  ".into();
        assert!(matches!(
            validate_add_account_form(&form),
            Err(AppError::Config(_))
        ));

        form.imap_host = "imap.qq.com".into();
        form.auth_code = "  ".into();
        assert!(matches!(
            validate_add_account_form(&form),
            Err(AppError::Config(_))
        ));
    }

    #[test]
    fn add_account_validation_rejects_malformed_email() {
        let form = AddAccountForm {
            email: "not-an-email".into(),
            display_name: None,
            provider: "qq".into(),
            imap_host: "imap.qq.com".into(),
            imap_port: 993,
            smtp_host: "smtp.qq.com".into(),
            smtp_port: 465,
            auth_code: "secret".into(),
        };
        assert!(matches!(
            validate_add_account_form(&form),
            Err(AppError::Config(_))
        ));
    }

    #[test]
    fn update_account_validation_rejects_blank_hosts_but_allows_blank_secret_to_preserve() {
        let mut form = UpdateAccountForm {
            display_name: None,
            imap_host: "imap.qq.com".into(),
            imap_port: 993,
            smtp_host: "smtp.qq.com".into(),
            smtp_port: 465,
            auth_code: Some("   ".into()),
        };
        assert!(validate_update_account_form(&form).is_ok());

        form.smtp_host = " ".into();
        assert!(matches!(
            validate_update_account_form(&form),
            Err(AppError::Config(_))
        ));
    }

    // ---- #11: account_remove ordering is tested behaviourally via integration tests
    // that check the DB row is gone after a remove call.  The keychain path cannot be
    // tested without a real OS keychain, so we document the invariant here rather than
    // mock the entire Tauri State machinery. ----

    // ---- credentials must not appear in Debug output (#2) ----

    #[test]
    fn add_account_form_debug_redacts_auth_code() {
        let form = super::AddAccountForm {
            email: "user@qq.com".into(),
            display_name: Some("Test User".into()),
            provider: "qq".into(),
            imap_host: "imap.qq.com".into(),
            imap_port: 993,
            smtp_host: "smtp.qq.com".into(),
            smtp_port: 465,
            auth_code: "super_secret_auth_code".into(),
        };
        let debug = format!("{:?}", form);
        assert!(
            !debug.contains("super_secret_auth_code"),
            "auth_code must not appear in Debug"
        );
        assert!(
            debug.contains("[redacted]"),
            "Debug must show [redacted] for auth_code"
        );
        assert!(
            debug.contains("user@qq.com"),
            "non-secret fields must still appear in Debug"
        );
        assert!(
            debug.contains("imap.qq.com"),
            "non-secret fields must still appear in Debug"
        );
    }

    #[test]
    fn update_account_form_debug_redacts_auth_code() {
        let form = super::UpdateAccountForm {
            display_name: Some("Test User".into()),
            imap_host: "imap.qq.com".into(),
            imap_port: 993,
            smtp_host: "smtp.qq.com".into(),
            smtp_port: 465,
            auth_code: Some("super_secret_auth_code".into()),
        };
        let debug = format!("{:?}", form);
        assert!(
            !debug.contains("super_secret_auth_code"),
            "auth_code must not appear in Debug"
        );
        assert!(
            debug.contains("[redacted]"),
            "Debug must show [redacted] for a present auth_code"
        );
        assert!(
            debug.contains("imap.qq.com"),
            "non-secret fields must still appear in Debug"
        );
    }

    // ---- L2: update 的 keychain 写失败 → DB 回滚到旧值，不残留新的非密字段 ----

    async fn test_pool() -> Pool {
        let opts = sqlx::sqlite::SqliteConnectOptions::new()
            .filename(":memory:")
            .foreign_keys(true);
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(opts)
            .await
            .unwrap();
        db::MIGRATOR.run(&pool).await.unwrap();
        pool
    }

    fn sample_account_input() -> AccountInput {
        AccountInput {
            email: "user@qq.com".into(),
            display_name: Some("Old Name".into()),
            provider: "qq".into(),
            imap_host: "imap.qq.com".into(),
            imap_port: 993,
            smtp_host: "smtp.qq.com".into(),
            smtp_port: 465,
        }
    }

    /// 与 [`sample_account_input`] 完全不同的新非密字段，便于断言更新是否残留。
    fn changed_update() -> AccountUpdate {
        AccountUpdate {
            display_name: Some("New Name".into()),
            imap_host: "imap.changed.invalid".into(),
            imap_port: 143,
            smtp_host: "smtp.changed.invalid".into(),
            smtp_port: 25,
        }
    }

    /// keychain 写失败时，DB 必须回滚到更新前的旧值，不残留新的非密字段。
    #[tokio::test]
    async fn account_update_rolls_back_db_when_keychain_write_fails() {
        let pool = test_pool().await;
        let original = accounts::insert(&pool, &sample_account_input())
            .await
            .unwrap();

        let result = apply_account_update(
            &pool,
            original.id,
            &changed_update(),
            Some("new_auth_code".into()),
            |_code| async { Err(AppError::Keychain("simulated keychain failure".into())) },
        )
        .await;

        assert!(result.is_err(), "keychain 写失败时整个更新应返回错误");

        // 关键不变量：所有非密字段回到旧值，没有「新配置 + 旧凭据」的部分成功。
        let reloaded = accounts::get(&pool, original.id).await.unwrap().unwrap();
        assert_eq!(
            reloaded.display_name.as_deref(),
            Some("Old Name"),
            "display_name 应回滚到旧值"
        );
        assert_eq!(reloaded.imap_host, "imap.qq.com", "imap_host 应回滚到旧值");
        assert_eq!(reloaded.imap_port, 993, "imap_port 应回滚到旧值");
        assert_eq!(reloaded.smtp_host, "smtp.qq.com", "smtp_host 应回滚到旧值");
        assert_eq!(reloaded.smtp_port, 465, "smtp_port 应回滚到旧值");
    }

    /// keychain 写成功时，新的非密字段落库，返回更新后的账户。
    #[tokio::test]
    async fn account_update_persists_when_keychain_write_succeeds() {
        let pool = test_pool().await;
        let original = accounts::insert(&pool, &sample_account_input())
            .await
            .unwrap();

        let updated = apply_account_update(
            &pool,
            original.id,
            &changed_update(),
            Some("new_auth_code".into()),
            |_code| async { Ok(()) },
        )
        .await
        .expect("keychain 成功时更新应成功");

        assert_eq!(updated.imap_host, "imap.changed.invalid");
        let reloaded = accounts::get(&pool, original.id).await.unwrap().unwrap();
        assert_eq!(reloaded.imap_host, "imap.changed.invalid", "新值应落库");
        assert_eq!(reloaded.smtp_port, 25, "新值应落库");
    }

    /// secret = None（未改授权码）时根本不触碰 keychain：store 闭包不应被调用。
    #[tokio::test]
    async fn account_update_without_secret_skips_keychain() {
        let pool = test_pool().await;
        let original = accounts::insert(&pool, &sample_account_input())
            .await
            .unwrap();

        let updated =
            apply_account_update(&pool, original.id, &changed_update(), None, |_code| async {
                panic!("无新授权码时不应触碰 keychain")
            })
            .await
            .expect("无 secret 的更新应成功");

        assert_eq!(updated.imap_host, "imap.changed.invalid");
        let reloaded = accounts::get(&pool, original.id).await.unwrap().unwrap();
        assert_eq!(reloaded.imap_host, "imap.changed.invalid", "新值应落库");
    }
}

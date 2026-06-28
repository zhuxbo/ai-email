//! 凭据存储：crate 里唯一触碰 OS 钥匙串的地方。
//!
//! - **release**：用 [`keyring`] crate，service = `com.zhuxbo.aiemail`(auth) / `.ai`(api key)，
//!   username = UUID 字符串。
//! - **debug**：旁路钥匙串，改用 app 数据目录下明文 JSON（见 [`dev_store`]），避免每次重建后
//!   重复系统授权。release 编译期剔除全部文件态代码。
//!
//! 函数同步（keyring / 文件 IO 均阻塞）；async 调用方须包 [`tokio::task::spawn_blocking`]。

use secrecy::{ExposeSecret, SecretString};
use uuid::Uuid;

use crate::error::{AppError, AppResult};

#[cfg(debug_assertions)]
mod dev_store;

/// dev 凭据文件路径：启动时由 lib.rs setup 设；测试/手动可用 env `AI_EMAIL_DEV_CRED_FILE` 覆盖。
#[cfg(debug_assertions)]
static DEV_CRED_PATH: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();

/// 启动时由 lib.rs setup 调一次（app_data_dir 已解析处）。二次调用静默忽略（OnceLock::set
/// 返回 Err）是有意的——启动期单次初始化、不该被改。
#[cfg(debug_assertions)]
pub fn set_dev_cred_path(path: std::path::PathBuf) {
    let _ = DEV_CRED_PATH.set(path);
}

/// 解析 dev 凭据文件路径：env 优先（供集成/手动测试注入），否则用启动时设的 app_data_dir 路径。
#[cfg(debug_assertions)]
fn dev_path() -> AppResult<std::path::PathBuf> {
    if let Ok(p) = std::env::var("AI_EMAIL_DEV_CRED_FILE") {
        return Ok(std::path::PathBuf::from(p));
    }
    DEV_CRED_PATH
        .get()
        .cloned()
        .ok_or_else(|| AppError::Keychain("dev cred path 未初始化".into()))
}

/// auth code 的 keychain service（按 UUID 键）。
#[cfg(not(debug_assertions))]
const MAIL_SERVICE: &str = "com.zhuxbo.aiemail";

/// AI provider key 的 keychain service。与 MAIL_SERVICE 分开避免 UUID 串味、且用户在
/// 钥匙串访问里看到两个独立条目。
#[cfg(not(debug_assertions))]
const AI_SERVICE: &str = "com.zhuxbo.aiemail.ai";

#[cfg(not(debug_assertions))]
fn account_entry(account_id: Uuid) -> AppResult<keyring::Entry> {
    keyring::Entry::new(MAIL_SERVICE, &account_id.to_string()).map_err(map_err)
}

#[cfg(not(debug_assertions))]
fn ai_entry(model_id: Uuid) -> AppResult<keyring::Entry> {
    keyring::Entry::new(AI_SERVICE, &model_id.to_string()).map_err(map_err)
}

// ── Mail accounts ─────────────────────────────────────────────────────────────

/// 存账户授权码（按 UUID）。覆盖旧值；唯一调用方 account_add 每次生成新 UUID。
pub fn store_auth_code(account_id: Uuid, code: &SecretString) -> AppResult<()> {
    #[cfg(debug_assertions)]
    {
        dev_store::write(
            &dev_path()?,
            dev_store::Namespace::Mail,
            &account_id.to_string(),
            code.expose_secret(),
        )
    }
    #[cfg(not(debug_assertions))]
    {
        account_entry(account_id)?
            .set_password(code.expose_secret())
            .map_err(map_err)
    }
}

/// 取账户授权码。条目缺失返回 [`AppError::Keychain`]。
pub fn get_auth_code(account_id: Uuid) -> AppResult<SecretString> {
    #[cfg(debug_assertions)]
    {
        match dev_store::read(
            &dev_path()?,
            dev_store::Namespace::Mail,
            &account_id.to_string(),
        )? {
            Some(s) => Ok(SecretString::from(s)),
            None => Err(AppError::Keychain("dev cred not found".into())),
        }
    }
    #[cfg(not(debug_assertions))]
    {
        let password = account_entry(account_id)?.get_password().map_err(map_err)?;
        Ok(SecretString::from(password))
    }
}

/// 删账户授权码。幂等：缺失 → Ok(())。
pub fn delete_auth_code(account_id: Uuid) -> AppResult<()> {
    #[cfg(debug_assertions)]
    {
        dev_store::delete(
            &dev_path()?,
            dev_store::Namespace::Mail,
            &account_id.to_string(),
        )
    }
    #[cfg(not(debug_assertions))]
    {
        match account_entry(account_id)?.delete_credential() {
            Ok(()) => Ok(()),
            Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(map_err(e)),
        }
    }
}

// ── AI provider keys ──────────────────────────────────────────────────────────

/// 存 AI provider API key（按 model UUID）。每个 ai_models 行独立条目。
pub fn store_ai_key(model_id: Uuid, key: &SecretString) -> AppResult<()> {
    #[cfg(debug_assertions)]
    {
        dev_store::write(
            &dev_path()?,
            dev_store::Namespace::Ai,
            &model_id.to_string(),
            key.expose_secret(),
        )
    }
    #[cfg(not(debug_assertions))]
    {
        ai_entry(model_id)?
            .set_password(key.expose_secret())
            .map_err(map_err)
    }
}

pub fn get_ai_key(model_id: Uuid) -> AppResult<SecretString> {
    #[cfg(debug_assertions)]
    {
        match dev_store::read(
            &dev_path()?,
            dev_store::Namespace::Ai,
            &model_id.to_string(),
        )? {
            Some(s) => Ok(SecretString::from(s)),
            None => Err(AppError::Keychain("dev cred not found".into())),
        }
    }
    #[cfg(not(debug_assertions))]
    {
        let password = ai_entry(model_id)?.get_password().map_err(map_err)?;
        Ok(SecretString::from(password))
    }
}

/// 删 AI provider key。幂等。
pub fn delete_ai_key(model_id: Uuid) -> AppResult<()> {
    #[cfg(debug_assertions)]
    {
        dev_store::delete(
            &dev_path()?,
            dev_store::Namespace::Ai,
            &model_id.to_string(),
        )
    }
    #[cfg(not(debug_assertions))]
    {
        match ai_entry(model_id)?.delete_credential() {
            Ok(()) => Ok(()),
            Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(map_err(e)),
        }
    }
}

#[cfg(not(debug_assertions))]
fn map_err(e: keyring::Error) -> AppError {
    AppError::Keychain(e.to_string())
}

#[cfg(all(test, debug_assertions))]
mod tests {
    use super::*;
    use std::sync::{Mutex, PoisonError};
    use tempfile::tempdir;

    // 串行化设进程级 env 的测试，避免 race。
    static ENV_MUTEX: Mutex<()> = Mutex::new(());

    #[test]
    fn auth_roundtrip_and_not_in_ai_namespace() {
        let _g = ENV_MUTEX.lock().unwrap_or_else(PoisonError::into_inner);
        let dir = tempdir().unwrap();
        std::env::set_var("AI_EMAIL_DEV_CRED_FILE", dir.path().join("c.json"));
        let id = Uuid::new_v4();
        store_auth_code(id, &SecretString::from("authcode".to_string())).unwrap();
        assert_eq!(get_auth_code(id).unwrap().expose_secret(), "authcode");
        // 变异验证：把 auth 的 Mail 错写 Ai，则同 UUID 在 ai 命名空间能取到值、is_err FAIL。
        assert!(get_ai_key(id).is_err());
        std::env::remove_var("AI_EMAIL_DEV_CRED_FILE");
    }

    #[test]
    fn ai_key_roundtrip() {
        let _g = ENV_MUTEX.lock().unwrap_or_else(PoisonError::into_inner);
        let dir = tempdir().unwrap();
        std::env::set_var("AI_EMAIL_DEV_CRED_FILE", dir.path().join("c.json"));
        let id = Uuid::new_v4();
        store_ai_key(id, &SecretString::from("apikey".to_string())).unwrap();
        assert_eq!(get_ai_key(id).unwrap().expose_secret(), "apikey");
        std::env::remove_var("AI_EMAIL_DEV_CRED_FILE");
    }
}

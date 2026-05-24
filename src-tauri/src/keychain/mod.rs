//! OS-keychain wrapper. The only place the rest of the crate touches the [`keyring`] crate.
//!
//! Layout:
//!   • service = `com.zhuxbo.aiemail` (matches the Tauri bundle identifier — keeps dev / release
//!     installs separate from any other Anthropic / Tauri app on the same machine)
//!   • username = `account.id` (UUID stringified)
//!
//! Functions are synchronous — the keyring crate's platform backends are blocking. Callers in
//! async contexts must wrap calls with [`tokio::task::spawn_blocking`].

use secrecy::{ExposeSecret, SecretString};
use uuid::Uuid;

use crate::error::{AppError, AppResult};

/// Service for mail-account auth codes. Entries are keyed by `accounts.id` (UUID).
const MAIL_SERVICE: &str = "com.zhuxbo.aiemail";

/// Service for AI provider API keys. Separated from MAIL_SERVICE so the two namespaces can't
/// alias UUIDs and so the user sees two distinct app entries in Keychain Access.
const AI_SERVICE: &str = "com.zhuxbo.aiemail.ai";

fn account_entry(account_id: Uuid) -> AppResult<keyring::Entry> {
    keyring::Entry::new(MAIL_SERVICE, &account_id.to_string()).map_err(map_err)
}

fn ai_entry(model_id: Uuid) -> AppResult<keyring::Entry> {
    keyring::Entry::new(AI_SERVICE, &model_id.to_string()).map_err(map_err)
}

// ── Mail accounts ─────────────────────────────────────────────────────────────

/// Store an account's auth code under its UUID. Overwrites any prior value silently — the only
/// caller is `account_add`, which generates a fresh UUID per insert.
pub fn store_auth_code(account_id: Uuid, code: &SecretString) -> AppResult<()> {
    account_entry(account_id)?
        .set_password(code.expose_secret())
        .map_err(map_err)
}

/// Fetch an account's auth code. Returns [`AppError::Keychain`] with the underlying message if
/// the entry is missing.
pub fn get_auth_code(account_id: Uuid) -> AppResult<SecretString> {
    let password = account_entry(account_id)?.get_password().map_err(map_err)?;
    Ok(SecretString::from(password))
}

/// Remove an account's auth code. Idempotent: missing entries return Ok(()).
pub fn delete_auth_code(account_id: Uuid) -> AppResult<()> {
    match account_entry(account_id)?.delete_credential() {
        Ok(()) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(map_err(e)),
    }
}

// ── AI provider keys ──────────────────────────────────────────────────────────

/// Store an AI provider API key under the AI model's UUID. Each `ai_models` row gets its own
/// keychain entry so users can rotate one provider's key without touching others.
pub fn store_ai_key(model_id: Uuid, key: &SecretString) -> AppResult<()> {
    ai_entry(model_id)?
        .set_password(key.expose_secret())
        .map_err(map_err)
}

pub fn get_ai_key(model_id: Uuid) -> AppResult<SecretString> {
    let password = ai_entry(model_id)?.get_password().map_err(map_err)?;
    Ok(SecretString::from(password))
}

/// Remove an AI provider key. Idempotent — used during `model_remove` cleanup.
pub fn delete_ai_key(model_id: Uuid) -> AppResult<()> {
    match ai_entry(model_id)?.delete_credential() {
        Ok(()) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(map_err(e)),
    }
}

fn map_err(e: keyring::Error) -> AppError {
    AppError::Keychain(e.to_string())
}

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

/// Keychain service name. Shared across every account so all entries cluster under one app
/// in Keychain Access / Credential Manager.
const SERVICE: &str = "com.zhuxbo.aiemail";

fn entry(account_id: Uuid) -> AppResult<keyring::Entry> {
    keyring::Entry::new(SERVICE, &account_id.to_string()).map_err(map_err)
}

/// Store an account's auth code under its UUID. Overwrites any prior value silently — the only
/// caller is `account_add`, which generates a fresh UUID per insert.
pub fn store_auth_code(account_id: Uuid, code: &SecretString) -> AppResult<()> {
    entry(account_id)?
        .set_password(code.expose_secret())
        .map_err(map_err)
}

/// Fetch an account's auth code. Returns [`AppError::Keychain`] with the underlying message if
/// the entry is missing — callers may match on that string for now; we can promote to a typed
/// variant once we have more callers.
pub fn get_auth_code(account_id: Uuid) -> AppResult<SecretString> {
    let password = entry(account_id)?.get_password().map_err(map_err)?;
    Ok(SecretString::from(password))
}

/// Remove an account's auth code. Idempotent: missing entries return Ok(()).
pub fn delete_auth_code(account_id: Uuid) -> AppResult<()> {
    match entry(account_id)?.delete_credential() {
        Ok(()) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(map_err(e)),
    }
}

fn map_err(e: keyring::Error) -> AppError {
    AppError::Keychain(e.to_string())
}

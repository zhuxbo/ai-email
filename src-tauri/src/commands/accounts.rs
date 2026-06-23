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

use secrecy::SecretString;
use serde::Deserialize;
use tauri::State;
use uuid::Uuid;

use crate::db::accounts::{self, Account, AccountInput};
use crate::error::{AppError, AppResult};
use crate::keychain;
use crate::AppState;

/// Known email provider identifiers.
const KNOWN_PROVIDERS: &[&str] = &["qq", "imap"];

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

/// Validate port number is in the legal TCP range 1–65535.
fn validate_port(port: i32, field: &str) -> AppResult<()> {
    if !(1..=65535).contains(&port) {
        return Err(AppError::Config(format!(
            "{field} must be between 1 and 65535, got {port}"
        )));
    }
    Ok(())
}

#[tauri::command]
pub async fn accounts_list(state: State<'_, AppState>) -> AppResult<Vec<Account>> {
    accounts::list(&state.db).await
}

#[tauri::command]
pub async fn account_add(state: State<'_, AppState>, form: AddAccountForm) -> AppResult<Account> {
    // #39: validate port range and provider up-front so errors surface immediately.
    validate_port(form.imap_port, "imap_port")?;
    validate_port(form.smtp_port, "smtp_port")?;
    if !KNOWN_PROVIDERS.contains(&form.provider.as_str()) {
        return Err(AppError::Config(format!(
            "unknown provider: {} (must be one of: {})",
            form.provider,
            KNOWN_PROVIDERS.join(", ")
        )));
    }

    let auth = SecretString::from(form.auth_code);
    let input = AccountInput {
        email: form.email,
        display_name: form.display_name,
        provider: form.provider,
        imap_host: form.imap_host,
        imap_port: form.imap_port,
        smtp_host: form.smtp_host,
        smtp_port: form.smtp_port,
    };

    let account = accounts::insert(&state.db, &input).await?;
    let id = account.id;

    let stored = tokio::task::spawn_blocking(move || keychain::store_auth_code(id, &auth))
        .await
        .map_err(|e| AppError::Other(anyhow::anyhow!(e)))?;

    if let Err(e) = stored {
        // Roll the DB back so the user isn't stuck with a row whose secret is missing.
        if let Err(cleanup) = accounts::delete(&state.db, id).await {
            tracing::error!(error = ?cleanup, "failed to roll back account row after keychain failure");
        }
        return Err(e);
    }

    tracing::info!(account_id = %account.id, email = %account.email, "account added");
    Ok(account)
}

#[tauri::command]
pub async fn account_remove(state: State<'_, AppState>, id: Uuid) -> AppResult<()> {
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

    accounts::delete(&state.db, id).await?;
    tracing::info!(account_id = %id, "account removed");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::validate_port;
    use crate::error::AppError;

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
}

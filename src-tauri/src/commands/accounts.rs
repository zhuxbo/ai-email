//! `account_*` / `accounts_*` Tauri commands.
//!
//! Add-account flow:
//!   1. INSERT into Postgres → get fresh UUID
//!   2. Wrap the auth code in [`SecretString`] (so it stops appearing in logs)
//!   3. Store it in the OS keychain on a blocking thread
//!   4. On keychain failure, DELETE the row so the user can retry from a clean slate
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

/// What the add-account form sends across the FFI. `authCode` is split out and never
/// round-tripped back to the frontend.
#[derive(Debug, Clone, Deserialize)]
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

#[tauri::command]
pub async fn accounts_list(state: State<'_, AppState>) -> AppResult<Vec<Account>> {
    accounts::list(&state.db).await
}

#[tauri::command]
pub async fn account_add(state: State<'_, AppState>, form: AddAccountForm) -> AppResult<Account> {
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
    accounts::delete(&state.db, id).await?;
    tokio::task::spawn_blocking(move || keychain::delete_auth_code(id))
        .await
        .map_err(|e| AppError::Other(anyhow::anyhow!(e)))??;
    tracing::info!(account_id = %id, "account removed");
    Ok(())
}

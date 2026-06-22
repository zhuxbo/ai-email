//! AI provider configuration commands. The settings UI is the only caller.
//!
//! Surface (camelCase on the TS side):
//!   • `modelsList()`                              → [AiModel]
//!   • `modelAdd(form)`                            → AiModel        (form includes apiKey)
//!   • `modelRemove(id)`                           → ()              (clears keychain)
//!   • `roleDefaultsList()`                        → [RoleDefault]
//!   • `roleDefaultSet({ role, modelId })`         → ()
//!   • `roleDefaultClear(role)`                    → ()
//!
//! Add-model flow mirrors account_add: INSERT row, then write the API key to the keychain
//! on a blocking thread, then roll the row back if the keychain write fails.
//!
//! Remove-model flow: best-effort delete keychain key first (warn on failure, never abort),
//! then delete the DB row — mirrors the delete-warn-not-fail invariant from account_remove.

use secrecy::SecretString;
use serde::Deserialize;
use tauri::State;
use uuid::Uuid;

use crate::db::ai_models::{self, AiModel, AiModelInput};
use crate::db::ai_role_defaults::{self, RoleDefault};
use crate::error::{AppError, AppResult};
use crate::keychain;
use crate::AppState;

/// Add-model form. `apiKey` is split out and goes straight to the keychain.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddModelForm {
    pub display_name: String,
    pub provider: String,
    pub model_id: String,
    pub base_url: Option<String>,
    pub api_key: String,
}

/// Validate that a `base_url`, if provided, uses HTTPS.
///
/// An empty / whitespace-only value is treated as "use the provider default" and returns
/// `Ok(None)`. A non-empty value must start with `https://`; anything else (including
/// `http://`) is rejected to enforce the TLS invariant — API keys must never travel in
/// plain text.
fn validate_base_url(raw: Option<String>) -> AppResult<Option<String>> {
    match raw {
        None => Ok(None),
        Some(s) => {
            let s = s.trim().to_owned();
            if s.is_empty() {
                return Ok(None);
            }
            if !s.starts_with("https://") {
                return Err(AppError::Config(format!(
                    "base_url must start with https:// to protect the API key in transit; got: {s}"
                )));
            }
            Ok(Some(s))
        }
    }
}

#[tauri::command]
pub async fn models_list(state: State<'_, AppState>) -> AppResult<Vec<AiModel>> {
    ai_models::list(&state.db).await
}

#[tauri::command]
pub async fn model_add(state: State<'_, AppState>, form: AddModelForm) -> AppResult<AiModel> {
    if form.provider != "anthropic" && form.provider != "openai" {
        return Err(AppError::Config(format!(
            "unknown provider: {} (must be 'anthropic' or 'openai')",
            form.provider
        )));
    }
    // #3: reject non-HTTPS base_url to prevent API key leakage over plain HTTP.
    let base_url = validate_base_url(form.base_url)?;

    let key = SecretString::from(form.api_key);
    let input = AiModelInput {
        display_name: form.display_name,
        provider: form.provider,
        model_id: form.model_id,
        base_url,
    };
    let model = ai_models::insert(&state.db, &input).await?;
    let id = model.id;

    let stored = tokio::task::spawn_blocking(move || keychain::store_ai_key(id, &key))
        .await
        .map_err(|e| AppError::Other(anyhow::anyhow!(e)))?;
    if let Err(e) = stored {
        if let Err(cleanup) = ai_models::delete(&state.db, id).await {
            tracing::error!(error = ?cleanup, "failed to roll back ai_models row after keychain failure");
        }
        return Err(e);
    }

    tracing::info!(
        model_id = %model.id,
        provider = %model.provider,
        api_model = %model.model_id,
        "ai model added"
    );
    Ok(model)
}

#[tauri::command]
pub async fn model_remove(state: State<'_, AppState>, id: Uuid) -> AppResult<()> {
    // #44: delete keychain key first (best-effort — warn on failure, never abort).
    // Mirrors the delete-warn-not-fail invariant from account_remove (#11).
    // ON DELETE RESTRICT — the DB enforces "must reassign role defaults first".
    let keychain_result = tokio::task::spawn_blocking(move || keychain::delete_ai_key(id))
        .await
        .map_err(|e| AppError::Other(anyhow::anyhow!(e)));
    match keychain_result {
        Ok(Ok(())) => {}
        Ok(Err(e)) => {
            tracing::warn!(model_id = %id, error = %e, "keychain delete_ai_key failed; proceeding with DB remove");
        }
        Err(e) => {
            tracing::warn!(model_id = %id, error = %e, "spawn_blocking for keychain delete failed; proceeding with DB remove");
        }
    }

    ai_models::delete(&state.db, id).await?;
    tracing::info!(model_id = %id, "ai model removed");
    Ok(())
}

#[tauri::command]
pub async fn role_defaults_list(state: State<'_, AppState>) -> AppResult<Vec<RoleDefault>> {
    ai_role_defaults::list(&state.db).await
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetRoleDefaultForm {
    pub role: String,
    pub model_id: Uuid,
}

#[tauri::command]
pub async fn role_default_set(
    state: State<'_, AppState>,
    form: SetRoleDefaultForm,
) -> AppResult<()> {
    if !matches!(
        form.role.as_str(),
        "summary" | "classify" | "translate" | "draft"
    ) {
        return Err(AppError::Config(format!("unknown role: {}", form.role)));
    }
    ai_role_defaults::set(&state.db, &form.role, form.model_id).await
}

#[tauri::command]
pub async fn role_default_clear(state: State<'_, AppState>, role: String) -> AppResult<()> {
    ai_role_defaults::clear(&state.db, &role).await
}

#[cfg(test)]
mod tests {
    use super::validate_base_url;
    use crate::error::AppError;

    // ---- #3: base_url HTTPS enforcement ----

    #[test]
    fn none_returns_none() {
        assert!(validate_base_url(None).unwrap().is_none());
    }

    #[test]
    fn empty_string_returns_none() {
        assert!(validate_base_url(Some(String::new())).unwrap().is_none());
    }

    #[test]
    fn whitespace_only_returns_none() {
        assert!(validate_base_url(Some("   ".into())).unwrap().is_none());
    }

    #[test]
    fn http_url_is_rejected() {
        let err = validate_base_url(Some("http://api.example.com/v1".into())).unwrap_err();
        assert!(matches!(err, AppError::Config(_)));
    }

    #[test]
    fn ftp_url_is_rejected() {
        let err = validate_base_url(Some("ftp://api.example.com".into())).unwrap_err();
        assert!(matches!(err, AppError::Config(_)));
    }

    #[test]
    fn bare_host_is_rejected() {
        let err = validate_base_url(Some("api.example.com".into())).unwrap_err();
        assert!(matches!(err, AppError::Config(_)));
    }

    #[test]
    fn https_url_is_accepted() {
        let url = validate_base_url(Some("https://api.example.com/v1".into()))
            .unwrap()
            .unwrap();
        assert_eq!(url, "https://api.example.com/v1");
    }

    #[test]
    fn https_url_is_trimmed() {
        let url = validate_base_url(Some("  https://api.example.com/v1  ".into()))
            .unwrap()
            .unwrap();
        assert_eq!(url, "https://api.example.com/v1");
    }
}

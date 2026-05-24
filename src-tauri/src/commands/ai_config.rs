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
    let key = SecretString::from(form.api_key);
    let input = AiModelInput {
        display_name: form.display_name,
        provider: form.provider,
        model_id: form.model_id,
        base_url: form.base_url.filter(|s| !s.trim().is_empty()),
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
    // ON DELETE RESTRICT — the DB enforces "must reassign role defaults first". The
    // resulting sqlx error message contains the offending FK so the UI can prompt the
    // user clearly; we don't pre-check here.
    ai_models::delete(&state.db, id).await?;
    tokio::task::spawn_blocking(move || keychain::delete_ai_key(id))
        .await
        .map_err(|e| AppError::Other(anyhow::anyhow!(e)))??;
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

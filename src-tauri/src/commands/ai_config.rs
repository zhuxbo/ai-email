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

use std::future::Future;

use secrecy::SecretString;
use serde::Deserialize;
use tauri::State;
use uuid::Uuid;

use crate::db::ai_models::{self, AiModel, AiModelInput, AiModelUpdate};
use crate::db::ai_role_defaults::{self, RoleDefault};
use crate::db::Pool;
use crate::error::{AppError, AppResult};
use crate::keychain;
use crate::AppState;

/// Add-model form. `apiKey` is split out and goes straight to the keychain.
///
/// `Debug` is implemented manually so that `api_key` never appears in log output.
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddModelForm {
    pub display_name: String,
    pub provider: String,
    pub model_id: String,
    pub base_url: Option<String>,
    pub api_key: String,
}

impl std::fmt::Debug for AddModelForm {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AddModelForm")
            .field("display_name", &self.display_name)
            .field("provider", &self.provider)
            .field("model_id", &self.model_id)
            .field("base_url", &self.base_url)
            .field("api_key", &"[redacted]")
            .finish()
    }
}

/// Update-model form. Like `AddModelForm` but `provider` is fixed (not editable) and
/// `api_key` is optional — `None` / empty means "keep the existing key in the keychain".
///
/// `Debug` is implemented manually so that `api_key` never appears in log output.
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateModelForm {
    pub display_name: String,
    pub model_id: String,
    pub base_url: Option<String>,
    pub api_key: Option<String>,
}

impl std::fmt::Debug for UpdateModelForm {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UpdateModelForm")
            .field("display_name", &self.display_name)
            .field("model_id", &self.model_id)
            .field("base_url", &self.base_url)
            .field("api_key", &self.api_key.as_ref().map(|_| "[redacted]"))
            .finish()
    }
}

/// Validate that a `base_url`, if provided, uses HTTPS and has a non-empty host.
///
/// An empty / whitespace-only value is treated as "use the provider default" and returns
/// `Ok(None)`. A non-empty value must:
///   1. Use the `https` scheme — checked case-insensitively (RFC 3986 §3.1: scheme is
///      case-insensitive, so `HTTPS://host` is legal and must not be rejected).
///   2. Have a non-empty host after the `https://` prefix — bare `https://` (scheme only,
///      no host) is rejected to avoid `https:///path` requests at runtime.
///
/// The original casing is preserved in storage/use; only the scheme check uses a lowercase
/// copy.
fn validate_base_url(raw: Option<String>) -> AppResult<Option<String>> {
    match raw {
        None => Ok(None),
        Some(s) => {
            let s = s.trim().to_owned();
            if s.is_empty() {
                return Ok(None);
            }
            let lower = s.to_ascii_lowercase();
            if !lower.starts_with("https://") {
                return Err(AppError::Config(format!(
                    "base_url must start with https:// to protect the API key in transit; got: {s}"
                )));
            }
            // Reject scheme-only URLs like "https://" that have no host.
            let after_scheme = &s["https://".len()..];
            if after_scheme.trim().is_empty() {
                return Err(AppError::Config(format!(
                    "base_url must include a host after https://; got: {s}"
                )));
            }
            Ok(Some(s))
        }
    }
}

#[tauri::command]
pub async fn models_list(state: State<'_, AppState>) -> AppResult<Vec<AiModel>> {
    ai_models::list(state.pool().await?).await
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
    let pool = state.pool().await?;
    let model = ai_models::insert(pool, &input).await?;
    let id = model.id;

    let stored = tokio::task::spawn_blocking(move || keychain::store_ai_key(id, &key))
        .await
        .map_err(|e| AppError::Other(anyhow::anyhow!(e)))?;
    if let Err(e) = stored {
        if let Err(cleanup) = ai_models::delete(pool, id).await {
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
    // 不需要像 account_remove 那样按模型粒度取消在途后台任务，原因如下：
    //   1. 后台任务（classify/evaluate）以账户为粒度 spawn，注册在 AppState::account_tokens 中，
    //      没有按模型分组的令牌注册表。
    //   2. 任务内部通过 `ai_role_defaults JOIN ai_models` 实时查询当前配置的 model_id，
    //      不会在任务启动时持久持有 model_id，所以模型切换/删除不影响已在运行的任务。
    //   3. `ai_role_defaults.model_id` 设有 `ON DELETE RESTRICT` 外键约束：只要该模型
    //      还被某个角色引用，DB 就会拒绝删除，彻底避免孤儿任务的产生。
    //   4. classify/evaluate 的结果表（emails、ai_summaries 等）不含 model_id 外键，
    //      删模型不会造成外键冲突，与删账户（cascade 多张表）的情形不同。
    //
    // #44: delete keychain key first (best-effort — warn on failure, never abort).
    // Mirrors the delete-warn-not-fail invariant from account_remove (#11).
    // ON DELETE RESTRICT — the DB enforces "must reassign role defaults first".
    //
    // Reverse orphan trade-off: if the keychain delete succeeds but the DB `delete` call
    // below fails and propagates via `?`, the DB row survives while the key is already
    // gone. This is the acceptable direction — DB deletes are rare failures and retryable,
    // and `keychain::delete_ai_key` is idempotent (NoEntry → Ok) so a retry self-heals.
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

    ai_models::delete(state.pool().await?, id).await?;
    tracing::info!(model_id = %id, "ai model removed");
    Ok(())
}

/// Update an existing model's editable fields. `provider` is fixed. The API key is only
/// touched when `form.api_key` is a non-empty value, so a blank field preserves the
/// existing keychain entry — mirrors how the UI prefills everything except the key.
#[tauri::command]
pub async fn model_update(
    state: State<'_, AppState>,
    id: Uuid,
    form: UpdateModelForm,
) -> AppResult<AiModel> {
    // #3: reject non-HTTPS base_url to prevent API key leakage over plain HTTP.
    let base_url = validate_base_url(form.base_url)?;
    let input = AiModelUpdate {
        display_name: form.display_name,
        model_id: form.model_id,
        base_url,
    };
    let pool = state.pool().await?;
    // API key 仅在提供了非空值时覆盖 keychain；留空 = 保持原 key 不变。keychain 写失败时
    // apply_model_update 会把 DB 回滚到旧值（见其文档），避免「新配置 + 旧 key」的部分成功。
    let secret = super::secret_to_store(form.api_key);
    let model = apply_model_update(pool, id, &input, secret, |key| async move {
        let key = SecretString::from(key);
        tokio::task::spawn_blocking(move || keychain::store_ai_key(id, &key))
            .await
            .map_err(|e| AppError::Other(anyhow::anyhow!(e)))?
    })
    .await?;

    tracing::info!(model_id = %model.id, "ai model updated");
    Ok(model)
}

/// 提交一次模型更新：写入新的非密字段，并在 `secret` 为非空值时把新 API key 写入 keychain。
///
/// 不变量：keychain 写入失败时，DB 行回滚到更新前的旧值，使模型停留在「旧配置 + 旧 key」
/// 的一致状态、可安全重试 —— 而不是「新配置 + 旧 key」的部分成功。`store` 是唯一的副作用
/// 注入点，测试以一个必失败的实现覆盖回滚路径，无需真实 keychain。
async fn apply_model_update<F, Fut>(
    pool: &Pool,
    id: Uuid,
    input: &AiModelUpdate,
    secret: Option<String>,
    store: F,
) -> AppResult<AiModel>
where
    F: FnOnce(String) -> Fut,
    Fut: Future<Output = AppResult<()>>,
{
    // 仅当确实要改 API key 时，才先抓旧值用于回滚；无 secret 时不触碰 keychain，无需回滚。
    let prev = if secret.is_some() {
        Some(
            ai_models::get(pool, id)
                .await?
                .ok_or_else(|| AppError::Config(format!("model not found: {id}")))?,
        )
    } else {
        None
    };

    let updated = ai_models::update(pool, id, input).await?;

    if let Some(key) = secret {
        if let Err(e) = store(key).await {
            // keychain 写失败：把 DB 回滚到旧值，保持「旧配置 + 旧 key」一致、可安全重试。
            if let Some(prev) = prev {
                let rollback = AiModelUpdate {
                    display_name: prev.display_name,
                    model_id: prev.model_id,
                    base_url: prev.base_url,
                };
                if let Err(re) = ai_models::update(pool, id, &rollback).await {
                    tracing::error!(model_id = %id, error = ?re, "keychain 写失败后回滚模型行也失败");
                }
            }
            return Err(e);
        }
    }

    Ok(updated)
}

#[tauri::command]
pub async fn role_defaults_list(state: State<'_, AppState>) -> AppResult<Vec<RoleDefault>> {
    ai_role_defaults::list(state.pool().await?).await
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
    ai_role_defaults::set(state.pool().await?, &form.role, form.model_id).await
}

#[tauri::command]
pub async fn role_default_clear(state: State<'_, AppState>, role: String) -> AppResult<()> {
    ai_role_defaults::clear(state.pool().await?, &role).await
}

#[cfg(test)]
mod tests {
    use super::{ai_models, apply_model_update, validate_base_url};
    use super::{AiModelInput, AiModelUpdate};
    use crate::db::{self, Pool};
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

    // ---- case-insensitive scheme (RFC 3986) ----

    #[test]
    fn uppercase_https_scheme_is_accepted() {
        // RFC 3986 §3.1: scheme is case-insensitive; HTTPS:// must not be rejected.
        // Original casing must be preserved in the returned value.
        let url = validate_base_url(Some("HTTPS://api.example.com".into()))
            .unwrap()
            .unwrap();
        assert_eq!(url, "HTTPS://api.example.com");
    }

    #[test]
    fn mixed_case_https_scheme_is_accepted() {
        let url = validate_base_url(Some("Https://api.example.com".into()))
            .unwrap()
            .unwrap();
        assert_eq!(url, "Https://api.example.com");
    }

    // ---- host-less URL rejected ----

    #[test]
    fn scheme_only_https_is_rejected() {
        // "https://" with no host must be rejected to prevent https:///path at runtime.
        let err = validate_base_url(Some("https://".into())).unwrap_err();
        assert!(matches!(err, AppError::Config(_)));
    }

    #[test]
    fn single_char_host_is_accepted() {
        // Minimal valid: scheme + at least one character of host.
        let url = validate_base_url(Some("https://x".into()))
            .unwrap()
            .unwrap();
        assert_eq!(url, "https://x");
    }

    // ---- credentials must not appear in Debug output (#2) ----

    #[test]
    fn add_model_form_debug_redacts_api_key() {
        let form = super::AddModelForm {
            display_name: "Claude Sonnet".into(),
            provider: "anthropic".into(),
            model_id: "claude-sonnet-4-6".into(),
            base_url: Some("https://api.anthropic.com".into()),
            api_key: "sk-ant-super_secret_key_12345".into(),
        };
        let debug = format!("{:?}", form);
        assert!(
            !debug.contains("sk-ant-super_secret_key_12345"),
            "api_key must not appear in Debug"
        );
        assert!(
            debug.contains("[redacted]"),
            "Debug must show [redacted] for api_key"
        );
        assert!(
            debug.contains("Claude Sonnet"),
            "non-secret fields must still appear in Debug"
        );
        assert!(
            debug.contains("anthropic"),
            "non-secret fields must still appear in Debug"
        );
    }

    #[test]
    fn update_model_form_debug_redacts_api_key() {
        let form = super::UpdateModelForm {
            display_name: "Claude".into(),
            model_id: "claude-sonnet-4-6".into(),
            base_url: None,
            api_key: Some("sk-ant-secret_999".into()),
        };
        let debug = format!("{:?}", form);
        assert!(
            !debug.contains("sk-ant-secret_999"),
            "api_key must not appear in Debug"
        );
        assert!(
            debug.contains("[redacted]"),
            "Debug must show [redacted] for a present api_key"
        );
        assert!(
            debug.contains("Claude"),
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

    fn sample_model_input() -> AiModelInput {
        AiModelInput {
            display_name: "Old Model".into(),
            provider: "anthropic".into(),
            model_id: "claude-old".into(),
            base_url: Some("https://old.example.com".into()),
        }
    }

    /// 与 [`sample_model_input`] 完全不同的新非密字段，便于断言更新是否残留。
    fn changed_update() -> AiModelUpdate {
        AiModelUpdate {
            display_name: "New Model".into(),
            model_id: "claude-new".into(),
            base_url: Some("https://new.example.com".into()),
        }
    }

    /// keychain 写失败时，DB 必须回滚到更新前的旧值，不残留新的非密字段。
    #[tokio::test]
    async fn model_update_rolls_back_db_when_keychain_write_fails() {
        let pool = test_pool().await;
        let original = ai_models::insert(&pool, &sample_model_input())
            .await
            .unwrap();

        let result = apply_model_update(
            &pool,
            original.id,
            &changed_update(),
            Some("sk-new-key".into()),
            |_key| async { Err(AppError::Keychain("simulated keychain failure".into())) },
        )
        .await;

        assert!(result.is_err(), "keychain 写失败时整个更新应返回错误");

        // 关键不变量：所有非密字段回到旧值，没有「新配置 + 旧 key」的部分成功。
        let reloaded = ai_models::get(&pool, original.id).await.unwrap().unwrap();
        assert_eq!(
            reloaded.display_name, "Old Model",
            "display_name 应回滚到旧值"
        );
        assert_eq!(reloaded.model_id, "claude-old", "model_id 应回滚到旧值");
        assert_eq!(
            reloaded.base_url.as_deref(),
            Some("https://old.example.com"),
            "base_url 应回滚到旧值"
        );
    }

    /// keychain 写成功时，新的非密字段落库，返回更新后的模型。
    #[tokio::test]
    async fn model_update_persists_when_keychain_write_succeeds() {
        let pool = test_pool().await;
        let original = ai_models::insert(&pool, &sample_model_input())
            .await
            .unwrap();

        let updated = apply_model_update(
            &pool,
            original.id,
            &changed_update(),
            Some("sk-new-key".into()),
            |_key| async { Ok(()) },
        )
        .await
        .expect("keychain 成功时更新应成功");

        assert_eq!(updated.model_id, "claude-new");
        let reloaded = ai_models::get(&pool, original.id).await.unwrap().unwrap();
        assert_eq!(reloaded.model_id, "claude-new", "新值应落库");
        assert_eq!(reloaded.display_name, "New Model", "新值应落库");
    }

    /// secret = None（未改 API key）时根本不触碰 keychain：store 闭包不应被调用。
    #[tokio::test]
    async fn model_update_without_secret_skips_keychain() {
        let pool = test_pool().await;
        let original = ai_models::insert(&pool, &sample_model_input())
            .await
            .unwrap();

        let updated =
            apply_model_update(&pool, original.id, &changed_update(), None, |_key| async {
                panic!("无新 API key 时不应触碰 keychain")
            })
            .await
            .expect("无 secret 的更新应成功");

        assert_eq!(updated.model_id, "claude-new");
        let reloaded = ai_models::get(&pool, original.id).await.unwrap().unwrap();
        assert_eq!(reloaded.model_id, "claude-new", "新值应落库");
    }
}

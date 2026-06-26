//! filter_rules CRUD 命令。设置中心「AI 过滤规则」页是唯一调用方。薄封装,业务逻辑在 db::filter_rules。
//! pattern 经 Regex::new 校验:非法正则即拒(线性引擎,无 ReDoS)。

use tauri::State;
use uuid::Uuid;

use crate::db::filter_rules::{self, FilterRule, FilterRuleInput};
use crate::error::{AppError, AppResult};
use crate::AppState;

const SCOPES: [&str; 3] = ["global", "domain", "email"];
const TARGETS: [&str; 3] = ["signature", "quote", "repeat"];
const ACTIONS: [&str; 2] = ["keep", "strip"];

/// 校验规则字段。scope/target/action 闭集;scope_value 与 scope 一致;pattern 非空时必须可编译。
fn validate(
    scope: &str,
    scope_value: &str,
    target: &str,
    action: &str,
    pattern: &Option<String>,
) -> AppResult<()> {
    if !SCOPES.contains(&scope) {
        return Err(AppError::Config(format!("unknown scope: {scope}")));
    }
    if !TARGETS.contains(&target) {
        return Err(AppError::Config(format!("unknown target: {target}")));
    }
    if !ACTIONS.contains(&action) {
        return Err(AppError::Config(format!("unknown action: {action}")));
    }
    match scope {
        "global" => {
            if !scope_value.trim().is_empty() {
                return Err(AppError::Config(
                    "global 规则的 scope_value 必须为空".into(),
                ));
            }
        }
        _ => {
            if scope_value.trim().is_empty() {
                return Err(AppError::Config(format!(
                    "{scope} 规则必须指定 scope_value"
                )));
            }
        }
    }
    if let Some(p) = pattern {
        if !p.is_empty() {
            regex::Regex::new(p).map_err(|e| AppError::Config(format!("非法正则 pattern: {e}")))?;
        }
    }
    Ok(())
}

#[tauri::command]
pub async fn filter_rules_list(state: State<'_, AppState>) -> AppResult<Vec<FilterRule>> {
    filter_rules::list_all(state.pool().await?).await
}

#[tauri::command]
pub async fn filter_rule_add(
    state: State<'_, AppState>,
    input: FilterRuleInput,
) -> AppResult<FilterRule> {
    validate(
        &input.scope,
        &input.scope_value,
        &input.target,
        &input.action,
        &input.pattern,
    )?;
    filter_rules::insert(state.pool().await?, &input).await
}

#[tauri::command]
pub async fn filter_rule_update(state: State<'_, AppState>, rule: FilterRule) -> AppResult<()> {
    validate(
        &rule.scope,
        &rule.scope_value,
        &rule.target,
        &rule.action,
        &rule.pattern,
    )?;
    filter_rules::update(state.pool().await?, &rule).await
}

#[tauri::command]
pub async fn filter_rule_remove(state: State<'_, AppState>, id: Uuid) -> AppResult<()> {
    filter_rules::delete(state.pool().await?, id).await
}

#[tauri::command]
pub async fn filter_rule_set_enabled(
    state: State<'_, AppState>,
    id: Uuid,
    enabled: bool,
) -> AppResult<()> {
    filter_rules::set_enabled(state.pool().await?, id, enabled).await
}

#[cfg(test)]
mod tests {
    use super::validate;

    #[test]
    fn accepts_valid_rules() {
        assert!(validate("global", "", "signature", "strip", &None).is_ok());
        assert!(validate(
            "domain",
            "cnssl.cn",
            "quote",
            "keep",
            &Some("Disclaimer".into())
        )
        .is_ok());
        assert!(validate("email", "a@x.com", "repeat", "strip", &None).is_ok());
        // 空串 pattern 视同无条件。
        assert!(validate("global", "", "signature", "strip", &Some(String::new())).is_ok());
    }

    #[test]
    fn rejects_bad_enum() {
        assert!(validate("nope", "", "signature", "strip", &None).is_err());
        assert!(validate("global", "", "nope", "strip", &None).is_err());
        assert!(validate("global", "", "signature", "nope", &None).is_err());
    }

    #[test]
    fn rejects_scope_value_mismatch() {
        // global 必须空。
        assert!(validate("global", "x.com", "signature", "strip", &None).is_err());
        // domain/email 必须非空。
        assert!(validate("domain", "", "signature", "strip", &None).is_err());
        assert!(validate("email", "  ", "signature", "strip", &None).is_err());
    }

    #[test]
    fn rejects_invalid_regex() {
        // 未闭合分组 → Regex::new 失败 → 拒绝。
        assert!(validate("domain", "x.com", "signature", "strip", &Some("(".into())).is_err());
    }
}

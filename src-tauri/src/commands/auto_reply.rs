//! 自动回复规则 CRUD + 建议回复队列命令。设置 UI / 自动回复中心是唯一调用方。
//! 本文件不发送邮件——发送恒由前端 compose tab 经 smtp_send 人工触发。

use tauri::State;
use uuid::Uuid;

use crate::db::auto_reply_rules::{self, AutoReplyRule, AutoReplyRuleInput};
use crate::db::suggested_replies::{self, SuggestedReply};
use crate::error::{AppError, AppResult};
use crate::AppState;

const CATEGORIES: [&str; 5] = ["personal", "work", "notification", "promotion", "spam"];

/// 校验：name/draft_intent 不得为空白；category 必须在闭集；priority_ceiling 必须 1..=3。
fn validate(
    name: &str,
    draft_intent: &str,
    category: &Option<String>,
    ceiling: Option<i64>,
) -> AppResult<()> {
    if name.trim().is_empty() {
        return Err(AppError::Config("rule name must not be empty".into()));
    }
    if draft_intent.trim().is_empty() {
        return Err(AppError::Config("draft intent must not be empty".into()));
    }
    if let Some(c) = category {
        if !CATEGORIES.contains(&c.as_str()) {
            return Err(AppError::Config(format!("unknown category: {c}")));
        }
    }
    if let Some(p) = ceiling {
        if !(1..=3).contains(&p) {
            return Err(AppError::Config(format!(
                "priority ceiling must be 1..=3, got {p}"
            )));
        }
    }
    Ok(())
}

#[tauri::command]
pub async fn auto_reply_rules_list(
    state: State<'_, AppState>,
    account_id: Uuid,
) -> AppResult<Vec<AutoReplyRule>> {
    auto_reply_rules::list_by_account(state.pool().await?, account_id).await
}

#[tauri::command]
pub async fn auto_reply_rule_add(
    state: State<'_, AppState>,
    input: AutoReplyRuleInput,
) -> AppResult<AutoReplyRule> {
    validate(
        &input.name,
        &input.draft_intent,
        &input.match_category,
        input.match_priority_ceiling,
    )?;
    auto_reply_rules::insert(state.pool().await?, &input).await
}

#[tauri::command]
pub async fn auto_reply_rule_update(
    state: State<'_, AppState>,
    rule: AutoReplyRule,
) -> AppResult<()> {
    validate(
        &rule.name,
        &rule.draft_intent,
        &rule.match_category,
        rule.match_priority_ceiling,
    )?;
    auto_reply_rules::update(state.pool().await?, &rule).await
}

#[tauri::command]
pub async fn auto_reply_rule_remove(state: State<'_, AppState>, id: Uuid) -> AppResult<()> {
    auto_reply_rules::delete(state.pool().await?, id).await
}

#[tauri::command]
pub async fn auto_reply_rule_set_enabled(
    state: State<'_, AppState>,
    id: Uuid,
    enabled: bool,
) -> AppResult<()> {
    auto_reply_rules::set_enabled(state.pool().await?, id, enabled).await
}

#[tauri::command]
pub async fn suggested_replies_list(state: State<'_, AppState>) -> AppResult<Vec<SuggestedReply>> {
    suggested_replies::list_pending(state.pool().await?).await
}

#[tauri::command]
pub async fn suggested_reply_dismiss(state: State<'_, AppState>, id: Uuid) -> AppResult<()> {
    suggested_replies::dismiss(state.pool().await?, id).await
}

#[cfg(test)]
mod tests {
    use super::validate;

    #[test]
    fn validate_accepts_valid() {
        assert!(validate("工作", "确认", &None, None).is_ok());
        assert!(validate("r", "i", &Some("work".into()), Some(1)).is_ok());
        assert!(validate("r", "i", &Some("spam".into()), Some(3)).is_ok());
    }
    #[test]
    fn validate_rejects_bad_category_and_ceiling() {
        assert!(validate("r", "i", &Some("nope".into()), None).is_err());
        assert!(validate("r", "i", &None, Some(0)).is_err());
        assert!(validate("r", "i", &None, Some(4)).is_err());
    }
    #[test]
    fn validate_rejects_empty_name_or_intent() {
        assert!(validate("", "i", &None, None).is_err());
        assert!(validate("   ", "i", &None, None).is_err());
        assert!(validate("r", "", &None, None).is_err());
        assert!(validate("r", "  ", &None, None).is_err());
    }
}

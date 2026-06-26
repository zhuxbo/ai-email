//! Repository for `filter_rules` — Plan B AI 过滤规则。CRUD 仿 `auto_reply_rules`，
//! 但 `resolve_for` 是全局 scope 三级优先级解析（email > domain > global）。

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::addr;
use crate::ai::extract::Action;
use crate::db::Pool;
use crate::error::AppResult;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
#[serde(rename_all = "camelCase")]
pub struct FilterRule {
    pub id: Uuid,
    pub scope: String,
    pub scope_value: String,
    pub target: String,
    pub action: String,
    pub pattern: Option<String>,
    pub enabled: bool,
    pub note: Option<String>,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FilterRuleInput {
    pub scope: String,
    pub scope_value: String,
    pub target: String,
    pub action: String,
    pub pattern: Option<String>,
    pub enabled: bool,
    pub note: Option<String>,
}

/// 解析后的每-target 动作 + 签名 pattern。None=无规则命中（用能力默认）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ResolvedRules {
    pub signature: Option<Action>,
    pub quote: Option<Action>,
    pub repeat: Option<Action>,
    pub signature_pattern: Option<String>,
}

const COLS: &str = "id, scope, scope_value, target, action, pattern, enabled, note, created_at";

pub async fn insert(pool: &Pool, input: &FilterRuleInput) -> AppResult<FilterRule> {
    let row = sqlx::query_as::<_, FilterRule>(&format!(
        "INSERT INTO filter_rules \
         (id, scope, scope_value, target, action, pattern, enabled, note) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8) \
         RETURNING {COLS}"
    ))
    .bind(Uuid::new_v4())
    .bind(&input.scope)
    .bind(&input.scope_value)
    .bind(&input.target)
    .bind(&input.action)
    .bind(&input.pattern)
    .bind(input.enabled)
    .bind(&input.note)
    .fetch_one(pool)
    .await?;
    Ok(row)
}

/// UI 用：全部规则（含禁用），按 scope/scope_value/target 稳定排序。
pub async fn list_all(pool: &Pool) -> AppResult<Vec<FilterRule>> {
    let rows = sqlx::query_as::<_, FilterRule>(&format!(
        "SELECT {COLS} FROM filter_rules ORDER BY scope, scope_value, target"
    ))
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn update(pool: &Pool, rule: &FilterRule) -> AppResult<()> {
    sqlx::query(
        "UPDATE filter_rules \
         SET scope = ?2, scope_value = ?3, target = ?4, action = ?5, pattern = ?6, \
             enabled = ?7, note = ?8 \
         WHERE id = ?1",
    )
    .bind(rule.id)
    .bind(&rule.scope)
    .bind(&rule.scope_value)
    .bind(&rule.target)
    .bind(&rule.action)
    .bind(&rule.pattern)
    .bind(rule.enabled)
    .bind(&rule.note)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn set_enabled(pool: &Pool, id: Uuid, enabled: bool) -> AppResult<()> {
    sqlx::query("UPDATE filter_rules SET enabled = ?2 WHERE id = ?1")
        .bind(id)
        .bind(enabled)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn delete(pool: &Pool, id: Uuid) -> AppResult<()> {
    sqlx::query("DELETE FROM filter_rules WHERE id = ?1")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

/// 解析某发件人适用的每-target action。优先级 email > domain > global；每 target 独立取最高命中。
/// UNIQUE(scope, scope_value, target) 保证同层无冲突。仅 enabled 规则参与。
pub async fn resolve_for(pool: &Pool, sender: Option<&str>) -> AppResult<ResolvedRules> {
    let email = addr::extract_email(sender);
    let domain = email
        .as_deref()
        .and_then(addr::domain_of)
        .map(str::to_owned);

    let rows = sqlx::query_as::<_, FilterRule>(&format!(
        "SELECT {COLS} FROM filter_rules WHERE enabled = 1"
    ))
    .fetch_all(pool)
    .await?;

    // (weight, action, pattern) per target. Higher weight wins.
    let mut best: HashMap<String, (u8, Action, Option<String>)> = HashMap::new();

    for r in &rows {
        let weight = match r.scope.as_str() {
            "global" if r.scope_value.is_empty() => 0u8,
            "domain" => match domain.as_deref() {
                Some(d) if d.eq_ignore_ascii_case(&r.scope_value) => 1,
                _ => continue,
            },
            "email" => match email.as_deref() {
                Some(e) if e.eq_ignore_ascii_case(&r.scope_value) => 2,
                _ => continue,
            },
            _ => continue,
        };
        let action = match r.action.as_str() {
            "strip" => Action::Strip,
            _ => Action::Keep,
        };
        let entry = best.entry(r.target.clone());
        match entry {
            std::collections::hash_map::Entry::Occupied(mut o) if weight > o.get().0 => {
                *o.get_mut() = (weight, action, r.pattern.clone());
            }
            std::collections::hash_map::Entry::Occupied(_) => {}
            std::collections::hash_map::Entry::Vacant(v) => {
                v.insert((weight, action, r.pattern.clone()));
            }
        }
    }

    let mut out = ResolvedRules::default();
    if let Some((_, a, p)) = best.get("signature") {
        out.signature = Some(*a);
        out.signature_pattern = p.clone();
    }
    if let Some((_, a, _)) = best.get("quote") {
        out.quote = Some(*a);
    }
    if let Some((_, a, _)) = best.get("repeat") {
        out.repeat = Some(*a);
    }
    Ok(out)
}

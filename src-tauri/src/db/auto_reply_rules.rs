//! Repository for `auto_reply_rules` — 用户定义的自动回复规则。模式仿 `ai_models`。

use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::db::Pool;
use crate::error::AppResult;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
#[serde(rename_all = "camelCase")]
pub struct AutoReplyRule {
    pub id: Uuid,
    pub account_id: Uuid,
    pub name: String,
    pub enabled: bool,
    pub match_domain: Option<String>,
    pub match_category: Option<String>,
    pub match_priority_ceiling: Option<i64>,
    pub draft_intent: String,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoReplyRuleInput {
    pub account_id: Uuid,
    pub name: String,
    pub enabled: bool,
    pub match_domain: Option<String>,
    pub match_category: Option<String>,
    pub match_priority_ceiling: Option<i64>,
    pub draft_intent: String,
}

const COLS: &str = "id, account_id, name, enabled, match_domain, match_category, \
                    match_priority_ceiling, draft_intent, created_at";

pub async fn insert(pool: &Pool, input: &AutoReplyRuleInput) -> AppResult<AutoReplyRule> {
    let row = sqlx::query_as::<_, AutoReplyRule>(&format!(
        "INSERT INTO auto_reply_rules
           (id, account_id, name, enabled, match_domain, match_category, match_priority_ceiling, draft_intent)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
         RETURNING {COLS}"
    ))
    .bind(Uuid::new_v4())
    .bind(input.account_id)
    .bind(&input.name)
    .bind(input.enabled)
    .bind(&input.match_domain)
    .bind(&input.match_category)
    .bind(input.match_priority_ceiling)
    .bind(&input.draft_intent)
    .fetch_one(pool)
    .await?;
    Ok(row)
}

/// UI 用：某账户全部规则（含禁用），按创建时间。
pub async fn list_by_account(pool: &Pool, account_id: Uuid) -> AppResult<Vec<AutoReplyRule>> {
    let rows = sqlx::query_as::<_, AutoReplyRule>(&format!(
        "SELECT {COLS} FROM auto_reply_rules WHERE account_id = ?1 ORDER BY created_at"
    ))
    .bind(account_id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// 评估用：某账户已启用规则，按 created_at 升序（命中取首个）。
pub async fn list_enabled_by_account(
    pool: &Pool,
    account_id: Uuid,
) -> AppResult<Vec<AutoReplyRule>> {
    // 加 id 作次级排序键，使同毫秒内插入的多条规则首命中确定性可复现。
    let rows = sqlx::query_as::<_, AutoReplyRule>(&format!(
        "SELECT {COLS} FROM auto_reply_rules
         WHERE account_id = ?1 AND enabled = 1 ORDER BY created_at ASC, id ASC"
    ))
    .bind(account_id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn update(pool: &Pool, rule: &AutoReplyRule) -> AppResult<()> {
    sqlx::query(
        "UPDATE auto_reply_rules
         SET name = ?2, enabled = ?3, match_domain = ?4, match_category = ?5,
             match_priority_ceiling = ?6, draft_intent = ?7
         WHERE id = ?1",
    )
    .bind(rule.id)
    .bind(&rule.name)
    .bind(rule.enabled)
    .bind(&rule.match_domain)
    .bind(&rule.match_category)
    .bind(rule.match_priority_ceiling)
    .bind(&rule.draft_intent)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn set_enabled(pool: &Pool, id: Uuid, enabled: bool) -> AppResult<()> {
    sqlx::query("UPDATE auto_reply_rules SET enabled = ?2 WHERE id = ?1")
        .bind(id)
        .bind(enabled)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn delete(pool: &Pool, id: Uuid) -> AppResult<()> {
    sqlx::query("DELETE FROM auto_reply_rules WHERE id = ?1")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

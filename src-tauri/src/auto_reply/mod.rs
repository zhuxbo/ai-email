//! 自动回复：规则评估 + 命中入队。AI 草稿待审，本模块绝不发送邮件。

pub mod rules;

use uuid::Uuid;

use crate::db::{auto_reply_rules, suggested_replies, Pool};
use crate::error::AppResult;
use rules::{rule_matches, MatchCandidate};

/// 取一批邮件的匹配相关字段（仅 from_addr/category/priority，省 token）。
async fn fetch_candidates(pool: &Pool, ids: &[Uuid]) -> AppResult<Vec<MatchCandidate>> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let placeholders = vec!["?"; ids.len()].join(", ");
    let sql = format!(
        "SELECT id, from_addr, category, priority FROM messages WHERE id IN ({placeholders})"
    );
    let mut q = sqlx::query_as::<_, MatchCandidate>(&sql);
    for id in ids {
        q = q.bind(id);
    }
    Ok(q.fetch_all(pool).await?)
}

/// 对 new_ids 评估该账户的启用规则，命中首个即入队（懒：仅入队、不起草、不发送）。
/// **调用时机：必须在 sync 的 classify 之后顺序 await**——否则 category/priority 尚未写回，
/// 基于它们的规则会因竞态永不命中（仅 domain 规则侥幸命中）。见 spec §3/§6。
pub async fn evaluate_rules(pool: &Pool, account_id: Uuid, new_ids: &[Uuid]) -> AppResult<()> {
    if new_ids.is_empty() {
        return Ok(());
    }
    let rules = auto_reply_rules::list_enabled_by_account(pool, account_id).await?;
    if rules.is_empty() {
        return Ok(());
    }
    for cand in fetch_candidates(pool, new_ids).await? {
        if let Some(rule) = rules.iter().find(|r| rule_matches(&cand, r)) {
            suggested_replies::insert_if_absent(
                pool,
                cand.id,
                rule.id,
                &rule.draft_intent,
                &rule.name,
            )
            .await?;
        }
    }
    Ok(())
}

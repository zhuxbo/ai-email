//! 自动回复：规则评估 + 命中入队。AI 草稿待审，本模块绝不发送邮件。

pub mod rules;

use std::collections::HashMap;

use uuid::Uuid;

use crate::db::{auto_reply_rules, suggested_replies, Pool};
use crate::error::AppResult;
use rules::{rule_matches, MatchCandidate};

/// 取一批邮件的匹配相关字段（仅 from_addr/category/priority，省 token）。
/// 按 IN_CHUNK_SIZE 分块查询，避免超过 SQLite 绑定变量上限（增量同步无上界）。
async fn fetch_candidates(pool: &Pool, ids: &[Uuid]) -> AppResult<Vec<MatchCandidate>> {
    use crate::db::messages::IN_CHUNK_SIZE;
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let mut out = Vec::with_capacity(ids.len());
    for chunk in ids.chunks(IN_CHUNK_SIZE) {
        let placeholders = vec!["?"; chunk.len()].join(", ");
        let sql = format!(
            "SELECT id, from_addr, category, priority FROM messages WHERE id IN ({placeholders}) AND category_locked = 0"
        );
        let mut q = sqlx::query_as::<_, MatchCandidate>(&sql);
        for id in chunk {
            q = q.bind(id);
        }
        out.extend(q.fetch_all(pool).await?);
    }
    Ok(out)
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

/// 统一入口：给任意一批 message id（可跨账户）评估自动回复规则。
///
/// 自动按 `account_id` 分组后分别调用 [`evaluate_rules`]，调用方无需感知账户边界。
/// 适用于 **手动重分类**（`ai_classify` 命令）等写回 category/priority 后需要触发评估的场景。
///
/// **不变量：只入队建议草稿，绝不自动发送。**
pub async fn evaluate_rules_for_messages(pool: &Pool, message_ids: &[Uuid]) -> AppResult<()> {
    if message_ids.is_empty() {
        return Ok(());
    }
    // 批量查 account_id，按 IN_CHUNK_SIZE 分块避免超过 SQLite 绑定变量上限。
    use crate::db::messages::IN_CHUNK_SIZE;
    let mut account_groups: HashMap<Uuid, Vec<Uuid>> = HashMap::new();
    for chunk in message_ids.chunks(IN_CHUNK_SIZE) {
        let placeholders = vec!["?"; chunk.len()].join(", ");
        let sql = format!("SELECT id, account_id FROM messages WHERE id IN ({placeholders})");
        let mut q = sqlx::query_as::<_, (Uuid, Uuid)>(&sql);
        for id in chunk {
            q = q.bind(id);
        }
        for (msg_id, account_id) in q.fetch_all(pool).await? {
            account_groups.entry(account_id).or_default().push(msg_id);
        }
    }
    for (account_id, ids) in account_groups {
        evaluate_rules(pool, account_id, &ids).await?;
    }
    Ok(())
}

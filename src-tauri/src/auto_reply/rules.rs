//! 规则匹配纯函数。无 DB、无 IO，便于穷举单测。
//! 条件全为 AND；未指定（None）=不限。priority 数值越小越重要（1=最重要），
//! 命中判据 `candidate.priority <= rule.match_priority_ceiling`。

use sqlx::FromRow;
use uuid::Uuid;

use crate::db::auto_reply_rules::AutoReplyRule;

/// 一封待评估邮件的匹配相关字段（由 messages 表取）。
#[derive(Debug, Clone, FromRow)]
pub struct MatchCandidate {
    pub id: Uuid,
    pub from_addr: Option<String>,
    pub category: Option<String>,
    pub priority: Option<i32>,
}

pub fn rule_matches(c: &MatchCandidate, rule: &AutoReplyRule) -> bool {
    if let Some(dom) = &rule.match_domain {
        let dom = dom.to_lowercase();
        match &c.from_addr {
            Some(f) if f.to_lowercase().contains(&dom) => {}
            _ => return false,
        }
    }
    if let Some(cat) = &rule.match_category {
        if c.category.as_deref() != Some(cat.as_str()) {
            return false;
        }
    }
    if let Some(ceiling) = rule.match_priority_ceiling {
        match c.priority {
            Some(p) if i64::from(p) <= ceiling => {}
            _ => return false,
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::OffsetDateTime;

    fn rule(domain: Option<&str>, category: Option<&str>, ceiling: Option<i64>) -> AutoReplyRule {
        AutoReplyRule {
            id: Uuid::new_v4(),
            account_id: Uuid::new_v4(),
            name: "r".into(),
            enabled: true,
            match_domain: domain.map(String::from),
            match_category: category.map(String::from),
            match_priority_ceiling: ceiling,
            draft_intent: "i".into(),
            created_at: OffsetDateTime::UNIX_EPOCH,
        }
    }
    fn cand(from: Option<&str>, cat: Option<&str>, pri: Option<i32>) -> MatchCandidate {
        MatchCandidate {
            id: Uuid::new_v4(),
            from_addr: from.map(String::from),
            category: cat.map(String::from),
            priority: pri,
        }
    }

    #[test]
    fn empty_rule_matches_everything() {
        assert!(rule_matches(
            &cand(None, None, None),
            &rule(None, None, None)
        ));
        assert!(rule_matches(
            &cand(Some("x@y.com"), Some("work"), Some(2)),
            &rule(None, None, None)
        ));
    }

    #[test]
    fn domain_is_case_insensitive_substring() {
        let r = rule(Some("Client.com"), None, None);
        assert!(rule_matches(&cand(Some("a@CLIENT.com"), None, None), &r));
        assert!(rule_matches(&cand(Some("a@client.com.cn"), None, None), &r));
        assert!(!rule_matches(&cand(Some("a@other.com"), None, None), &r));
        assert!(!rule_matches(&cand(None, None, None), &r));
    }

    #[test]
    fn category_exact_and_graceful_on_none() {
        let r = rule(None, Some("work"), None);
        assert!(rule_matches(&cand(None, Some("work"), None), &r));
        assert!(!rule_matches(&cand(None, Some("personal"), None), &r));
        assert!(!rule_matches(&cand(None, None, None), &r));
    }

    #[test]
    fn priority_ceiling_is_upper_bound_on_value() {
        let r = rule(None, None, Some(1));
        assert!(rule_matches(&cand(None, None, Some(1)), &r));
        assert!(!rule_matches(&cand(None, None, Some(2)), &r));
        assert!(!rule_matches(&cand(None, None, None), &r));
        let r3 = rule(None, None, Some(3));
        assert!(rule_matches(&cand(None, None, Some(3)), &r3));
        assert!(rule_matches(&cand(None, None, Some(1)), &r3));
    }

    #[test]
    fn conditions_are_anded() {
        let r = rule(Some("client.com"), Some("work"), Some(1));
        assert!(rule_matches(
            &cand(Some("a@client.com"), Some("work"), Some(1)),
            &r
        ));
        assert!(!rule_matches(
            &cand(Some("a@client.com"), Some("work"), Some(2)),
            &r
        ));
        assert!(!rule_matches(
            &cand(Some("a@other.com"), Some("work"), Some(1)),
            &r
        ));
    }
}

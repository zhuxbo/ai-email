//! 发件人黑白名单：全局共享（无 account_id）。
//! struct/CRUD 仿 db/auto_reply_rules.rs；命中判定见 verdict（Task 5），判型见 normalize_entry（Task 4）。

use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::db::{is_unique_violation, Pool};
use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
#[serde(rename_all = "camelCase")]
pub struct SenderFilter {
    pub id: Uuid,
    pub list_type: String,  // 'black' | 'white'
    pub match_type: String, // 'address' | 'domain' | 'domain_glob'
    pub pattern: String,
    pub note: Option<String>,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
}

pub enum Verdict {
    Blacklist,
    Whitelist,
    None,
}

pub async fn load_all(pool: &Pool) -> AppResult<Vec<SenderFilter>> {
    let rows = sqlx::query_as::<_, SenderFilter>(
        "SELECT id, list_type, match_type, pattern, note, created_at
         FROM sender_filters ORDER BY created_at DESC",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn insert(
    pool: &Pool,
    list_type: &str,
    match_type: &str,
    pattern: &str,
    note: Option<&str>,
) -> AppResult<SenderFilter> {
    let id = Uuid::new_v4();
    let list_label = if list_type == "black" { "黑" } else { "白" };
    let result = sqlx::query_as::<_, SenderFilter>(
        "INSERT INTO sender_filters (id, list_type, match_type, pattern, note)
         VALUES (?, ?, ?, ?, ?)
         RETURNING id, list_type, match_type, pattern, note, created_at",
    )
    .bind(id)
    .bind(list_type)
    .bind(match_type)
    .bind(pattern)
    .bind(note)
    .fetch_one(pool)
    .await;
    match result {
        Ok(record) => Ok(record),
        Err(e) if is_unique_violation(&e) => {
            Err(AppError::Config(format!("该条目已在{list_label}名单中")))
        }
        Err(e) => Err(e.into()),
    }
}

pub async fn delete(pool: &Pool, id: Uuid) -> AppResult<()> {
    sqlx::query("DELETE FROM sender_filters WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

/// §2.2 命中判定。优先级阶梯：具体度（address > domain > domain_glob），同档白先于黑。
pub fn verdict(from_addr: Option<&str>, filters: &[SenderFilter]) -> Verdict {
    let Some(addr) = crate::addr::extract_email(from_addr) else {
        return Verdict::None;
    };
    let domain = crate::addr::domain_of(&addr);

    let hits = |mt: &str, lt: &str| -> bool {
        filters.iter().any(|f| {
            f.match_type == mt
                && f.list_type == lt
                && match mt {
                    "address" => addr == f.pattern,
                    "domain" => domain == Some(f.pattern.as_str()),
                    "domain_glob" => {
                        domain.is_some_and(|d| crate::addr::domain_matches(d, &f.pattern))
                    }
                    _ => false,
                }
        })
    };

    // 七层阶梯，自上而下命中即返回
    for mt in ["address", "domain", "domain_glob"] {
        if hits(mt, "white") {
            return Verdict::Whitelist;
        }
        if hits(mt, "black") {
            return Verdict::Blacklist;
        }
    }
    Verdict::None
}

/// §2.1 录入规范化 + 判型 + 校验。返回 (match_type, pattern)。纯函数可单测。
pub fn normalize_entry(value: &str) -> AppResult<(String, String)> {
    let v = value.trim().to_ascii_lowercase();
    if v.is_empty() {
        return Err(AppError::Config("名单条目不能为空".into()));
    }
    // 1. glob：去前导 '@' 后以 "*." 开头
    let no_at = v.strip_prefix('@').unwrap_or(v.as_str());
    if let Some(base) = no_at.strip_prefix("*.") {
        validate_domain(base)?;
        return Ok(("domain_glob".into(), base.to_string()));
    }
    // 2. domain：'@' 开头
    if let Some(domain) = v.strip_prefix('@') {
        validate_domain(domain)?;
        return Ok(("domain".into(), domain.to_string()));
    }
    // 3. address：含 '@'
    if v.contains('@') {
        validate_address(&v)?;
        return Ok(("address".into(), v));
    }
    // 4. 裸域名
    validate_domain(&v)?;
    Ok(("domain".into(), v))
}

fn validate_domain(d: &str) -> AppResult<()> {
    if !d.is_ascii() {
        return Err(AppError::Config(
            "域名须为 ASCII（IDN 请用 punycode xn-- 形式）".into(),
        ));
    }
    if d.contains(' ') || d.contains('@') || d.contains('*') {
        return Err(AppError::Config("域名含非法字符".into()));
    }
    if !d.contains('.') || d.starts_with('.') || d.ends_with('.') || d.contains("..") {
        return Err(AppError::Config(
            "域名格式非法（需形如 example.com）".into(),
        ));
    }
    Ok(())
}

fn validate_address(v: &str) -> AppResult<()> {
    if v.contains(' ') || v.contains('*') {
        return Err(AppError::Config("地址含非法字符".into()));
    }
    let parts: Vec<&str> = v.split('@').collect();
    if parts.len() != 2 || parts[0].is_empty() {
        return Err(AppError::Config(
            "地址格式非法（需形如 user@example.com）".into(),
        ));
    }
    validate_domain(parts[1])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn insert_list_delete_roundtrip() {
        let pool = crate::db::test_pool().await;
        let f = insert(&pool, "black", "address", "a@x.com", Some("垃圾佬"))
            .await
            .unwrap();
        assert_eq!(f.list_type, "black");
        assert_eq!(f.pattern, "a@x.com");
        let all = load_all(&pool).await.unwrap();
        assert_eq!(all.len(), 1);
        delete(&pool, f.id).await.unwrap();
        assert_eq!(load_all(&pool).await.unwrap().len(), 0);
    }

    #[tokio::test]
    async fn duplicate_insert_returns_config_error() {
        let pool = crate::db::test_pool().await;
        insert(&pool, "black", "domain", "x.com", None)
            .await
            .unwrap();
        let err = insert(&pool, "black", "domain", "x.com", None)
            .await
            .unwrap_err();
        match err {
            AppError::Config(msg) => assert!(msg.contains("已在黑名单中")),
            other => panic!("期望 Config，得到 {other:?}"),
        }
    }

    #[test]
    fn normalize_classifies_three_types() {
        assert_eq!(
            normalize_entry("a@x.com").unwrap(),
            ("address".into(), "a@x.com".into())
        );
        assert_eq!(
            normalize_entry("A@X.COM").unwrap(),
            ("address".into(), "a@x.com".into())
        );
        assert_eq!(
            normalize_entry("@x.com").unwrap(),
            ("domain".into(), "x.com".into())
        );
        assert_eq!(
            normalize_entry("x.com").unwrap(),
            ("domain".into(), "x.com".into())
        );
        assert_eq!(
            normalize_entry("*.x.com").unwrap(),
            ("domain_glob".into(), "x.com".into())
        );
        assert_eq!(
            normalize_entry("@*.x.com").unwrap(),
            ("domain_glob".into(), "x.com".into())
        );
    }

    #[test]
    fn normalize_rejects_dead_entries() {
        for bad in [
            "",
            "   ",
            "*.com",
            "x*.com",
            "a@b@x.com",
            "a @x.com",
            "@x.com.",
            "*",
            "x.com.",
            ".x.com",
            "x..com",
            "*.例え.jp",
            "中文.com",
        ] {
            assert!(normalize_entry(bad).is_err(), "应拒: {bad:?}");
        }
    }

    fn mk(list: &str, mt: &str, pat: &str) -> SenderFilter {
        SenderFilter {
            id: Uuid::new_v4(),
            list_type: list.into(),
            match_type: mt.into(),
            pattern: pat.into(),
            note: None,
            created_at: OffsetDateTime::UNIX_EPOCH,
        }
    }

    fn is_black(v: &Verdict) -> bool {
        matches!(v, Verdict::Blacklist)
    }
    fn is_white(v: &Verdict) -> bool {
        matches!(v, Verdict::Whitelist)
    }
    fn is_none(v: &Verdict) -> bool {
        matches!(v, Verdict::None)
    }

    #[test]
    fn verdict_address_domain_glob_and_case() {
        let fs = vec![mk("black", "address", "a@x.com")];
        assert!(is_black(&verdict(Some("a@x.com"), &fs)));
        assert!(is_black(&verdict(Some("Alice <A@X.COM>"), &fs))); // 大小写 + 显示名
        assert!(is_none(&verdict(Some("b@x.com"), &fs)));

        let fs = vec![mk("black", "domain", "x.com")];
        assert!(is_black(&verdict(Some("a@x.com"), &fs)));
        assert!(is_none(&verdict(Some("a@sub.x.com"), &fs))); // 精确域不含子域

        let fs = vec![mk("black", "domain_glob", "x.com")];
        assert!(is_black(&verdict(Some("a@x.com"), &fs))); // 含裸域
        assert!(is_black(&verdict(Some("a@sub.x.com"), &fs))); // 子域
        assert!(is_black(&verdict(Some("a@a.b.x.com"), &fs))); // 多层
        assert!(is_none(&verdict(Some("a@evilx.com"), &fs))); // 后缀混淆
    }

    #[test]
    fn verdict_malformed_and_empty_are_none() {
        let fs = vec![mk("black", "domain", "x.com")];
        for bad in [None, Some(""), Some("Alice <>"), Some("no-at"), Some("  ")] {
            assert!(is_none(&verdict(bad, &fs)), "应 None: {bad:?}");
        }
        // a@b@x.com 的域是 x.com → 命中 domain 条目
        assert!(is_black(&verdict(Some("a@b@x.com"), &fs)));
        // 空名单短路
        assert!(is_none(&verdict(Some("a@x.com"), &[])));
    }

    #[test]
    fn verdict_priority_specificity_then_white() {
        // 白通配 + 黑地址，来信地址 → 具体的黑胜
        let fs = vec![
            mk("white", "domain_glob", "x.com"),
            mk("black", "address", "ceo@x.com"),
        ];
        assert!(is_black(&verdict(Some("ceo@x.com"), &fs)));
        // 黑通配 + 白地址，来信地址 → 具体的白胜
        let fs = vec![
            mk("black", "domain_glob", "x.com"),
            mk("white", "address", "ceo@x.com"),
        ];
        assert!(is_white(&verdict(Some("ceo@x.com"), &fs)));
        // 同档同 pattern 黑白并存 → 白优先
        let fs = vec![
            mk("black", "domain", "x.com"),
            mk("white", "domain", "x.com"),
        ];
        assert!(is_white(&verdict(Some("a@x.com"), &fs)));
        // 地址 > 域名：白地址 + 黑精确域
        let fs = vec![
            mk("white", "address", "a@x.com"),
            mk("black", "domain", "x.com"),
        ];
        assert!(is_white(&verdict(Some("a@x.com"), &fs)));
    }
}

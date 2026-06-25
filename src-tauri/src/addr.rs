//! 发件地址解析与域名匹配纯函数。无 IO，可穷举单测。
//! 供 auto_reply::rules 与 db::sender_filters 共用，避免两套分叉。

/// 提取裸 email（小写）。失败（None/空/无 '@'/'@' 在首尾）→ None，绝不 panic。
/// 取最后一对非空 `<...>` 内，否则整串。
pub fn extract_email(from: Option<&str>) -> Option<String> {
    let s = from?.trim();
    if s.is_empty() {
        return None;
    }
    let inner = match (s.rfind('<'), s.rfind('>')) {
        (Some(l), Some(r)) if r > l + 1 => &s[l + 1..r],
        _ => s,
    };
    let addr = inner.trim().to_ascii_lowercase();
    let at = addr.find('@')?;
    if at == 0 || at == addr.len() - 1 {
        return None;
    }
    Some(addr)
}

/// email 的域名段（最后一个 '@' 之后）。无 '@' 或域名空 → None。
pub fn domain_of(email: &str) -> Option<&str> {
    let (_, domain) = email.rsplit_once('@')?;
    (!domain.is_empty()).then_some(domain)
}

/// 子域匹配（大小写不敏感）：domain == pattern || domain.ends_with(".{pattern}")。
pub fn domain_matches(domain: &str, pattern: &str) -> bool {
    let d = domain.to_lowercase();
    let p = pattern.to_lowercase();
    d == p || d.ends_with(&format!(".{p}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_email_handles_real_and_malformed() {
        assert_eq!(
            extract_email(Some("Alice <a@x.com>")).as_deref(),
            Some("a@x.com")
        );
        assert_eq!(extract_email(Some("a@x.com")).as_deref(), Some("a@x.com"));
        assert_eq!(
            extract_email(Some("  A@X.COM  ")).as_deref(),
            Some("a@x.com")
        );
        // 显示名含域名样文本：取尖括号内
        assert_eq!(
            extract_email(Some("example.com Fake <real@evil.org>")).as_deref(),
            Some("real@evil.org")
        );
        // 畸形/缺失 → None
        assert_eq!(extract_email(None), None);
        assert_eq!(extract_email(Some("")), None);
        assert_eq!(extract_email(Some("   ")), None);
        assert_eq!(extract_email(Some("Alice <>")), None);
        assert_eq!(extract_email(Some("no-at-here")), None);
        assert_eq!(extract_email(Some("@x.com")), None); // @ 在首
        assert_eq!(extract_email(Some("a@")), None); // @ 在尾
    }

    #[test]
    fn domain_of_takes_last_at_segment() {
        assert_eq!(domain_of("a@x.com"), Some("x.com"));
        assert_eq!(domain_of("a@b@x.com"), Some("x.com"));
        assert_eq!(domain_of("no-at"), None);
        assert_eq!(domain_of("a@"), None);
    }

    #[test]
    fn domain_matches_exact_and_subdomain_no_suffix_confusion() {
        assert!(domain_matches("x.com", "x.com"));
        assert!(domain_matches("a.x.com", "x.com"));
        assert!(domain_matches("a.b.x.com", "x.com"));
        assert!(domain_matches("A.X.COM", "x.com")); // 大小写不敏感
        assert!(!domain_matches("evilx.com", "x.com")); // 后缀混淆防护
        assert!(!domain_matches("notx.com", "x.com"));
    }
}

//! 会话上下文：物化整条会话并切分，供 conversation_thread 与剥离引擎复用。

/// 规范化地址：小写 + 去 display-name（取尖括号内）。
// Task 7 接入后移除（届时三函数经 load_thread_context 获得非-test 调用方）。
#[allow(dead_code)]
pub(crate) fn normalize_addr(raw: &str) -> String {
    let s = raw.trim();
    let inner = match (s.rfind('<'), s.rfind('>')) {
        (Some(a), Some(b)) if b > a + 1 => &s[a + 1..b],
        _ => s,
    };
    inner.trim().to_lowercase()
}

/// 是否自己发的：在 Sent OR from 规范化 == 账户邮箱。取 OR（多判 own 安全：下游 (a) 找不到分隔符回落 (b)）。
// Task 7 接入后移除
#[allow(dead_code)]
pub(crate) fn is_own_message(
    mailbox_special_use: Option<&str>,
    from_addr: Option<&str>,
    account_email: &str,
) -> bool {
    if mailbox_special_use == Some("sent") {
        return true;
    }
    match from_addr {
        Some(f) => normalize_addr(f) == normalize_addr(account_email),
        None => false,
    }
}

/// 在按 sent_at 升序的成员里定位当前封下标。
// Task 7 接入后移除
#[allow(dead_code)]
pub(crate) fn current_index(ids: &[uuid::Uuid], current: uuid::Uuid) -> Option<usize> {
    ids.iter().position(|&id| id == current)
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn normalize_strips_and_lowercases() {
        assert_eq!(normalize_addr("Me <Me@QQ.com>"), "me@qq.com");
        assert_eq!(normalize_addr("  PEER@x.com "), "peer@x.com");
    }

    #[test]
    fn is_own_true_in_sent_even_if_from_differs() {
        assert!(is_own_message(
            Some("sent"),
            Some("alias@other.com"),
            "me@qq.com"
        ));
    }

    #[test]
    fn is_own_true_when_from_matches() {
        assert!(is_own_message(
            Some("inbox"),
            Some("Me <me@qq.com>"),
            "me@qq.com"
        ));
    }

    #[test]
    fn is_own_false_for_peer() {
        assert!(!is_own_message(
            Some("inbox"),
            Some("peer@x.com"),
            "me@qq.com"
        ));
        assert!(!is_own_message(None, None, "me@qq.com"));
    }

    #[test]
    fn current_index_locates_or_none() {
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        assert_eq!(current_index(&[a, b], b), Some(1));
        assert_eq!(current_index(&[a], b), None);
    }
}

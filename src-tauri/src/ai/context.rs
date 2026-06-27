//! 会话上下文：物化整条会话并切分，供 conversation_thread 与剥离引擎复用。

use secrecy::SecretString;
use uuid::Uuid;

use crate::db::accounts::Account;
use crate::db::messages::MessageHeader;
use crate::db::{self, Pool};
use crate::error::{AppError, AppResult};

/// 规范化地址：小写 + 去 display-name（取尖括号内）。
pub(crate) fn normalize_addr(raw: &str) -> String {
    let s = raw.trim();
    let inner = match (s.rfind('<'), s.rfind('>')) {
        (Some(a), Some(b)) if b > a + 1 => &s[a + 1..b],
        _ => s,
    };
    inner.trim().to_lowercase()
}

/// 是否自己发的：在 Sent OR from 规范化 == 账户邮箱。取 OR（多判 own 安全：下游 (a) 找不到分隔符回落 (b)）。
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
pub(crate) fn current_index(ids: &[uuid::Uuid], current: uuid::Uuid) -> Option<usize> {
    ids.iter().position(|&id| id == current)
}

pub struct ThreadMember {
    pub header: MessageHeader,
    pub text_plain: Option<String>,
    pub html: Option<String>,
    pub is_own: bool,
}

pub struct ThreadContext {
    pub thread_id: Option<String>,
    pub members: Vec<ThreadMember>,
    pub current_index: usize, // Plan A 不消费；Plan B extract 据此切 prior
    pub sent_sync_ok: bool,
}

pub(crate) async fn take_auth(account_id: Uuid) -> AppResult<SecretString> {
    tokio::task::spawn_blocking(move || crate::keychain::get_auth_code(account_id))
        .await
        .map_err(|e| AppError::Other(anyhow::anyhow!(e)))?
}

async fn build_member(
    pool: &Pool,
    account: &Account,
    header: MessageHeader,
) -> AppResult<ThreadMember> {
    let mailbox = db::mailboxes::get(pool, header.mailbox_id).await?;
    let body = db::bodies::get(pool, header.id).await?;
    let is_own = is_own_message(
        mailbox.as_ref().and_then(|mb| mb.special_use.as_deref()),
        header.from_addr.as_deref(),
        &account.email,
    );
    Ok(ThreadMember {
        text_plain: body.as_ref().and_then(|b| b.text_plain.clone()),
        html: body.and_then(|b| b.html),
        is_own,
        header,
    })
}

/// 物化并切分一条会话。唯一入口：conversation_thread 与剥离引擎共用。内部自取 account+auth。
pub async fn load_thread_context(pool: &Pool, message_id: Uuid) -> AppResult<ThreadContext> {
    let current = db::messages::get(pool, message_id)
        .await?
        .ok_or_else(|| AppError::Config(format!("message {message_id} not found")))?;
    let account = db::accounts::get(pool, current.account_id)
        .await?
        .ok_or_else(|| AppError::Config(format!("account {} not found", current.account_id)))?;

    // 无 thread_id（极罕见：连 message-id 都没有）→ 单封：物化 current 后返回。
    let Some(thread_id) = current.thread_id.clone() else {
        let auth = take_auth(account.id).await?;
        let _ = crate::imap::materialize::materialize_one(pool, &account, &auth, current.id).await;
        let member = build_member(pool, &account, current).await?;
        return Ok(ThreadContext {
            thread_id: None,
            members: vec![member],
            current_index: 0,
            sent_sync_ok: true,
        });
    };

    let auth = take_auth(account.id).await?;
    // 按需补同步 Sent（5min 节流；失败 warn-not-fail）
    let sent_sync_ok = match crate::imap::sync::ensure_sent_synced(pool, &account, &auth).await {
        Ok(()) => true,
        Err(e) => {
            tracing::warn!(error = %e, "ensure_sent_synced failed; own replies may be missing");
            false
        }
    };
    let members = db::conversations::list_conversation(pool, account.id, &thread_id).await?;
    let report =
        crate::imap::materialize::materialize_thread_bodies(pool, &account, &auth, &members)
            .await?;
    if !report.failed.is_empty() {
        tracing::warn!(
            failed = ?report.failed,
            count = report.failed.len(),
            "部分会话正文物化失败；这些成员暂无正文"
        );
    }

    let ids: Vec<Uuid> = members.iter().map(|m| m.id).collect();
    let current_index = current_index(&ids, message_id).unwrap_or(0);
    let mut out = Vec::with_capacity(members.len());
    for m in members {
        out.push(build_member(pool, &account, m).await?);
    }
    Ok(ThreadContext {
        thread_id: Some(thread_id),
        members: out,
        current_index,
        sent_sync_ok,
    })
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

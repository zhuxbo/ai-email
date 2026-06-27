//! conversation_thread：打开邮件取整条会话（含 body）供详情对话流。
//! sender_group_thread：把「同发件人折叠组」（孤立通知/推广）当成一条会话流展示。
use crate::ai::context::{ThreadContext, ThreadMember};
use crate::db::messages::MessageHeader;
use crate::db::{self};
use crate::error::AppResult;
use crate::AppState;
use tauri::State;
use uuid::Uuid;

/// 同发件人组取多少封封顶（与折叠列表 sender 组同口径，避免大组无界物化）。
const SENDER_GROUP_CAP: i64 = 50;

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationMessage {
    #[serde(flatten)]
    pub header: MessageHeader,
    pub text_plain: Option<String>,
    pub html: Option<String>,
    pub is_own: bool,
}
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationView {
    pub thread_id: Option<String>,
    pub messages: Vec<ConversationMessage>,
    pub sent_sync_ok: bool,
}

fn message_from_member(m: ThreadMember) -> ConversationMessage {
    ConversationMessage {
        text_plain: m.text_plain,
        html: m.html,
        is_own: m.is_own,
        header: m.header,
    }
}
fn view_from_context(ctx: ThreadContext) -> ConversationView {
    ConversationView {
        thread_id: ctx.thread_id,
        sent_sync_ok: ctx.sent_sync_ok,
        messages: ctx.members.into_iter().map(message_from_member).collect(),
    }
}

#[tauri::command]
pub async fn conversation_thread(
    state: State<'_, AppState>,
    message_id: Uuid,
) -> AppResult<ConversationView> {
    let pool = state.pool().await?;
    let ctx = crate::ai::context::load_thread_context(pool, message_id).await?;
    Ok(view_from_context(ctx))
}

/// 同发件人组详情：把某账户内同一 `from_addr` 的孤立通知/推广（折叠 sender 组）当成一条会话流。
///
/// 不能复用 `load_thread_context`（它按 thread_id 选成员）——sender 组按 from_addr 选，故自建：
/// 选成员（db）→ 物化正文（IMAP，复用 thread 的 materialize_thread_bodies）→ 组装 view。
/// 返回的 `messages` 按 sent_at 升序（与 conversation_thread 一致；前端反转显示最新在上）。
/// 成员皆来自对端，`is_own = false`；`thread_id = None`（非真 thread）；`sent_sync_ok = true`
/// （不涉及 Sent 同步）。
#[tauri::command]
pub async fn sender_group_thread(
    state: State<'_, AppState>,
    account_id: Uuid,
    from_addr: String,
) -> AppResult<ConversationView> {
    let pool = state.pool().await?;
    let acct = db::accounts::get(pool, account_id)
        .await?
        .ok_or_else(|| crate::error::AppError::Config(format!("account {account_id} not found")))?;
    let auth = crate::ai::context::take_auth(acct.id).await?;

    // 选成员（DESC 最新在前）→ 物化正文（尽力而为；失败成员暂无 body）。
    let headers =
        db::messages::sender_group_members(pool, account_id, &from_addr, SENDER_GROUP_CAP).await?;
    let report =
        crate::imap::materialize::materialize_thread_bodies(pool, &acct, &auth, &headers).await?;
    if !report.failed.is_empty() {
        tracing::warn!(
            failed = ?report.failed,
            count = report.failed.len(),
            "部分同发件人组成员正文物化失败；这些成员暂无正文"
        );
    }

    // 组装：自建 ConversationMessage（从物化后的 body 取正文），按 sent_at 升序（反转 DESC）。
    let mut messages = Vec::with_capacity(headers.len());
    for header in headers.into_iter().rev() {
        let body = db::bodies::get(pool, header.id).await?;
        messages.push(ConversationMessage {
            text_plain: body.as_ref().and_then(|b| b.text_plain.clone()),
            html: body.and_then(|b| b.html),
            is_own: false,
            header,
        });
    }
    Ok(ConversationView {
        thread_id: None,
        messages,
        sent_sync_ok: true,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn minimal_header() -> MessageHeader {
        MessageHeader {
            id: Uuid::new_v4(),
            account_id: Uuid::new_v4(),
            mailbox_id: Uuid::new_v4(),
            imap_uid: 1,
            rfc_message_id: None,
            thread_id: None,
            subject: None,
            from_addr: None,
            to_addrs: vec![],
            cc_addrs: vec![],
            sent_at: None,
            internal_date: None,
            flags: vec![],
            size_bytes: None,
            has_attachment: false,
            snippet: None,
            priority: None,
            category: None,
            tags: vec![],
            body_fetched_at: None,
            references_header: None,
            filter_disabled: false,
            category_locked: false,
        }
    }
    #[test]
    fn view_preserves_order_and_flags() {
        let mk = |own: bool| ThreadMember {
            header: minimal_header(),
            text_plain: Some("x".into()),
            html: None,
            is_own: own,
        };
        // current_index=1 故意不被 view 读取（Plan A 不消费 current_index，见结构注释）
        let ctx = ThreadContext {
            thread_id: Some("t".into()),
            members: vec![mk(false), mk(true)],
            current_index: 1,
            sent_sync_ok: false,
        };
        let v = view_from_context(ctx);
        assert_eq!(v.messages.len(), 2);
        assert!(!v.messages[0].is_own);
        assert!(v.messages[1].is_own);
        assert!(!v.sent_sync_ok);
    }
}

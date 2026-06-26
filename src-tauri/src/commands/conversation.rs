//! conversation_thread：打开邮件取整条会话（含 body）供详情对话流。
use crate::ai::context::{ThreadContext, ThreadMember};
use crate::db::messages::MessageHeader;
use crate::error::AppResult;
use crate::AppState;
use tauri::State;
use uuid::Uuid;

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

//! 会话正文物化：缺 body 的成员按 mailbox 分组批量 UID FETCH 入库。
//! materialize_one 供拼引用单封；二者均走 [`ImapManager`] 复用连接
//! （fetch_raw_body 保留为附件裸字节原语，不并入此处）。
use std::collections::{HashMap, HashSet};

use secrecy::SecretString;
use uuid::Uuid;

use crate::db::accounts::Account;
use crate::db::messages::MessageHeader;
use crate::db::{self, Pool};
use crate::error::{AppError, AppResult};
use crate::imap::manager::ImapManager;
use crate::imap::parse;

#[derive(Debug, Default, PartialEq, Eq)]
pub struct MaterializeReport {
    pub fetched: usize,
    pub failed: Vec<Uuid>,
}

/// 纯函数：成员 + 已在库 body 的 id 集合 → 每 mailbox 仍需拉的 (uid, msg_id)。已有/uid 非法的跳过。
pub(crate) fn members_needing_body(
    members: &[MessageHeader],
    have_body: &HashSet<Uuid>,
) -> HashMap<Uuid, Vec<(u32, Uuid)>> {
    let mut out: HashMap<Uuid, Vec<(u32, Uuid)>> = HashMap::new();
    for m in members {
        if have_body.contains(&m.id) {
            continue;
        }
        // imap_uid 越界（理论不达：IMAP UID 是 u32）静默跳过——保持纯函数无副作用。
        if let Ok(uid) = u32::try_from(m.imap_uid) {
            out.entry(m.mailbox_id).or_default().push((uid, m.id));
        }
    }
    out
}

/// 单封：在复用连接上取 BODY[]、解析、入库 + 标记。供拼引用与单封物化复用。
pub async fn materialize_one(
    pool: &Pool,
    imap: &ImapManager,
    account: &Account,
    auth: &SecretString,
    message_id: Uuid,
) -> AppResult<()> {
    let msg = db::messages::get(pool, message_id)
        .await?
        .ok_or_else(|| AppError::Config(format!("message {message_id} not found")))?;
    let mailbox = db::mailboxes::get(pool, msg.mailbox_id)
        .await?
        .ok_or_else(|| AppError::Config(format!("mailbox {} not found", msg.mailbox_id)))?;
    let uid = u32::try_from(msg.imap_uid)
        .map_err(|_| AppError::Imap(format!("invalid imap_uid: {}", msg.imap_uid)))?;
    let mailbox_name = mailbox.name.as_str();
    let raw = imap
        .run(account, auth, |mut lease| {
            Box::pin(async move {
                let client = lease.client()?;
                client.select(mailbox_name).await?;
                client.uid_fetch_body(uid).await
            })
        })
        .await?;
    persist_body(pool, message_id, &raw).await
}

/// 批量：按 mailbox 分组拉缺失成员、逐封入库。尽力而为：单封失败计入 report.failed。
pub async fn materialize_thread_bodies(
    pool: &Pool,
    imap: &ImapManager,
    account: &Account,
    auth: &SecretString,
    members: &[MessageHeader],
) -> AppResult<MaterializeReport> {
    let mut have = HashSet::new();
    for m in members {
        if db::bodies::get(pool, m.id).await?.is_some() {
            have.insert(m.id);
        }
    }
    let mut need = members_needing_body(members, &have);
    let mut report = MaterializeReport::default();
    if need.is_empty() {
        return Ok(report);
    }
    // 引用先行：async move 会整体移动捕获，FnMut 二次调用即 E0507；借 &need（Copy）规避。
    let need_ref = &need;
    let fetched_by_group = imap
        .run(account, auth, |mut lease| {
            Box::pin(async move {
                let client = lease.client()?;
                // (mailbox_id, 该信箱批量 FETCH 到的 (uid, raw) 列表)。不同信箱的 UID 会重复，
                // 必须按信箱分组配对，不能全局按 UID 合并。
                let mut out: Vec<(Uuid, Vec<(u32, Vec<u8>)>)> = Vec::new();
                for (mailbox_id, items) in need_ref {
                let mailbox = match db::mailboxes::get(pool, *mailbox_id).await? {
                    Some(mb) => mb,
                    None => {
                        tracing::warn!(mailbox_id = %mailbox_id, count = items.len(), "mailbox not found during materialize; skipping group");
                        continue;
                    }
                };
                if client.select(&mailbox.name).await.is_err() {
                    tracing::warn!(mailbox = %mailbox.name, count = items.len(), "select failed during materialize; skipping group");
                    continue;
                }
                let uids: Vec<u32> = items.iter().map(|(u, _)| *u).collect();
                match client.uid_fetch_bodies(&uids).await {
                    Ok(v) => out.push((*mailbox_id, v)),
                    Err(e) => {
                        tracing::warn!(error = %e, mailbox = %mailbox.name, "batch body fetch failed; skipping group");
                    }
                }
            }
            Ok(out)
            })
        })
        .await?;

    let mut fetched_count = 0_usize;
    for (mailbox_id, fetched) in fetched_by_group {
        let by_uid: HashMap<u32, Vec<u8>> = fetched.into_iter().collect();
        let Some(items) = need.remove(&mailbox_id) else {
            continue;
        };
        for (uid, msg_id) in items {
            match by_uid.get(&uid) {
                Some(raw) => match persist_body(pool, msg_id, raw).await {
                    Ok(()) => fetched_count += 1,
                    Err(_) => report.failed.push(msg_id),
                },
                // 拉取阶段单封缺流（服务端未返回该 UID 的 BODY）。
                None => report.failed.push(msg_id),
            }
        }
    }
    // 拉取阶段被跳过的组（信箱缺行 / select 失败 / 批量 FETCH 失败）成员全部计失败。
    for items in need.into_values() {
        report.failed.extend(items.into_iter().map(|(_, id)| id));
    }
    report.fetched = fetched_count;
    Ok(report)
}

async fn persist_body(pool: &Pool, message_id: Uuid, raw: &[u8]) -> AppResult<()> {
    let parsed = parse::parse_body(raw);
    let snippet = parsed
        .text_plain
        .as_deref()
        .and_then(|t| parse::snippet(t, 200));
    db::bodies::upsert(pool, message_id, &parsed).await?;
    db::messages::mark_body_fetched(pool, message_id, parsed.has_attachment, snippet).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_header(id: Uuid, mailbox_id: Uuid, imap_uid: i64) -> MessageHeader {
        MessageHeader {
            id,
            account_id: Uuid::new_v4(),
            mailbox_id,
            imap_uid,
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
    fn groups_missing_by_mailbox_and_skips_present() {
        let mb1 = Uuid::new_v4();
        let mb2 = Uuid::new_v4();
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        let c = Uuid::new_v4();
        let members = vec![
            make_header(a, mb1, 10),
            make_header(b, mb1, 11),
            make_header(c, mb2, 20),
        ];
        let mut have = HashSet::new();
        have.insert(b);
        let need = members_needing_body(&members, &have);
        assert_eq!(need.get(&mb1), Some(&vec![(10u32, a)]));
        assert_eq!(need.get(&mb2), Some(&vec![(20u32, c)]));
    }
}

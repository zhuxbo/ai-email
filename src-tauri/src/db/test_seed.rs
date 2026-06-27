#![cfg(test)]
#![allow(dead_code)]

use uuid::Uuid;

use crate::db::Pool;

/// 建一个 account，返回 account_id。
pub(crate) async fn seed_account(pool: &Pool) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO accounts (id, email, provider, imap_host, smtp_host) \
         VALUES (?1, ?2, 'imap', 'imap.test', 'smtp.test')",
    )
    .bind(id)
    .bind(format!("acct-{id}@test.invalid"))
    .execute(pool)
    .await
    .unwrap();
    id
}

/// 建一个 mailbox（special_use: None=INBOX 类）。
pub(crate) async fn seed_mailbox(
    pool: &Pool,
    account_id: Uuid,
    name: &str,
    special_use: Option<&str>,
) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO mailboxes (id, account_id, name, special_use) VALUES (?1,?2,?3,?4)")
        .bind(id)
        .bind(account_id)
        .bind(name)
        .bind(special_use)
        .execute(pool)
        .await
        .unwrap();
    id
}

/// 建一封消息，可控 from_addr / thread_id / category / category_locked / imap_uid / flags。
///
/// `sent_at` 由 `imap_uid` 派生（非 NULL、按 uid 递增），使「代表=最新」断言对 sent_at 排序稳定。
#[allow(clippy::too_many_arguments)]
pub(crate) async fn seed_msg(
    pool: &Pool,
    account_id: Uuid,
    mailbox_id: Uuid,
    imap_uid: i64,
    from: &str,
    thread_id: Option<&str>,
    category: Option<&str>,
    locked: bool,
    flags: &str,
) -> Uuid {
    let id = Uuid::new_v4();
    let sent_at = format!("2026-01-01T00:00:{:02}Z", imap_uid.rem_euclid(60));
    sqlx::query(
        "INSERT INTO messages \
         (id, account_id, mailbox_id, imap_uid, flags, from_addr, thread_id, category, category_locked, sent_at) \
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
    )
    .bind(id)
    .bind(account_id)
    .bind(mailbox_id)
    .bind(imap_uid)
    .bind(flags)
    .bind(from)
    .bind(thread_id)
    .bind(category)
    .bind(locked as i64)
    .bind(sent_at)
    .execute(pool)
    .await
    .unwrap();
    id
}

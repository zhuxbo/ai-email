use ai_email_lib::db;
use uuid::Uuid;

async fn mem_pool() -> sqlx::SqlitePool {
    let opts = sqlx::sqlite::SqliteConnectOptions::new()
        .in_memory(true)
        .foreign_keys(true);
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(opts)
        .await
        .unwrap();
    db::MIGRATOR.run(&pool).await.unwrap();
    pool
}

async fn seed_account(pool: &sqlx::SqlitePool, email: &str) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO accounts (id, email, provider, imap_host, imap_port, smtp_host, smtp_port)
         VALUES (?1, ?2, 'qq', 'imap.qq.com', 993, 'smtp.qq.com', 465)",
    )
    .bind(id)
    .bind(email)
    .execute(pool)
    .await
    .unwrap();
    id
}

async fn seed_mailbox(
    pool: &sqlx::SqlitePool,
    account_id: Uuid,
    name: &str,
    special: Option<&str>,
) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO mailboxes (id, account_id, name, special_use) VALUES (?1, ?2, ?3, ?4)",
    )
    .bind(id)
    .bind(account_id)
    .bind(name)
    .bind(special)
    .execute(pool)
    .await
    .unwrap();
    id
}

async fn seed_message(
    pool: &sqlx::SqlitePool,
    account_id: Uuid,
    mailbox_id: Uuid,
    uid: i64,
    thread: &str,
    sent_at: &str,
    from: &str,
) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO messages (id, account_id, mailbox_id, imap_uid, thread_id, subject, from_addr,
            to_addrs, cc_addrs, sent_at, flags, has_attachment)
         VALUES (?1,?2,?3,?4,?5,'主题',?6,'[]','[]',?7,'[]',0)",
    )
    .bind(id)
    .bind(account_id)
    .bind(mailbox_id)
    .bind(uid)
    .bind(thread)
    .bind(from)
    .bind(sent_at)
    .execute(pool)
    .await
    .unwrap();
    id
}

#[tokio::test]
async fn get_by_special_use_finds_sent() {
    let pool = mem_pool().await;
    let acc = seed_account(&pool, "me@qq.com").await;
    seed_mailbox(&pool, acc, "INBOX", Some("inbox")).await;
    let sent_id = seed_mailbox(&pool, acc, "Sent Messages", Some("sent")).await;
    let found = db::mailboxes::get_by_special_use(&pool, acc, "sent")
        .await
        .unwrap();
    assert_eq!(found.map(|m| m.id), Some(sent_id));
    assert!(db::mailboxes::get_by_special_use(&pool, acc, "drafts")
        .await
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn list_conversation_spans_inbox_and_sent_ordered() {
    let pool = mem_pool().await;
    let acc = seed_account(&pool, "me@qq.com").await;
    let inbox = seed_mailbox(&pool, acc, "INBOX", Some("inbox")).await;
    let sent = seed_mailbox(&pool, acc, "Sent", Some("sent")).await;
    let m1 = seed_message(
        &pool,
        acc,
        inbox,
        1,
        "t1",
        "2026-06-25T10:00:00+00:00",
        "peer@x.com",
    )
    .await;
    let m2 = seed_message(
        &pool,
        acc,
        sent,
        1,
        "t1",
        "2026-06-25T11:00:00+00:00",
        "me@qq.com",
    )
    .await;
    let m3 = seed_message(
        &pool,
        acc,
        inbox,
        2,
        "t1",
        "2026-06-25T12:00:00+00:00",
        "peer@x.com",
    )
    .await;
    seed_message(
        &pool,
        acc,
        inbox,
        3,
        "t2",
        "2026-06-25T13:00:00+00:00",
        "peer@x.com",
    )
    .await;
    let conv = db::conversations::list_conversation(&pool, acc, "t1")
        .await
        .unwrap();
    assert_eq!(
        conv.iter().map(|m| m.id).collect::<Vec<_>>(),
        vec![m1, m2, m3]
    );
}

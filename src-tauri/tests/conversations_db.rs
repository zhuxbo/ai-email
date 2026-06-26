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

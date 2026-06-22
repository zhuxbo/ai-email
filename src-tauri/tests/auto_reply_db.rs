// 验证 0005 迁移生效 + 两表 FK 行为（CASCADE / SET NULL / UNIQUE）。
// in-memory pool（foreign_keys 必须开）。max_connections(1)：每个 `:memory:` 连接是独立空库，
// 限单连接才能让迁移后的表对后续查询可见。

use sqlx::Row;
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
    ai_email_lib::db::MIGRATOR.run(&pool).await.unwrap();
    pool
}

// 最小种子：插一个 account + 一封 message，返回它们的 id。
async fn seed_account_and_message(pool: &sqlx::SqlitePool) -> (Uuid, Uuid) {
    let account_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO accounts (id, email, provider, imap_host, imap_port, smtp_host, smtp_port)
         VALUES (?1, 'a@qq.com', 'qq', 'imap.qq.com', 993, 'smtp.qq.com', 465)",
    )
    .bind(account_id)
    .execute(pool)
    .await
    .unwrap();
    let mailbox_id = Uuid::new_v4();
    sqlx::query("INSERT INTO mailboxes (id, account_id, name) VALUES (?1, ?2, 'INBOX')")
        .bind(mailbox_id)
        .bind(account_id)
        .execute(pool)
        .await
        .unwrap();
    let msg_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO messages (id, account_id, mailbox_id, imap_uid, to_addrs, cc_addrs, flags)
         VALUES (?1, ?2, ?3, 1, '[]', '[]', '[]')",
    )
    .bind(msg_id)
    .bind(account_id)
    .bind(mailbox_id)
    .execute(pool)
    .await
    .unwrap();
    (account_id, msg_id)
}

#[tokio::test]
async fn migration_creates_tables_and_message_delete_cascades_suggestion() {
    let pool = mem_pool().await;
    let (account_id, msg_id) = seed_account_and_message(&pool).await;
    let rule_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO auto_reply_rules (id, account_id, name, draft_intent) VALUES (?1, ?2, 'r', 'i')",
    )
    .bind(rule_id)
    .bind(account_id)
    .execute(&pool)
    .await
    .unwrap();
    let sug_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO suggested_replies (id, message_id, rule_id, intent_snapshot, rule_name_snapshot)
         VALUES (?1, ?2, ?3, 'i', 'r')",
    )
    .bind(sug_id)
    .bind(msg_id)
    .bind(rule_id)
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query("DELETE FROM messages WHERE id = ?1")
        .bind(msg_id)
        .execute(&pool)
        .await
        .unwrap();
    let n: i64 = sqlx::query("SELECT COUNT(*) AS c FROM suggested_replies")
        .fetch_one(&pool)
        .await
        .unwrap()
        .get("c");
    assert_eq!(n, 0, "message 删除应级联清除其 suggested_replies");
}

#[tokio::test]
async fn rule_delete_sets_suggestion_rule_id_null_not_cascade() {
    let pool = mem_pool().await;
    let (account_id, msg_id) = seed_account_and_message(&pool).await;
    let rule_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO auto_reply_rules (id, account_id, name, draft_intent) VALUES (?1, ?2, 'r', 'i')",
    )
    .bind(rule_id)
    .bind(account_id)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO suggested_replies (id, message_id, rule_id, intent_snapshot, rule_name_snapshot)
         VALUES (?1, ?2, ?3, 'i', 'r')",
    )
    .bind(Uuid::new_v4())
    .bind(msg_id)
    .bind(rule_id)
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query("DELETE FROM auto_reply_rules WHERE id = ?1")
        .bind(rule_id)
        .execute(&pool)
        .await
        .unwrap();
    let row = sqlx::query("SELECT rule_id FROM suggested_replies WHERE message_id = ?1")
        .bind(msg_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    let rid: Option<Uuid> = row.get("rule_id");
    assert!(
        rid.is_none(),
        "rule 删除应把 suggested_replies.rule_id 置 NULL，保留建议"
    );
}

#[tokio::test]
async fn suggested_replies_message_id_unique_blocks_second_insert() {
    let pool = mem_pool().await;
    let (_account_id, msg_id) = seed_account_and_message(&pool).await;
    let ins = |sid: Uuid| {
        sqlx::query(
            "INSERT OR IGNORE INTO suggested_replies (id, message_id, intent_snapshot, rule_name_snapshot)
             VALUES (?1, ?2, 'i', 'r')",
        )
        .bind(sid)
        .bind(msg_id)
    };
    ins(Uuid::new_v4()).execute(&pool).await.unwrap();
    ins(Uuid::new_v4()).execute(&pool).await.unwrap();
    let n: i64 = sqlx::query("SELECT COUNT(*) AS c FROM suggested_replies")
        .fetch_one(&pool)
        .await
        .unwrap()
        .get("c");
    assert_eq!(
        n, 1,
        "同一 message_id 只应存在一条（UNIQUE + INSERT OR IGNORE）"
    );
}

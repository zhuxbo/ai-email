// 验证 0010 迁移生效：filter_rules 表 + UNIQUE 约束 + messages.filter_disabled 列。
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

#[tokio::test]
async fn migration_creates_filter_rules_and_unique_holds() {
    let pool = mem_pool().await;
    // 插一条 global signature strip 规则。
    sqlx::query(
        "INSERT INTO filter_rules (id, scope, scope_value, target, action) \
         VALUES (?1, 'global', '', 'signature', 'strip')",
    )
    .bind(Uuid::new_v4())
    .execute(&pool)
    .await
    .unwrap();

    // 同 (scope, scope_value, target) 第二条 → UNIQUE 拒绝。
    let dup = sqlx::query(
        "INSERT INTO filter_rules (id, scope, scope_value, target, action) \
         VALUES (?1, 'global', '', 'signature', 'keep')",
    )
    .bind(Uuid::new_v4())
    .execute(&pool)
    .await;
    assert!(
        dup.is_err(),
        "UNIQUE(scope, scope_value, target) 应拒绝重复"
    );

    // 读回校验默认值。
    let row = sqlx::query("SELECT enabled, pattern, note FROM filter_rules LIMIT 1")
        .fetch_one(&pool)
        .await
        .unwrap();
    let enabled: i64 = row.get("enabled");
    assert_eq!(enabled, 1, "enabled 默认应为 1");
}

#[tokio::test]
async fn migration_adds_filter_disabled_column_default_zero() {
    let pool = mem_pool().await;
    let account_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO accounts (id, email, provider, imap_host, imap_port, smtp_host, smtp_port) \
         VALUES (?1, 'a@qq.com', 'qq', 'imap.qq.com', 993, 'smtp.qq.com', 465)",
    )
    .bind(account_id)
    .execute(&pool)
    .await
    .unwrap();
    let mailbox_id = Uuid::new_v4();
    sqlx::query("INSERT INTO mailboxes (id, account_id, name) VALUES (?1, ?2, 'INBOX')")
        .bind(mailbox_id)
        .bind(account_id)
        .execute(&pool)
        .await
        .unwrap();
    let msg_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO messages (id, account_id, mailbox_id, imap_uid, to_addrs, cc_addrs, flags) \
         VALUES (?1, ?2, ?3, 1, '[]', '[]', '[]')",
    )
    .bind(msg_id)
    .bind(account_id)
    .bind(mailbox_id)
    .execute(&pool)
    .await
    .unwrap();

    let row = sqlx::query("SELECT filter_disabled FROM messages WHERE id = ?1")
        .bind(msg_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    let fd: i64 = row.get("filter_disabled");
    assert_eq!(fd, 0, "filter_disabled 默认应为 0");
}

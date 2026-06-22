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

#[tokio::test]
async fn auto_reply_rules_crud_roundtrip() {
    use ai_email_lib::db::auto_reply_rules as repo;
    let pool = mem_pool().await;
    let (account_id, _msg) = seed_account_and_message(&pool).await;

    let r = repo::insert(
        &pool,
        &repo::AutoReplyRuleInput {
            account_id,
            name: "工作紧急".into(),
            enabled: true,
            match_domain: None,
            match_category: Some("work".into()),
            match_priority_ceiling: Some(1),
            draft_intent: "礼貌确认今天内回复".into(),
        },
    )
    .await
    .unwrap();
    assert!(r.enabled);
    assert_eq!(r.match_category.as_deref(), Some("work"));

    repo::set_enabled(&pool, r.id, false).await.unwrap();
    assert!(repo::list_enabled_by_account(&pool, account_id)
        .await
        .unwrap()
        .is_empty());
    assert_eq!(
        repo::list_by_account(&pool, account_id)
            .await
            .unwrap()
            .len(),
        1
    );

    let mut edited = r.clone();
    edited.enabled = true;
    edited.match_priority_ceiling = Some(2);
    repo::update(&pool, &edited).await.unwrap();
    let again = &repo::list_enabled_by_account(&pool, account_id)
        .await
        .unwrap()[0];
    assert_eq!(again.match_priority_ceiling, Some(2));

    repo::delete(&pool, r.id).await.unwrap();
    assert!(repo::list_by_account(&pool, account_id)
        .await
        .unwrap()
        .is_empty());
}

#[tokio::test]
async fn list_pending_excludes_replied() {
    use ai_email_lib::db::suggested_replies as q;
    let pool = mem_pool().await;
    let (account_id, msg_id) = seed_account_and_message(&pool).await;
    let rule_id = Uuid::new_v4();
    sqlx::query("INSERT INTO auto_reply_rules (id, account_id, name, draft_intent) VALUES (?1, ?2, 'r', 'i')")
        .bind(rule_id).bind(account_id).execute(&pool).await.unwrap();

    q::insert_if_absent(&pool, msg_id, rule_id, "i", "r")
        .await
        .unwrap();
    assert_eq!(q::list_pending(&pool).await.unwrap().len(), 1);

    // 写一条 send_log 回复该邮件 → 派生为已回复 → 不在 pending 列表
    sqlx::query(
        "INSERT INTO send_log (id, account_id, in_reply_to, to_addrs, subject, ai_assisted, smtp_response)
         VALUES (?1, ?2, ?3, '[]', 's', 1, 'OK')",
    ).bind(Uuid::new_v4()).bind(account_id).bind(msg_id).execute(&pool).await.unwrap();
    assert!(
        q::list_pending(&pool).await.unwrap().is_empty(),
        "已回复邮件应从队列派生消失"
    );
}

#[tokio::test]
async fn dismiss_removes_suggestion_from_pending() {
    use ai_email_lib::db::suggested_replies as q;
    let pool = mem_pool().await;
    let (account_id, msg_id) = seed_account_and_message(&pool).await;
    let rule_id = Uuid::new_v4();
    sqlx::query("INSERT INTO auto_reply_rules (id, account_id, name, draft_intent) VALUES (?1, ?2, 'r', 'i')")
        .bind(rule_id).bind(account_id).execute(&pool).await.unwrap();

    q::insert_if_absent(&pool, msg_id, rule_id, "i", "r")
        .await
        .unwrap();
    let pending = q::list_pending(&pool).await.unwrap();
    assert_eq!(pending.len(), 1);

    // 忽略该建议 → status='dismissed' → 从 pending 队列消失
    q::dismiss(&pool, pending[0].id).await.unwrap();
    assert!(
        q::list_pending(&pool).await.unwrap().is_empty(),
        "dismissed 建议应从 pending 队列消失"
    );
}

#[tokio::test]
async fn evaluate_rules_matches_after_classification_and_degrades_without_it() {
    use ai_email_lib::auto_reply::evaluate_rules;
    use ai_email_lib::db::suggested_replies as q;
    let pool = mem_pool().await;
    let (account_id, msg_id) = seed_account_and_message(&pool).await; // from_addr=NULL, category=NULL, priority=NULL

    sqlx::query("UPDATE messages SET from_addr = 'boss@client.com' WHERE id = ?1")
        .bind(msg_id)
        .execute(&pool)
        .await
        .unwrap();

    // 规则 A：category=work（依赖分类）；规则 B：domain=client.com（不依赖分类）。
    // 平铺插入，不用闭包——避免「闭包产 async block 捕获外部 &pool」的借用检查坑。
    sqlx::query(
        "INSERT INTO auto_reply_rules (id, account_id, name, match_category, draft_intent)
         VALUES (?1, ?2, 'A-work', 'work', 'i')",
    )
    .bind(Uuid::new_v4())
    .bind(account_id)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO auto_reply_rules (id, account_id, name, match_domain, draft_intent)
         VALUES (?1, ?2, 'B-domain', 'client.com', 'i')",
    )
    .bind(Uuid::new_v4())
    .bind(account_id)
    .execute(&pool)
    .await
    .unwrap();

    // ① category 仍为 NULL（classify 未跑）→ 仅 domain 规则命中
    evaluate_rules(&pool, account_id, &[msg_id]).await.unwrap();
    let pend = q::list_pending(&pool).await.unwrap();
    assert_eq!(pend.len(), 1);
    assert_eq!(
        pend[0].rule_name_snapshot, "B-domain",
        "未分类时只应 domain 规则命中"
    );

    // ② 第二封邮件已分类（category=work）→ category 规则命中
    let msg2 = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO messages (id, account_id, mailbox_id, imap_uid, from_addr, category, priority, to_addrs, cc_addrs, flags)
         SELECT ?1, account_id, mailbox_id, 2, 'x@nowhere.com', 'work', 2, '[]', '[]', '[]' FROM messages WHERE id = ?2",
    )
    .bind(msg2)
    .bind(msg_id)
    .execute(&pool)
    .await
    .unwrap();
    evaluate_rules(&pool, account_id, &[msg2]).await.unwrap();
    let names: Vec<String> = q::list_pending(&pool)
        .await
        .unwrap()
        .into_iter()
        .map(|s| s.rule_name_snapshot)
        .collect();
    assert!(
        names.contains(&"A-work".to_string()),
        "分类写回后 category 规则应命中"
    );
}

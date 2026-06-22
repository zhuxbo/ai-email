//! SQLite layer integration test: real on-disk DB round-trips covering the bits that only fail
//! at runtime — strftime timestamps decoded into OffsetDateTime, JSON arrays (to_addrs / flags),
//! json_group_array tags, RETURNING, and BLOB UUIDs. The unit tests elsewhere don't touch the DB.

use ai_email_lib::db::accounts::AccountInput;
use ai_email_lib::db::messages::MessageInsert;
use ai_email_lib::db::{self, message_tags, Pool};
use time::OffsetDateTime;
use uuid::Uuid;

async fn temp_db() -> Pool {
    let path = std::env::temp_dir().join(format!("ai-email-test-{}.db", Uuid::new_v4()));
    db::connect(&path).await.expect("connect + migrate")
}

#[tokio::test]
async fn account_roundtrip_decodes_timestamp() {
    let pool = temp_db().await;
    let acc = db::accounts::insert(
        &pool,
        &AccountInput {
            email: "t@example.com".into(),
            display_name: Some("T".into()),
            provider: "imap".into(),
            imap_host: "imap.example.com".into(),
            imap_port: 993,
            smtp_host: "smtp.example.com".into(),
            smtp_port: 465,
        },
    )
    .await
    .expect("insert account");

    // created_at must have decoded from the strftime TEXT default into a real timestamp.
    assert!(acc.created_at.year() >= 2025);
    assert!(acc.last_synced_at.is_none());

    let all = db::accounts::list(&pool).await.expect("list");
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].id, acc.id);
}

#[tokio::test]
async fn message_arrays_and_tags_roundtrip() {
    let pool = temp_db().await;
    let acc = db::accounts::insert(
        &pool,
        &AccountInput {
            email: "m@example.com".into(),
            display_name: None,
            provider: "imap".into(),
            imap_host: "h".into(),
            imap_port: 993,
            smtp_host: "h".into(),
            smtp_port: 465,
        },
    )
    .await
    .expect("insert account");

    // Insert a mailbox directly — the repo upsert needs an imap MailboxInfo we don't build here.
    let mailbox_id = Uuid::new_v4();
    sqlx::query("INSERT INTO mailboxes (id, account_id, name) VALUES (?1, ?2, 'INBOX')")
        .bind(mailbox_id)
        .bind(acc.id)
        .execute(&pool)
        .await
        .expect("insert mailbox");

    let msg_id = db::messages::insert(
        &pool,
        &MessageInsert {
            account_id: acc.id,
            mailbox_id,
            imap_uid: 1,
            rfc_message_id: None,
            thread_id: None,
            subject: Some("hi".into()),
            from_addr: Some("a@x.com".into()),
            to_addrs: vec!["b@x.com".into(), "c@x.com".into()],
            cc_addrs: vec![],
            sent_at: Some(OffsetDateTime::now_utc()),
            internal_date: None,
            flags: vec!["\\Seen".into()],
            size_bytes: Some(42),
            has_attachment: false,
            snippet: Some("hello".into()),
            references_header: None,
        },
    )
    .await
    .expect("insert message")
    .expect("new row id");

    let list = db::messages::list_in_mailbox(&pool, mailbox_id, 50, 0)
        .await
        .expect("list");
    assert_eq!(list.len(), 1);
    let m = &list[0];
    assert_eq!(m.id, msg_id);
    assert_eq!(
        m.to_addrs,
        vec!["b@x.com".to_string(), "c@x.com".to_string()]
    ); // JSON array decoded
    assert_eq!(m.flags, vec!["\\Seen".to_string()]);
    assert!(m.cc_addrs.is_empty());
    assert!(m.tags.is_empty()); // json_group_array with no tags → []
    assert!(m.sent_at.is_some()); // timestamp decoded
    assert!(!m.has_attachment);

    // AI tags flow through json_group_array on the next list.
    message_tags::replace_ai_tags(&pool, msg_id, &["work".into(), "urgent".into()])
        .await
        .expect("replace tags");
    let list2 = db::messages::list_in_mailbox(&pool, mailbox_id, 50, 0)
        .await
        .expect("list");
    let mut tags = list2[0].tags.clone();
    tags.sort();
    assert_eq!(tags, vec!["urgent".to_string(), "work".to_string()]);
}

#[tokio::test]
async fn update_flags_overwrites_and_remove_deletes() {
    let pool = temp_db().await;
    let acc = db::accounts::insert(
        &pool,
        &AccountInput {
            email: "f@example.com".into(),
            display_name: None,
            provider: "imap".into(),
            imap_host: "h".into(),
            imap_port: 993,
            smtp_host: "h".into(),
            smtp_port: 465,
        },
    )
    .await
    .expect("insert account");

    let mailbox_id = Uuid::new_v4();
    sqlx::query("INSERT INTO mailboxes (id, account_id, name) VALUES (?1, ?2, 'INBOX')")
        .bind(mailbox_id)
        .bind(acc.id)
        .execute(&pool)
        .await
        .expect("insert mailbox");

    let id = db::messages::insert(
        &pool,
        &MessageInsert {
            account_id: acc.id,
            mailbox_id,
            imap_uid: 1,
            rfc_message_id: None,
            thread_id: None,
            subject: Some("hi".into()),
            from_addr: Some("a@x.com".into()),
            to_addrs: vec![],
            cc_addrs: vec![],
            sent_at: None,
            internal_date: None,
            flags: vec![],
            size_bytes: None,
            has_attachment: false,
            snippet: None,
            references_header: None,
        },
    )
    .await
    .expect("insert message")
    .expect("new row id");

    // update_flags 覆盖式写入
    db::messages::update_flags(&pool, id, &["\\Seen".to_string(), "\\Flagged".to_string()])
        .await
        .unwrap();
    let got = db::messages::get(&pool, id).await.unwrap().unwrap();
    assert!(got.flags.contains(&"\\Seen".to_string()));
    assert!(got.flags.contains(&"\\Flagged".to_string()));

    // remove 删除该行（bodies 等子表 CASCADE，单删即净）
    db::messages::remove(&pool, id).await.unwrap();
    assert!(db::messages::get(&pool, id).await.unwrap().is_none());
}

#[tokio::test]
async fn remove_cascades_children_and_nulls_send_log() {
    let pool = temp_db().await;
    let acc = db::accounts::insert(
        &pool,
        &AccountInput {
            email: "g@example.com".into(),
            display_name: None,
            provider: "imap".into(),
            imap_host: "h".into(),
            imap_port: 993,
            smtp_host: "h".into(),
            smtp_port: 465,
        },
    )
    .await
    .expect("insert account");

    let mailbox_id = Uuid::new_v4();
    sqlx::query("INSERT INTO mailboxes (id, account_id, name) VALUES (?1, ?2, 'INBOX')")
        .bind(mailbox_id)
        .bind(acc.id)
        .execute(&pool)
        .await
        .expect("insert mailbox");

    let id = db::messages::insert(
        &pool,
        &MessageInsert {
            account_id: acc.id,
            mailbox_id,
            imap_uid: 1,
            rfc_message_id: None,
            thread_id: None,
            subject: Some("hi".into()),
            from_addr: Some("a@x.com".into()),
            to_addrs: vec![],
            cc_addrs: vec![],
            sent_at: None,
            internal_date: None,
            flags: vec![],
            size_bytes: None,
            has_attachment: false,
            snippet: None,
            references_header: None,
        },
    )
    .await
    .expect("insert message")
    .expect("new row id");

    // 子表：message_tags（应随 messages CASCADE 删除）
    sqlx::query("INSERT INTO message_tags (message_id, tag, source) VALUES (?1, 'work', 'ai')")
        .bind(id)
        .execute(&pool)
        .await
        .expect("insert tag");

    // send_log 引用该邮件（修复后应 SET NULL，而非阻止删除）
    let send_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO send_log (id, account_id, in_reply_to, subject, ai_assisted) \
         VALUES (?1, ?2, ?3, 'Re: hi', 0)",
    )
    .bind(send_id)
    .bind(acc.id)
    .bind(id)
    .execute(&pool)
    .await
    .expect("insert send_log");

    // 删除父邮件：不应因 send_log FK 失败
    db::messages::remove(&pool, id)
        .await
        .expect("remove must not fail on replied mail");

    // messages 行没了
    assert!(db::messages::get(&pool, id).await.unwrap().is_none());

    // message_tags 级联删除
    let tag_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM message_tags WHERE message_id = ?1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(tag_count, 0, "message_tags 应随父邮件 CASCADE 删除");

    // send_log 审计行保留，in_reply_to 被置空
    let sl_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM send_log WHERE id = ?1")
        .bind(send_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(sl_count, 1, "send_log 审计行不应被删");
    let null_ref: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM send_log WHERE id = ?1 AND in_reply_to IS NULL")
            .bind(send_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(null_ref, 1, "in_reply_to 应被 SET NULL");
}

// ── #36: fetch_for_classify 分块查询，结果与单次等价 ───────────────────────────────

#[tokio::test]
async fn fetch_for_classify_chunked_returns_complete_results() {
    use ai_email_lib::db::messages::{fetch_for_classify, IN_CHUNK_SIZE};
    let pool = temp_db().await;
    let acc = db::accounts::insert(
        &pool,
        &AccountInput {
            email: "chunk@example.com".into(),
            display_name: None,
            provider: "imap".into(),
            imap_host: "h".into(),
            imap_port: 993,
            smtp_host: "h".into(),
            smtp_port: 465,
        },
    )
    .await
    .expect("insert account");

    let mailbox_id = Uuid::new_v4();
    sqlx::query("INSERT INTO mailboxes (id, account_id, name) VALUES (?1, ?2, 'INBOX')")
        .bind(mailbox_id)
        .bind(acc.id)
        .execute(&pool)
        .await
        .expect("insert mailbox");

    // 插入 IN_CHUNK_SIZE + 3 条消息，边界跨过一个批次
    let total = IN_CHUNK_SIZE + 3;
    let mut ids = Vec::with_capacity(total);
    for i in 0..total {
        let id = db::messages::insert(
            &pool,
            &MessageInsert {
                account_id: acc.id,
                mailbox_id,
                imap_uid: i as i64 + 1,
                rfc_message_id: None,
                thread_id: None,
                subject: Some(format!("subject-{i}")),
                from_addr: None,
                to_addrs: vec![],
                cc_addrs: vec![],
                sent_at: None,
                internal_date: None,
                flags: vec![],
                size_bytes: None,
                has_attachment: false,
                snippet: None,
                references_header: None,
            },
        )
        .await
        .expect("insert message")
        .expect("new row id");
        ids.push(id);
    }

    let result = fetch_for_classify(&pool, &ids).await.expect("fetch");
    assert_eq!(
        result.len(),
        total,
        "分块查询应返回与原始列表等量的结果（跨批次边界）"
    );
    let mut result_ids: Vec<Uuid> = result.iter().map(|r| r.id).collect();
    result_ids.sort();
    let mut expected_ids = ids.clone();
    expected_ids.sort();
    assert_eq!(
        result_ids, expected_ids,
        "分块查询返回的 id 集合应与输入完全一致"
    );
}

// ── #38: ai_results ON CONFLICT 应刷新 model/tokens，不只刷 output ────────────────

#[tokio::test]
async fn ai_results_conflict_update_refreshes_model_and_tokens() {
    use ai_email_lib::db::ai_results::{self, AiResultInsert};
    let pool = temp_db().await;
    let acc = db::accounts::insert(
        &pool,
        &AccountInput {
            email: "airesult@example.com".into(),
            display_name: None,
            provider: "imap".into(),
            imap_host: "h".into(),
            imap_port: 993,
            smtp_host: "h".into(),
            smtp_port: 465,
        },
    )
    .await
    .expect("insert account");

    let mailbox_id = Uuid::new_v4();
    sqlx::query("INSERT INTO mailboxes (id, account_id, name) VALUES (?1, ?2, 'INBOX')")
        .bind(mailbox_id)
        .bind(acc.id)
        .execute(&pool)
        .await
        .expect("insert mailbox");

    let msg_id = db::messages::insert(
        &pool,
        &MessageInsert {
            account_id: acc.id,
            mailbox_id,
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
            references_header: None,
        },
    )
    .await
    .expect("insert message")
    .expect("new row id");

    // 首次插入
    let first = ai_results::insert(
        &pool,
        &AiResultInsert {
            message_id: msg_id,
            kind: "classify".into(),
            model: "haiku-old".into(),
            prompt_hash: "hash-abc".into(),
            output: serde_json::json!({"category":"work"}),
            input_tokens: Some(100),
            output_tokens: Some(50),
            cache_read_tokens: None,
        },
    )
    .await
    .expect("first insert");
    assert_eq!(first.model, "haiku-old");
    assert_eq!(first.input_tokens, Some(100));

    // 相同 (message_id, kind, prompt_hash)，触发 ON CONFLICT，更新所有字段
    let second = ai_results::insert(
        &pool,
        &AiResultInsert {
            message_id: msg_id,
            kind: "classify".into(),
            model: "sonnet-new".into(),
            prompt_hash: "hash-abc".into(),
            output: serde_json::json!({"category":"personal"}),
            input_tokens: Some(200),
            output_tokens: Some(80),
            cache_read_tokens: Some(10),
        },
    )
    .await
    .expect("second insert (conflict)");

    // model 和 tokens 应被刷新为新值
    assert_eq!(second.model, "sonnet-new", "冲突更新后 model 应刷新");
    assert_eq!(second.input_tokens, Some(200), "input_tokens 应刷新");
    assert_eq!(second.output_tokens, Some(80), "output_tokens 应刷新");
    assert_eq!(
        second.cache_read_tokens,
        Some(10),
        "cache_read_tokens 应刷新"
    );
    let out: serde_json::Value = second.output.clone();
    assert_eq!(out["category"], "personal", "output 应刷新为新值");
}

// ── #74: connect() 应设置 synchronous=NORMAL ──────────────────────────────────────

#[tokio::test]
async fn connect_sets_synchronous_normal() {
    let pool = temp_db().await;
    // PRAGMA synchronous 返回: 0=OFF 1=NORMAL 2=FULL 3=EXTRA
    let val: i32 = sqlx::query_scalar("PRAGMA synchronous")
        .fetch_one(&pool)
        .await
        .expect("query pragma");
    assert_eq!(
        val, 1,
        "WAL 模式下应设置 synchronous=NORMAL (1)，当前值: {val}"
    );
    // NORMAL 必须配在 WAL 模式下才安全；断言 journal_mode=wal 确认前提成立。
    let mode: String = sqlx::query_scalar("PRAGMA journal_mode")
        .fetch_one(&pool)
        .await
        .expect("query journal_mode");
    assert_eq!(
        mode, "wal",
        "synchronous=NORMAL 仅在 WAL 模式下安全，当前 journal_mode: {mode}"
    );
}

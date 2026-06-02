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

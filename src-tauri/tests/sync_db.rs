//! DB-layer integration tests for IMAP-sync bookkeeping correctness (audit Phase 2):
//!   - #9  update_after_sync writes the new uid_validity (no silent COALESCE masking)
//!   - #73 update_after_sync never lets uid_next regress under interleaved syncs (MAX guard)
//!   - #2  reset_mailbox_for_uidvalidity_change drops stale UID rows + resets uid_next
//!   - #27 get_by_name matches "INBOX" case-insensitively (write/read agree on the row)
//!   - #64 `messages::update_flags_by_uid` 按 (mailbox_id, imap_uid) 刷新 flags，并隔离跨 mailbox 的同名 UID
//!     （见 update_flags_by_uid_updates_correct_mailbox）
//!
//! All round-trips run against a real migrated on-disk SQLite — never QQ Mail.

use ai_email_lib::db::messages::MessageInsert;
use ai_email_lib::db::{self, accounts::AccountInput, mailboxes, messages, Pool};
use uuid::Uuid;

async fn temp_db() -> Pool {
    let path = std::env::temp_dir().join(format!("ai-email-syncdb-{}.db", Uuid::new_v4()));
    db::connect(&path).await.expect("connect + migrate")
}

async fn seed_account(pool: &Pool, email: &str) -> Uuid {
    db::accounts::insert(
        pool,
        &AccountInput {
            email: email.into(),
            display_name: None,
            provider: "imap".into(),
            imap_host: "h".into(),
            imap_port: 993,
            smtp_host: "h".into(),
            smtp_port: 465,
        },
    )
    .await
    .expect("insert account")
    .id
}

/// Insert a mailbox row with an explicit name (the repo upsert builds its own UUID).
async fn seed_mailbox(pool: &Pool, account_id: Uuid, name: &str) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO mailboxes (id, account_id, name) VALUES (?1, ?2, ?3)")
        .bind(id)
        .bind(account_id)
        .bind(name)
        .execute(pool)
        .await
        .expect("insert mailbox");
    id
}

async fn seed_message(pool: &Pool, account_id: Uuid, mailbox_id: Uuid, uid: i64) -> Uuid {
    messages::insert(
        pool,
        &MessageInsert {
            account_id,
            mailbox_id,
            imap_uid: uid,
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
        },
    )
    .await
    .expect("insert message")
    .expect("new row id")
}

// ---- #9: new uid_validity is persisted, not silently kept ----

#[tokio::test]
async fn update_after_sync_persists_new_uid_validity() {
    let pool = temp_db().await;
    let acc = seed_account(&pool, "v@example.com").await;
    let mb = seed_mailbox(&pool, acc, "INBOX").await;

    // First sync stores validity = 100, uid_next = 50.
    mailboxes::update_after_sync(&pool, mb, Some(50), Some(100))
        .await
        .unwrap();
    let row = mailboxes::get(&pool, mb).await.unwrap().unwrap();
    assert_eq!(row.uid_validity, Some(100));
    assert_eq!(row.uid_next, Some(50));

    // Server reports a *different* validity (mailbox rebuilt): it must land, not be masked.
    mailboxes::update_after_sync(&pool, mb, Some(60), Some(999))
        .await
        .unwrap();
    let row = mailboxes::get(&pool, mb).await.unwrap().unwrap();
    assert_eq!(
        row.uid_validity,
        Some(999),
        "new uid_validity must overwrite the old one"
    );
}

#[tokio::test]
async fn update_after_sync_keeps_validity_when_server_omits_it() {
    // Minimal SELECT response (validity = None) must not wipe a known validity.
    let pool = temp_db().await;
    let acc = seed_account(&pool, "vn@example.com").await;
    let mb = seed_mailbox(&pool, acc, "INBOX").await;

    mailboxes::update_after_sync(&pool, mb, Some(50), Some(100))
        .await
        .unwrap();
    mailboxes::update_after_sync(&pool, mb, Some(60), None)
        .await
        .unwrap();
    let row = mailboxes::get(&pool, mb).await.unwrap().unwrap();
    assert_eq!(
        row.uid_validity,
        Some(100),
        "None must preserve prior value"
    );
    assert_eq!(row.uid_next, Some(60));
}

// ---- #73: uid_next must never regress under interleaved syncs ----

#[tokio::test]
async fn update_after_sync_never_regresses_uid_next() {
    let pool = temp_db().await;
    let acc = seed_account(&pool, "toctou@example.com").await;
    let mb = seed_mailbox(&pool, acc, "INBOX").await;

    // Sync A (started first, finished first) writes the larger, newer uid_next.
    mailboxes::update_after_sync(&pool, mb, Some(200), Some(1))
        .await
        .unwrap();
    // Sync B started earlier with a stale snapshot and finishes last with a SMALLER uid_next.
    // The guard must keep the max so we don't re-fetch an already-seen range forever.
    mailboxes::update_after_sync(&pool, mb, Some(150), Some(1))
        .await
        .unwrap();

    let row = mailboxes::get(&pool, mb).await.unwrap().unwrap();
    assert_eq!(
        row.uid_next,
        Some(200),
        "uid_next must not regress when a stale sync finishes last"
    );
}

#[tokio::test]
async fn update_after_sync_advances_uid_next_forward() {
    let pool = temp_db().await;
    let acc = seed_account(&pool, "fwd@example.com").await;
    let mb = seed_mailbox(&pool, acc, "INBOX").await;

    mailboxes::update_after_sync(&pool, mb, Some(100), Some(1))
        .await
        .unwrap();
    mailboxes::update_after_sync(&pool, mb, Some(250), Some(1))
        .await
        .unwrap();
    let row = mailboxes::get(&pool, mb).await.unwrap().unwrap();
    assert_eq!(row.uid_next, Some(250), "forward progress still applies");
}

// ---- #2: UIDVALIDITY-change reset drops stale rows and resets uid_next ----

#[tokio::test]
async fn reset_for_uidvalidity_change_clears_rows_and_uid_next() {
    let pool = temp_db().await;
    let acc = seed_account(&pool, "reset@example.com").await;
    let mb = seed_mailbox(&pool, acc, "INBOX").await;
    let other_mb = seed_mailbox(&pool, acc, "Sent").await;

    // Old generation: validity 100, uid_next 50, two cached messages.
    mailboxes::update_after_sync(&pool, mb, Some(50), Some(100))
        .await
        .unwrap();
    seed_message(&pool, acc, mb, 10).await;
    seed_message(&pool, acc, mb, 20).await;
    // A message in a different mailbox must survive the reset.
    let keep = seed_message(&pool, acc, other_mb, 5).await;

    // Server now reports validity 777 — mailbox was rebuilt. Reset.
    mailboxes::reset_mailbox_for_uidvalidity_change(&pool, mb, 777)
        .await
        .unwrap();

    let row = mailboxes::get(&pool, mb).await.unwrap().unwrap();
    assert_eq!(row.uid_validity, Some(777), "new validity stored");
    assert_eq!(
        row.uid_next, None,
        "uid_next reset to NULL → next sync takes the first-sync path"
    );

    let remaining = messages::list_in_mailbox(&pool, mb, 100, 0).await.unwrap();
    assert!(
        remaining.is_empty(),
        "stale UID rows for this mailbox dropped"
    );

    // Other mailbox untouched.
    assert!(messages::get(&pool, keep).await.unwrap().is_some());

    // After reset, a fresh uid_next from the new generation lands cleanly (no MAX vs stale-50).
    mailboxes::update_after_sync(&pool, mb, Some(3), Some(777))
        .await
        .unwrap();
    let row = mailboxes::get(&pool, mb).await.unwrap().unwrap();
    assert_eq!(
        row.uid_next,
        Some(3),
        "post-reset the smaller new-generation uid_next must apply (uid_next was NULL)"
    );
}

// ---- #64: update_flags_by_uid patches the right row and is scoped to its mailbox ----

#[tokio::test]
async fn update_flags_by_uid_updates_correct_mailbox() {
    let pool = temp_db().await;
    let acc = seed_account(&pool, "flags@example.com").await;
    let mb_a = seed_mailbox(&pool, acc, "INBOX").await;
    let mb_b = seed_mailbox(&pool, acc, "Archive").await;

    // Same UID (42) exists in both mailboxes.
    let id_a = seed_message(&pool, acc, mb_a, 42).await;
    let id_b = seed_message(&pool, acc, mb_b, 42).await;

    // Positive: update mb_a UID 42 → \Seen should appear in mb_a.
    messages::update_flags_by_uid(&pool, mb_a, 42, &["\\Seen".to_string()])
        .await
        .unwrap();

    let row_a = messages::get(&pool, id_a).await.unwrap().unwrap();
    assert!(
        row_a.flags.iter().any(|f| f == "\\Seen"),
        "mb_a UID 42 must carry \\Seen after update"
    );

    // Negative (mailbox isolation): mb_b's UID 42 must be unchanged (still empty).
    let row_b = messages::get(&pool, id_b).await.unwrap().unwrap();
    assert!(
        row_b.flags.is_empty(),
        "mb_b UID 42 must not be touched by an update scoped to mb_a"
    );
}

// ---- #27: get_by_name is case-insensitive so write & read hit the same row ----

#[tokio::test]
async fn get_by_name_matches_inbox_case_insensitively() {
    let pool = temp_db().await;
    let acc = seed_account(&pool, "case@example.com").await;
    // Server advertised the mailbox as lowercase "Inbox".
    seed_mailbox(&pool, acc, "Inbox").await;

    // Sync looks it up with the hardcoded uppercase literal "INBOX".
    let found = mailboxes::get_by_name(&pool, acc, "INBOX").await.unwrap();
    assert!(
        found.is_some(),
        "get_by_name must match 'Inbox' when queried with 'INBOX'"
    );
    assert_eq!(found.unwrap().name, "Inbox");
}

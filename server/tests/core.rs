use ai_email_server::{
    api,
    config::Config,
    db::Database,
    mail::*,
    model::{now, SearchQuery, RETENTION_SECONDS},
    operations::PrepareSend,
    Service,
};
use async_trait::async_trait;
use axum::{body::Body, http::Request};
use http_body_util::BodyExt;
use serde_json::{json, Value};
use std::sync::{
    atomic::{AtomicU32, AtomicUsize, Ordering},
    Arc,
};
use tower::ServiceExt;
#[derive(Default)]
struct Fake {
    fail: AtomicU32,
    validity: AtomicU32,
    sends: AtomicUsize,
    outgoing: std::sync::Mutex<Vec<(String, OutgoingMail)>>,
    moves: AtomicUsize,
    attachments: AtomicUsize,
}
#[async_trait]
impl MailTransport for Fake {
    async fn snapshot(&self, _: &MailAccount, _: &str, _: i64) -> Result<MailboxState, MailError> {
        if self.fail.load(Ordering::SeqCst) == u32::MAX {
            std::future::pending::<()>().await;
        }
        Ok(MailboxState {
            uid_validity: self.validity.load(Ordering::SeqCst) + 1,
            uid_next: 4,
            uids: vec![1, 2, 3],
        })
    }
    async fn fetch(
        &self,
        _: &MailAccount,
        _: &str,
        _: u32,
        uid: u32,
    ) -> Result<MailMessage, MailError> {
        if self.fail.load(Ordering::SeqCst) == uid {
            Err(MailError::Unavailable("SECRET upstream failure".into()))
        } else {
            Ok(mail(uid, "正文证书 100%_到期", now()))
        }
    }
    async fn set_flags(
        &self,
        _: &MailAccount,
        _: &str,
        _: u32,
        _: u32,
        _: Option<bool>,
        _: Option<bool>,
    ) -> Result<(), MailError> {
        Ok(())
    }
    async fn move_message(
        &self,
        _: &MailAccount,
        _: &str,
        _: u32,
        _: u32,
        _: &str,
    ) -> Result<(), MailError> {
        self.moves.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
    async fn attachment(
        &self,
        _: &MailAccount,
        _: &str,
        _: u32,
        _: u32,
        _: usize,
    ) -> Result<AttachmentData, MailError> {
        self.attachments.fetch_add(1, Ordering::SeqCst);
        Err(MailError::NotFound)
    }
    async fn send(
        &self,
        account: &MailAccount,
        mail: &OutgoingMail,
    ) -> Result<SendResult, MailError> {
        self.outgoing
            .lock()
            .unwrap()
            .push((account.id.clone(), mail.clone()));
        self.sends.fetch_add(1, Ordering::SeqCst);
        Err(MailError::SendUnknown("SECRET smtp unknown".into()))
    }
}
fn mail(uid: u32, body: &str, date: i64) -> MailMessage {
    MailMessage {
        uid,
        received_at: date,
        subject: "证书通知".into(),
        from_address: "vendor@example.com".into(),
        to_addresses: vec!["me@example.com".into()],
        cc_addresses: vec!["copy@example.com".into()],
        reply_to_addresses: vec![],
        body_text: body.into(),
        message_id: Some(format!("<{uid}@example.com>")),
        references: vec![],
        flags: vec![],
        attachments: vec![],
    }
}
struct Fixture {
    _temp: tempfile::TempDir,
    config: Config,
    fake: Arc<Fake>,
}
impl Fixture {
    fn new(history: bool) -> Self {
        let temp = tempfile::tempdir().unwrap();
        for (file, secret) in [
            ("read", "r".repeat(32)),
            ("write", "w".repeat(32)),
            ("password", "secret".into()),
        ] {
            std::fs::write(temp.path().join(file), secret).unwrap();
        }
        let account = |id: &str| json!({"id":id,"email":format!("{id}@example.com"),"display_name":id,"imap_host":"imap.example.com","smtp_host":"smtp.example.com","password":{"file":temp.path().join("password")},"folders":["INBOX"]});
        let config=serde_json::from_value(json!({"database_path":temp.path().join("mail.sqlite"),"read_token":{"file":temp.path().join("read")},"write_token":{"file":temp.path().join("write")},"import_history":history,"accounts":[account("a"),account("b")]})).unwrap();
        Self {
            _temp: temp,
            config,
            fake: Arc::new(Fake::default()),
        }
    }
    async fn service(&self) -> Arc<Service> {
        Service::new(self.config.clone(), self.fake.clone())
            .await
            .unwrap()
    }
}
#[tokio::test]
async fn chinese_body_literal_wildcards_and_pagination() {
    let db = Database::open(":memory:").await.unwrap();
    db.put(
        "a",
        "a@example.com",
        "INBOX",
        1,
        mail(1, "中文证书 100%_完成", now()),
    )
    .await
    .unwrap();
    db.put(
        "b",
        "b@example.com",
        "INBOX",
        1,
        mail(1, "中文证书 100XX完成", now()),
    )
    .await
    .unwrap();
    let query = |q: &str, offset| SearchQuery {
        q: q.into(),
        account_id: None,
        limit: Some(1),
        offset: Some(offset),
    };
    let r = db.search(query("中文证书 100%_", 0)).await.unwrap();
    assert_eq!(r.total, 1);
    assert_eq!(r.items[0].account_id, "a");
    assert_eq!(
        db.search(query("中文证书 missing", 0)).await.unwrap().total,
        0
    );
    assert_eq!(
        db.search(query("copy@example.com", 0)).await.unwrap().total,
        2
    );
    let r = db.search(query("", 99)).await.unwrap();
    assert_eq!(r.total, 2);
    assert!(r.items.is_empty());
    assert_eq!(db.search(query("' OR 1=1 --", 0)).await.unwrap().total, 0);
}
#[tokio::test]
async fn cleanup_only_old_local_rows_and_account_uid_isolation() {
    let db = Database::open(":memory:").await.unwrap();
    db.put(
        "a",
        "a@example.com",
        "INBOX",
        1,
        mail(1, "old", now() - RETENTION_SECONDS - 10),
    )
    .await
    .unwrap();
    db.put("b", "b@example.com", "INBOX", 1, mail(1, "new", now()))
        .await
        .unwrap();
    assert_eq!(db.cleanup(now()).await.unwrap(), 1);
    let r = db.search(SearchQuery::default()).await.unwrap();
    assert_eq!(r.total, 1);
    assert_eq!(r.items[0].account_id, "b");
}
#[tokio::test]
async fn sync_failure_does_not_advance_cursor_or_publish_partial_mail() {
    let f = Fixture::new(true);
    let s = f.service().await;
    f.fake.fail.store(2, Ordering::SeqCst);
    assert!(s.sync_account("a").await.is_err());
    assert_eq!(s.db.cursor("a", "INBOX").await.unwrap(), Some((1, 1)));
    assert_eq!(s.db.search(SearchQuery::default()).await.unwrap().total, 1);
    f.fake.fail.store(0, Ordering::SeqCst);
    s.sync_account("a").await.unwrap();
    s.sync_account("b").await.unwrap();
    assert_eq!(s.db.search(SearchQuery::default()).await.unwrap().total, 6);
    f.fake.validity.store(1, Ordering::SeqCst);
    s.sync_account("a").await.unwrap();
    assert_eq!(s.db.cursor("a", "INBOX").await.unwrap(), Some((2, 3)));
    assert_eq!(s.db.search(SearchQuery::default()).await.unwrap().total, 6);
}
#[tokio::test]
async fn fresh_snapshot_defaults_to_no_historical_import() {
    let f = Fixture::new(false);
    let s = f.service().await;
    s.sync_account("a").await.unwrap();
    assert_eq!(s.db.cursor("a", "INBOX").await.unwrap(), Some((1, 3)));
    assert_eq!(s.db.search(SearchQuery::default()).await.unwrap().total, 0);
}
#[test]
fn rejects_more_than_ten_accounts() {
    let mut f = Fixture::new(false);
    f.config.accounts = (0..11)
        .map(|i| {
            let mut a = f.config.accounts[0].clone();
            a.id = i.to_string();
            a
        })
        .collect();
    assert!(f.config.validate().is_err());
}
fn outgoing(id: &str) -> PrepareSend {
    PrepareSend {
        operation_id: id.into(),
        account_id: "a".into(),
        to: vec!["target@example.com".into()],
        cc: vec![],
        subject: "hello".into(),
        body_text: "send content".into(),
        reply_to_id: None,
    }
}
#[tokio::test]
async fn smtp_unknown_is_never_resent_and_different_payload_conflicts() {
    let f = Fixture::new(false);
    let s = f.service().await;
    s.prepare_send(outgoing("op1")).await.unwrap();
    let (a, b) = tokio::join!(s.send_prepared("op1"), s.send_prepared("op1"));
    assert_eq!(a.unwrap().status, "unknown");
    assert_eq!(b.unwrap().status, "unknown");
    assert_eq!(f.fake.sends.load(Ordering::SeqCst), 1);
    let mut other = outgoing("op1");
    other.body_text = "different".into();
    assert!(s.prepare_send(other).await.is_err());
    assert_eq!(
        s.prepare_send(outgoing("op1")).await.unwrap().status,
        "unknown"
    );
    s.send_prepared("op1").await.unwrap();
    assert_eq!(f.fake.sends.load(Ordering::SeqCst), 1);
}
#[tokio::test]
async fn crashed_sending_operation_becomes_unknown_on_restart() {
    let f = Fixture::new(false);
    let s = f.service().await;
    s.prepare_send(outgoing("crash")).await.unwrap();
    sqlx::query("UPDATE send_operations SET status='sending' WHERE id='crash'")
        .execute(&s.db.pool)
        .await
        .unwrap();
    s.db.pool.close().await;
    drop(s);
    let s = f.service().await;
    assert_eq!(s.send_prepared("crash").await.unwrap().status, "unknown");
    assert_eq!(f.fake.sends.load(Ordering::SeqCst), 0);
}
async fn http(
    app: axum::Router,
    method: &str,
    path: &str,
    token: Option<&str>,
    origin: Option<&str>,
    body: Value,
) -> (u16, Value) {
    let mut req = Request::builder()
        .method(method)
        .uri(path)
        .header("content-type", "application/json")
        .header("accept", "application/json, text/event-stream")
        .header("host", "localhost");
    if let Some(token) = token {
        req = req.header("authorization", format!("Bearer {token}"));
    }
    if let Some(origin) = origin {
        req = req.header("origin", origin);
    }
    let response = app
        .oneshot(req.body(Body::from(body.to_string())).unwrap())
        .await
        .unwrap();
    let status = response.status().as_u16();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}
#[tokio::test]
async fn http_auth_origin_write_permissions_and_no_body_in_search() {
    let f = Fixture::new(true);
    let s = f.service().await;
    s.sync_account("a").await.unwrap();
    let app = api::router(s).unwrap();
    let read = "r".repeat(32);
    assert_eq!(
        http(app.clone(), "GET", "/api/accounts", None, None, json!(null))
            .await
            .0,
        401
    );
    assert_eq!(
        http(
            app.clone(),
            "GET",
            "/api/accounts",
            Some(&read),
            Some("https://evil.invalid"),
            json!(null)
        )
        .await
        .0,
        403
    );
    assert_eq!(
        http(
            app.clone(),
            "POST",
            "/api/send/prepare",
            Some(&read),
            None,
            serde_json::to_value(outgoing("op")).unwrap()
        )
        .await
        .0,
        403
    );
    let (code, body) = http(
        app.clone(),
        "GET",
        "/api/messages",
        Some(&read),
        None,
        json!(null),
    )
    .await;
    assert_eq!(code, 200);
    assert!(body["items"][0].get("body_text").is_none());
    assert_eq!(body["sync"].as_array().unwrap().len(), 2);
    let (code, body) = http(
        app,
        "GET",
        "/api/messages?limit=0",
        Some(&read),
        None,
        json!(null),
    )
    .await;
    assert_eq!(code, 400);
    assert_eq!(body["error"]["code"], "invalid_request");
}
#[tokio::test]
async fn mcp_standard_initialize_and_read_token_cannot_send() {
    let f = Fixture::new(false);
    let s = f.service().await;
    let app = api::router(s).unwrap();
    let read = "r".repeat(32);
    let (code,body)=http(app.clone(),"POST","/mcp",Some(&read),None,json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"test","version":"1"}}})).await;
    assert_eq!(code, 200);
    assert!(
        body["result"]["capabilities"]["tools"].is_object(),
        "{body}"
    );
    let (code,body)=http(app,"POST","/mcp",Some(&read),None,json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"prepare_send","arguments":serde_json::to_value(outgoing("mcp-op")).unwrap()}})).await;
    assert_eq!(code, 200, "{body}");
    assert_eq!(body["result"]["isError"], true, "{body}");
    assert!(body.to_string().contains("unauthorized"));
}
#[tokio::test]
async fn terminal_send_clears_body_and_retains_idempotency_tombstone() {
    let f = Fixture::new(false);
    let s = f.service().await;
    s.prepare_send(outgoing("terminal")).await.unwrap();
    s.send_prepared("terminal").await.unwrap();
    let (payload,): (String,) =
        sqlx::query_as("SELECT payload FROM send_operations WHERE id='terminal'")
            .fetch_one(&s.db.pool)
            .await
            .unwrap();
    assert!(payload.is_empty());
    assert_eq!(
        s.prepare_send(outgoing("terminal")).await.unwrap().status,
        "unknown"
    );
    s.prepare_send(outgoing("expired")).await.unwrap();
    s.db.cleanup(now() + RETENTION_SECONDS + 1).await.unwrap();
    assert_eq!(s.send_prepared("expired").await.unwrap().status, "expired");
    assert_eq!(
        s.prepare_send(outgoing("expired")).await.unwrap().status,
        "expired"
    );
    assert_eq!(f.fake.sends.load(Ordering::SeqCst), 1);
}
#[tokio::test]
async fn replying_from_wrong_account_is_rejected_before_smtp() {
    let f = Fixture::new(true);
    let s = f.service().await;
    s.sync_account("b").await.unwrap();
    let parent =
        s.db.search(SearchQuery::default())
            .await
            .unwrap()
            .items
            .remove(0);
    let mut outgoing = outgoing("badreply");
    outgoing.reply_to_id = Some(parent.id);
    assert!(s.prepare_send(outgoing).await.is_err());
    assert_eq!(f.fake.sends.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn shutdown_interrupts_stalled_background_sync() {
    let f = Fixture::new(false);
    f.fake.fail.store(u32::MAX, Ordering::SeqCst);
    let s = f.service().await;
    let cancel = tokio_util::sync::CancellationToken::new();
    let task = tokio::spawn(s.run_sync(cancel.clone()));
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    cancel.cancel();
    tokio::time::timeout(std::time::Duration::from_millis(100), task)
        .await
        .unwrap()
        .unwrap();
}

async fn wait_for_account_waiters(guard: &tokio::sync::OwnedMutexGuard<()>, expected: usize) {
    // Each queued operation owns one Arc clone after its initial database read.
    let mutex = tokio::sync::OwnedMutexGuard::mutex(guard);
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        while Arc::strong_count(mutex) < 2 + expected {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
}
#[tokio::test]
async fn concurrent_flag_changes_preserve_both_updates() {
    use ai_email_server::operations::FlagChange;
    let f = Fixture::new(true);
    let s = f.service().await;
    s.sync_account("a").await.unwrap();
    let id =
        s.db.search(SearchQuery::default())
            .await
            .unwrap()
            .items
            .remove(0)
            .id;
    let guard = s.account_lock("a").await;
    let first = tokio::spawn({
        let s = s.clone();
        let id = id.clone();
        async move {
            s.set_flags(
                &id,
                FlagChange {
                    seen: Some(true),
                    flagged: None,
                },
            )
            .await
        }
    });
    let second = tokio::spawn({
        let s = s.clone();
        let id = id.clone();
        async move {
            s.set_flags(
                &id,
                FlagChange {
                    seen: None,
                    flagged: Some(true),
                },
            )
            .await
        }
    });
    wait_for_account_waiters(&guard, 2).await;
    drop(guard);
    first.await.unwrap().unwrap();
    second.await.unwrap().unwrap();
    let flags = s.db.message(&id).await.unwrap().flags;
    assert!(
        flags.contains(&"\\Seen".into()) && flags.contains(&"\\Flagged".into()),
        "{flags:?}"
    );
}
#[tokio::test]
async fn waiting_operations_recheck_message_after_account_lock() {
    let f = Fixture::new(true);
    let s = f.service().await;
    s.sync_account("a").await.unwrap();
    let id =
        s.db.search(SearchQuery::default())
            .await
            .unwrap()
            .items
            .remove(0)
            .id;
    let guard = s.account_lock("a").await;
    let moving = tokio::spawn({
        let s = s.clone();
        let id = id.clone();
        async move { s.move_message(&id, "Archive").await }
    });
    let attachment = tokio::spawn({
        let s = s.clone();
        let id = id.clone();
        async move { s.attachment(&id, 0).await }
    });
    wait_for_account_waiters(&guard, 2).await;
    s.db.remove(&id).await.unwrap();
    drop(guard);
    assert!(moving.await.unwrap().is_err());
    assert!(attachment.await.unwrap().is_err());
    assert_eq!(f.fake.moves.load(Ordering::SeqCst), 0);
    assert_eq!(f.fake.attachments.load(Ordering::SeqCst), 0);
}
#[tokio::test]
async fn reply_prepare_derives_recipient_account_preview_and_is_idempotent() {
    let f = Fixture::new(true);
    let s = f.service().await;
    s.sync_account("b").await.unwrap();
    let msg =
        s.db.search(SearchQuery::default())
            .await
            .unwrap()
            .items
            .remove(0);
    let mut data = serde_json::to_value(s.db.message(&msg.id).await.unwrap()).unwrap();
    data["reply_to_addresses"] = json!(["reply-desk@example.com"]);
    sqlx::query("UPDATE messages SET data=? WHERE id=?")
        .bind(data.to_string())
        .bind(&msg.id)
        .execute(&s.db.pool)
        .await
        .unwrap();
    let app = api::router(s.clone()).unwrap();
    let read = "r".repeat(32);
    let write = "w".repeat(32);
    for (token, can_write) in [(&read, false), (&write, true)] {
        let (code, value) = http(
            app.clone(),
            "GET",
            "/api/accounts",
            Some(token),
            None,
            json!(null),
        )
        .await;
        assert_eq!(code, 200);
        assert_eq!(value["can_write"], can_write);
    }
    let path = format!("/api/messages/{}/reply/prepare", msg.id);
    let body = json!({"operation_id":"reply-one","body_text":"谢谢，已收到。"});
    assert_eq!(
        http(app.clone(), "POST", &path, Some(&read), None, body.clone())
            .await
            .0,
        403
    );
    let (code, preview) = http(app.clone(), "POST", &path, Some(&write), None, body.clone()).await;
    assert_eq!(code, 200, "{preview}");
    assert_eq!(preview["account_id"], "b");
    assert_eq!(preview["account_email"], "b@example.com");
    assert_eq!(preview["to"], json!(["reply-desk@example.com"]));
    assert_eq!(preview["subject"], "Re: 证书通知");
    assert_eq!(preview["body_text"], "谢谢，已收到。");
    assert_eq!(preview["status"], "prepared");
    assert_eq!(f.fake.sends.load(Ordering::SeqCst), 0);
    let (_, replay) = http(app.clone(), "POST", &path, Some(&write), None, body.clone()).await;
    assert_eq!(replay, preview);
    assert_eq!(
        http(
            app.clone(),
            "POST",
            &path,
            Some(&write),
            None,
            json!({"operation_id":"reply-one","body_text":"different"})
        )
        .await
        .0,
        409
    );
    assert_eq!(
        http(
            app.clone(),
            "POST",
            &path,
            Some(&write),
            None,
            json!({"operation_id":"blank","body_text":" \n\t"})
        )
        .await
        .0,
        400
    );
    assert_eq!(
        http(
            app.clone(),
            "POST",
            &path,
            Some(&write),
            None,
            json!({"operation_id":"inject","body_text":"text","account_id":"a"})
        )
        .await
        .0,
        400
    );
    s.send_prepared("reply-one").await.unwrap();
    s.send_prepared("reply-one").await.unwrap();
    assert_eq!(f.fake.sends.load(Ordering::SeqCst), 1);
    {
        let outgoing = f.fake.outgoing.lock().unwrap();
        assert_eq!(outgoing[0].0, "b");
        assert_eq!(outgoing[0].1.to, ["reply-desk@example.com"]);
        assert!(outgoing[0].1.cc.is_empty());
        assert_eq!(
            outgoing[0].1.in_reply_to,
            data["message_id"].as_str().map(str::to_owned)
        );
        assert_eq!(
            outgoing[0].1.references,
            vec![data["message_id"].as_str().unwrap()]
        );
        assert_eq!(outgoing[0].1.body_text, "谢谢，已收到。");
    }
    let (_, replay) = http(app, "POST", &path, Some(&write), None, body).await;
    assert_eq!(replay["status"], "unknown");
    assert_eq!(replay["message_id"], preview["message_id"]);
}
#[tokio::test]
async fn reply_prepare_falls_back_to_from_and_avoids_duplicate_re_prefix() {
    let f = Fixture::new(false);
    let s = f.service().await;
    let mut mail = mail(77, "body", now());
    mail.subject = "rE: Existing".into();
    s.db.put("a", "a@example.com", "INBOX", 1, mail)
        .await
        .unwrap();
    let msg =
        s.db.search(SearchQuery::default())
            .await
            .unwrap()
            .items
            .remove(0);
    let app = api::router(s).unwrap();
    let write = "w".repeat(32);
    let (code, preview) = http(
        app,
        "POST",
        &format!("/api/messages/{}/reply/prepare", msg.id),
        Some(&write),
        None,
        json!({"operation_id":"fallback","body_text":"Reply"}),
    )
    .await;
    assert_eq!(code, 200, "{preview}");
    assert_eq!(preview["to"], json!(["vendor@example.com"]));
    assert_eq!(preview["subject"], "rE: Existing");
}

#[tokio::test]
async fn old_cache_without_reply_to_remains_readable() {
    let f = Fixture::new(true);
    let s = f.service().await;
    s.sync_account("a").await.unwrap();
    let id =
        s.db.search(SearchQuery::default())
            .await
            .unwrap()
            .items
            .remove(0)
            .id;
    let mut data = serde_json::to_value(s.db.message(&id).await.unwrap()).unwrap();
    data.as_object_mut().unwrap().remove("reply_to_addresses");
    sqlx::query("UPDATE messages SET data=? WHERE id=?")
        .bind(data.to_string())
        .bind(&id)
        .execute(&s.db.pool)
        .await
        .unwrap();
    assert!(s
        .db
        .message(&id)
        .await
        .unwrap()
        .reply_to_addresses
        .is_empty());
}
#[tokio::test]
async fn mcp_list_accounts_exposes_current_request_write_permission() {
    let f = Fixture::new(false);
    let s = f.service().await;
    let app = api::router(s).unwrap();
    for (token, can_write) in [("r".repeat(32), false), ("w".repeat(32), true)] {
        let (code,body)=http(app.clone(),"POST","/mcp",Some(&token),None,json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"list_accounts","arguments":{}}})).await;
        assert_eq!(code, 200, "{body}");
        let value: Value =
            serde_json::from_str(body["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
        assert_eq!(value["can_write"], can_write);
    }
}

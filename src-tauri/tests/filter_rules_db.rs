// 验证 0010 迁移生效：filter_rules 表 + UNIQUE 约束 + messages.filter_disabled 列。
// in-memory pool（foreign_keys 必须开）。max_connections(1)：每个 `:memory:` 连接是独立空库，
// 限单连接才能让迁移后的表对后续查询可见。

use ai_email_lib::ai::extract::Action;
use ai_email_lib::db;
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

// ── CRUD + resolve_for ──────────────────────────────────────────────────────────

fn input(
    scope: &str,
    scope_value: &str,
    target: &str,
    action: &str,
    pattern: Option<&str>,
) -> db::filter_rules::FilterRuleInput {
    db::filter_rules::FilterRuleInput {
        scope: scope.into(),
        scope_value: scope_value.into(),
        target: target.into(),
        action: action.into(),
        pattern: pattern.map(|s| s.into()),
        enabled: true,
        note: None,
    }
}

#[tokio::test]
async fn crud_roundtrip() {
    let pool = mem_pool().await;
    let r = db::filter_rules::insert(
        &pool,
        &input("domain", "cnssl.cn", "signature", "strip", Some("免责声明")),
    )
    .await
    .unwrap();
    assert_eq!(r.scope, "domain");
    assert_eq!(r.pattern.as_deref(), Some("免责声明"));

    let all = db::filter_rules::list_all(&pool).await.unwrap();
    assert_eq!(all.len(), 1);

    let mut edited = r.clone();
    edited.action = "keep".into();
    db::filter_rules::update(&pool, &edited).await.unwrap();
    db::filter_rules::set_enabled(&pool, r.id, false)
        .await
        .unwrap();

    db::filter_rules::delete(&pool, r.id).await.unwrap();
    assert!(db::filter_rules::list_all(&pool).await.unwrap().is_empty());
}

#[tokio::test]
async fn resolve_for_email_beats_domain_beats_global() {
    let pool = mem_pool().await;
    db::filter_rules::insert(&pool, &input("global", "", "signature", "strip", None))
        .await
        .unwrap();
    db::filter_rules::insert(
        &pool,
        &input("domain", "cnssl.cn", "signature", "keep", None),
    )
    .await
    .unwrap();
    db::filter_rules::insert(
        &pool,
        &input("email", "boss@cnssl.cn", "signature", "strip", None),
    )
    .await
    .unwrap();
    db::filter_rules::insert(
        &pool,
        &input("email", "boss@cnssl.cn", "quote", "keep", None),
    )
    .await
    .unwrap();

    let r = db::filter_rules::resolve_for(&pool, Some("Boss <boss@cnssl.cn>"))
        .await
        .unwrap();
    assert_eq!(
        r.signature,
        Some(Action::Strip),
        "email 级 signature 应胜出"
    );
    assert_eq!(r.quote, Some(Action::Keep), "email 级 quote keep");
    assert_eq!(r.repeat, None, "无 repeat 规则 → None（走能力默认）");
}

#[tokio::test]
async fn resolve_for_domain_when_no_email_rule() {
    let pool = mem_pool().await;
    db::filter_rules::insert(&pool, &input("global", "", "signature", "strip", None))
        .await
        .unwrap();
    db::filter_rules::insert(
        &pool,
        &input("domain", "cnssl.cn", "signature", "keep", None),
    )
    .await
    .unwrap();
    let r = db::filter_rules::resolve_for(&pool, Some("hr@cnssl.cn"))
        .await
        .unwrap();
    assert_eq!(r.signature, Some(Action::Keep), "domain 应胜过 global");
}

#[tokio::test]
async fn resolve_for_disabled_rule_ignored() {
    let pool = mem_pool().await;
    let g = db::filter_rules::insert(&pool, &input("global", "", "signature", "strip", None))
        .await
        .unwrap();
    db::filter_rules::set_enabled(&pool, g.id, false)
        .await
        .unwrap();
    let r = db::filter_rules::resolve_for(&pool, Some("x@y.com"))
        .await
        .unwrap();
    assert_eq!(r.signature, None, "禁用规则不参与解析");
}

#[tokio::test]
async fn resolve_for_carries_signature_pattern() {
    let pool = mem_pool().await;
    db::filter_rules::insert(
        &pool,
        &input(
            "domain",
            "cnssl.cn",
            "signature",
            "strip",
            Some("Disclaimer"),
        ),
    )
    .await
    .unwrap();
    let r = db::filter_rules::resolve_for(&pool, Some("a@cnssl.cn"))
        .await
        .unwrap();
    assert_eq!(r.signature_pattern.as_deref(), Some("Disclaimer"));
}

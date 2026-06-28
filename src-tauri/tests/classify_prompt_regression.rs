//! 防误导 opt-in 回归测试
//!
//! 守卫两类误判：
//!   - 标题含反垃圾标记（如 `[SPAM]`）的正常业务邮件不应被分类为 spam；
//!   - 商业服务商的事务性通知（如「Certificate has been created」）应判 notification、不误判 promotion。
//!
//! 默认跳过（`#[ignore]`），不打 Anthropic API。
//! 手动真实 API 运行：
//!   ANTHROPIC_API_KEY=sk-ant-... cargo test --test classify_prompt_regression -- --ignored
//!
//! 脚手架依赖：
//!   - temp on-disk SQLite（与 sync_db.rs 等同款）
//!   - ai_models::insert + ai_role_defaults::set 配置 classify 角色
//!   - keychain::store_ai_key 把 env key 写入 OS keychain（测试结束后清理）
//!   - classify_message_ids 是集成被测路径

use ai_email_lib::db::ai_models::{self, AiModelInput};
use ai_email_lib::db::{self, ai_role_defaults, Pool};
use ai_email_lib::{ai::classify, keychain};
use secrecy::SecretString;
use uuid::Uuid;

/// debug 构建下 keychain 走文件态：两个 #[ignore] 例共用同一固定 dev 凭据文件
/// （不同 model UUID 天然隔离、env 恒同值 → `--ignored` 并行也无 race）。release 为 no-op。
#[cfg(debug_assertions)]
fn ensure_dev_cred_env() {
    use std::sync::OnceLock;
    static DEV_CRED: OnceLock<std::path::PathBuf> = OnceLock::new();
    let path = DEV_CRED.get_or_init(|| std::env::temp_dir().join("ai-email-itest-dev-creds.json"));
    std::env::set_var("AI_EMAIL_DEV_CRED_FILE", path);
}
#[cfg(not(debug_assertions))]
fn ensure_dev_cred_env() {}

/// 创建临时 on-disk DB，返回 (Pool, TempDir guard)。
async fn temp_db() -> (Pool, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("create tempdir");
    let path = dir.path().join("test.db");
    let pool = db::connect(&path).await.expect("connect + migrate");
    (pool, dir)
}

/// Seed 最小合法父行（accounts + mailboxes）再插一封邮件，返回 message_id。
async fn seed_message(pool: &Pool, subject: &str, from_addr: &str, snippet: &str) -> Uuid {
    let account_id = Uuid::new_v4();
    let mailbox_id = Uuid::new_v4();
    let msg_id = Uuid::new_v4();

    sqlx::query(
        "INSERT INTO accounts (id, email, provider, imap_host, smtp_host) \
         VALUES (?1, ?2, 'imap', 'imap.test', 'smtp.test')",
    )
    .bind(account_id)
    .bind(format!("reg-{account_id}@test.invalid"))
    .execute(pool)
    .await
    .expect("insert account");

    sqlx::query("INSERT INTO mailboxes (id, account_id, name) VALUES (?1, ?2, 'INBOX')")
        .bind(mailbox_id)
        .bind(account_id)
        .execute(pool)
        .await
        .expect("insert mailbox");

    sqlx::query(
        "INSERT INTO messages (id, account_id, mailbox_id, imap_uid, flags, subject, from_addr, snippet) \
         VALUES (?1, ?2, ?3, 1, '[]', ?4, ?5, ?6)",
    )
    .bind(msg_id)
    .bind(account_id)
    .bind(mailbox_id)
    .bind(subject)
    .bind(from_addr)
    .bind(snippet)
    .execute(pool)
    .await
    .expect("insert message");

    msg_id
}

/// A2 防误导规则回归：标题含 `[SPAM]` 反垃圾标记的正常业务邮件（合法域名 + 账单内容）
/// 不应被分类为 spam，期望分类为 notification 或 promotion。
///
/// 默认跳过；真实 API 时手动运行：
///   ANTHROPIC_API_KEY=sk-ant-... cargo test --test classify_prompt_regression -- --ignored
#[tokio::test]
#[ignore]
async fn antispam_label_in_subject_should_not_classify_as_spam() {
    let api_key_str = match std::env::var("ANTHROPIC_API_KEY") {
        Ok(k) if !k.is_empty() => k,
        _ => {
            eprintln!("缺 ANTHROPIC_API_KEY 环境变量，跳过真实 API 测试");
            return;
        }
    };

    let (pool, _dir) = temp_db().await;

    // 注册 classify 模型
    let model = ai_models::insert(
        &pool,
        &AiModelInput {
            display_name: "Haiku (regression test)".into(),
            provider: "anthropic".into(),
            model_id: "claude-haiku-4-5".into(),
            base_url: None,
        },
    )
    .await
    .expect("insert ai_model");

    // 配置 classify role
    ai_role_defaults::set(&pool, "classify", model.id)
        .await
        .expect("set ai_role_default");

    // 把 env key 写入 OS keychain（classify_message_ids 从 keychain 取 key）
    let secret = SecretString::from(api_key_str);
    ensure_dev_cred_env();
    keychain::store_ai_key(model.id, &secret).expect("store_ai_key");

    // Seed 一封看起来是合法账单通知、但标题含 [SPAM] 标记的邮件
    let msg_id = seed_message(
        &pool,
        "[SPAM] 您的账单已生成",
        "billing@real-co.com",
        "本月账单 ¥128 已生成，点击查看明细。",
    )
    .await;

    // 调被测函数
    let results = classify::classify_message_ids(&pool, &[msg_id])
        .await
        .expect("classify_message_ids");

    // 清理 keychain（测试结束无论成败都应清理，ignore 路径下 keyring 条目需手动删）
    let _ = keychain::delete_ai_key(model.id);

    assert_eq!(results.len(), 1, "应返回恰好一条分类结果");
    let cat = &results[0].classification.category;
    assert_ne!(
        cat, "spam",
        "标题含 [SPAM] 的合法账单邮件不应被判为 spam（实际分类: {cat}）\n\
         期望: notification 或 promotion"
    );
}

/// 通知 vs 推广 区分回归：来自商业服务商的事务性「证书已签发」邮件应判为 notification，
/// 不应因发件方是付费服务商而误判为 promotion / spam。
///
/// 默认跳过；真实 API 时手动运行：
///   ANTHROPIC_API_KEY=sk-ant-... cargo test --test classify_prompt_regression -- --ignored
#[tokio::test]
#[ignore]
async fn certificate_created_should_classify_as_notification() {
    let api_key_str = match std::env::var("ANTHROPIC_API_KEY") {
        Ok(k) if !k.is_empty() => k,
        _ => {
            eprintln!("缺 ANTHROPIC_API_KEY 环境变量，跳过真实 API 测试");
            return;
        }
    };

    let (pool, _dir) = temp_db().await;

    let model = ai_models::insert(
        &pool,
        &AiModelInput {
            display_name: "Haiku (regression test)".into(),
            provider: "anthropic".into(),
            model_id: "claude-haiku-4-5".into(),
            base_url: None,
        },
    )
    .await
    .expect("insert ai_model");

    ai_role_defaults::set(&pool, "classify", model.id)
        .await
        .expect("set ai_role_default");

    let secret = SecretString::from(api_key_str);
    ensure_dev_cred_env();
    keychain::store_ai_key(model.id, &secret).expect("store_ai_key");

    // 典型 SSL 服务商签发通知：商业发件方 + 产品名，但主旨是告知一个已发生的事务
    let msg_id = seed_message(
        &pool,
        "Certificate has been created - *.example.com",
        "noreply@cnssl.cn",
        "Your SSL certificate for *.example.com has been issued and is ready to download.",
    )
    .await;

    let results = classify::classify_message_ids(&pool, &[msg_id])
        .await
        .expect("classify_message_ids");

    let _ = keychain::delete_ai_key(model.id);

    assert_eq!(results.len(), 1, "应返回恰好一条分类结果");
    let cat = &results[0].classification.category;
    assert_eq!(
        cat, "notification",
        "商业服务商的事务性证书签发邮件应判为 notification（实际分类: {cat}）"
    );
}

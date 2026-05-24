//! SMTP delivery via lettre over rustls/tokio.

use lettre::message::{header::ContentType, Mailbox};
use lettre::transport::smtp::authentication::Credentials;
use lettre::transport::smtp::client::{Tls, TlsParameters};
use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::db::accounts::Account;
use crate::db::send_log::{self, SendLog, SendLogInsert};
use crate::db::{accounts, Pool};
use crate::error::{AppError, AppResult};
use crate::keychain;

/// What the UI sends across the FFI.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SendDraft {
    pub account_id: Uuid,
    pub to: Vec<String>,
    #[serde(default)]
    pub cc: Vec<String>,
    pub subject: String,
    pub body: String,
    /// Original message UUID when this is a reply; null for fresh compose.
    pub in_reply_to: Option<Uuid>,
    /// True when the body came (originally) from an AI draft; recorded in `send_log` for
    /// audit. The UI sets this when a draft was loaded into the composer.
    #[serde(default)]
    pub ai_assisted: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SendReceipt {
    pub send_log: SendLog,
}

/// One-shot SMTP delivery. Steps:
///   1. Load the account, pull keychain key.
///   2. Build the RFC 5322 Message via lettre's builder.
///   3. Connect over implicit TLS on `account.smtp_port` to `account.smtp_host`.
///   4. AUTH PLAIN/LOGIN with email + auth code.
///   5. SEND. On success: write a happy `send_log` row + return.
///      On failure: write a `send_log` row with the SMTP error, then return the error.
pub async fn send_draft(pool: &Pool, draft: &SendDraft) -> AppResult<SendReceipt> {
    if draft.to.is_empty() {
        return Err(AppError::Smtp("To list is empty".into()));
    }

    let account = accounts::get(pool, draft.account_id)
        .await?
        .ok_or_else(|| AppError::Config(format!("account {} not found", draft.account_id)))?;

    let account_id = account.id;
    let api_key: SecretString =
        tokio::task::spawn_blocking(move || keychain::get_auth_code(account_id))
            .await
            .map_err(|e| AppError::Other(anyhow::anyhow!(e)))??;

    let message = build_message(&account, draft)?;

    let port = u16::try_from(account.smtp_port)
        .map_err(|_| AppError::Smtp(format!("invalid smtp_port: {}", account.smtp_port)))?;
    let mailer = build_transport(&account.smtp_host, port, &account.email, &api_key)?;

    let smtp_outcome = mailer.send(message).await;
    match smtp_outcome {
        Ok(resp) => {
            let smtp_response = format!("{:?}", resp.code());
            let log = send_log::insert(
                pool,
                &SendLogInsert {
                    account_id: account.id,
                    in_reply_to: draft.in_reply_to,
                    to_addrs: draft.to.clone(),
                    subject: draft.subject.clone(),
                    ai_assisted: draft.ai_assisted,
                    smtp_response: Some(smtp_response.clone()),
                },
            )
            .await?;
            tracing::info!(
                send_log_id = %log.id,
                account_id = %account.id,
                ai_assisted = draft.ai_assisted,
                "smtp send succeeded"
            );
            Ok(SendReceipt { send_log: log })
        }
        Err(e) => {
            let msg = format!("{e}");
            // Audit even on failure — SPEC § 9 says we never silently drop sends.
            if let Err(log_err) = send_log::insert(
                pool,
                &SendLogInsert {
                    account_id: account.id,
                    in_reply_to: draft.in_reply_to,
                    to_addrs: draft.to.clone(),
                    subject: draft.subject.clone(),
                    ai_assisted: draft.ai_assisted,
                    smtp_response: Some(format!("ERROR: {msg}")),
                },
            )
            .await
            {
                tracing::error!(error = ?log_err, "failed to record send_log row after send failure");
            }
            Err(AppError::Smtp(msg))
        }
    }
}

fn build_message(account: &Account, draft: &SendDraft) -> AppResult<Message> {
    let from_addr: lettre::Address = account
        .email
        .parse()
        .map_err(|e| AppError::Smtp(format!("invalid from address {}: {e}", account.email)))?;
    let from = Mailbox::new(account.display_name.clone(), from_addr);

    let mut builder = Message::builder().from(from).subject(draft.subject.clone());

    for raw in &draft.to {
        let mb = parse_mailbox(raw)?;
        builder = builder.to(mb);
    }
    for raw in &draft.cc {
        let mb = parse_mailbox(raw)?;
        builder = builder.cc(mb);
    }

    builder
        .header(ContentType::TEXT_PLAIN)
        .body(draft.body.clone())
        .map_err(|e| AppError::Smtp(format!("message build failed: {e}")))
}

/// Parse "Name <email>" or bare "email". lettre's `Mailbox: FromStr` does exactly this; we
/// just wrap the error so the user sees which input was bad.
fn parse_mailbox(raw: &str) -> AppResult<Mailbox> {
    raw.trim()
        .parse::<Mailbox>()
        .map_err(|e| AppError::Smtp(format!("invalid address {raw:?}: {e}")))
}

fn build_transport(
    host: &str,
    port: u16,
    email: &str,
    auth_code: &SecretString,
) -> AppResult<AsyncSmtpTransport<Tokio1Executor>> {
    let tls = TlsParameters::new(host.to_string())
        .map_err(|e| AppError::Smtp(format!("TLS params for {host}: {e}")))?;
    let creds = Credentials::new(email.to_string(), auth_code.expose_secret().to_string());

    let mailer = AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(host)
        .port(port)
        .tls(Tls::Wrapper(tls))
        .credentials(creds)
        .build();
    Ok(mailer)
}

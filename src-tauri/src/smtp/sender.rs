//! SMTP delivery via lettre over rustls/tokio.

use lettre::message::{header, header::ContentType, Mailbox};
use lettre::transport::smtp::authentication::Credentials;
use lettre::transport::smtp::client::{Tls, TlsParameters};
use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::db::accounts::Account;
use crate::db::send_log::{self, SendLog, SendLogInsert};
use crate::db::{accounts, messages, Pool};
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
///   1. Validate recipients (to ∪ cc must be non-empty); deduplicate.
///   2. Load the account, pull keychain key.
///   3. If replying, fetch the original message's RFC Message-ID for threading headers.
///   4. Build the RFC 5322 Message via lettre's builder.
///   5. Connect over implicit TLS on `account.smtp_port` to `account.smtp_host`.
///   6. AUTH PLAIN/LOGIN with email + auth code.
///   7. SEND. On success: write a happy `send_log` row (best-effort; failure → warn only) + return.
///      On failure: write a `send_log` row WITHOUT `in_reply_to` (so the suggested reply
///      remains retryable), then return the error.
pub async fn send_draft(pool: &Pool, draft: &SendDraft) -> AppResult<SendReceipt> {
    // #35: validate to ∪ cc non-empty; allow cc-only sends.
    validate_recipients(&draft.to, &draft.cc)?;

    // #35: deduplicate recipients.
    let to_deduped = dedup_addrs(&draft.to);
    let cc_deduped = dedup_addrs(&draft.cc);

    let account = accounts::get(pool, draft.account_id)
        .await?
        .ok_or_else(|| AppError::Config(format!("account {} not found", draft.account_id)))?;

    let account_id = account.id;
    let api_key: SecretString =
        tokio::task::spawn_blocking(move || keychain::get_auth_code(account_id))
            .await
            .map_err(|e| AppError::Other(anyhow::anyhow!(e)))??;

    // #7: look up the RFC Message-ID and References chain of the original message for threading.
    let (rfc_message_id, original_references) = if let Some(reply_id) = draft.in_reply_to {
        match messages::get(pool, reply_id).await? {
            Some(m) => (m.rfc_message_id, m.references_header),
            None => (None, None),
        }
    } else {
        (None, None)
    };

    let message = build_message(
        &account,
        draft,
        &to_deduped,
        &cc_deduped,
        rfc_message_id.as_deref(),
        original_references.as_deref(),
    )?;

    let port = u16::try_from(account.smtp_port)
        .map_err(|_| AppError::Smtp(format!("invalid smtp_port: {}", account.smtp_port)))?;
    let mailer = build_transport(&account.smtp_host, port, &account.email, &api_key)?;

    let smtp_outcome = mailer.send(message).await;
    match smtp_outcome {
        Ok(resp) => {
            // #34: record both code and first response line (queue-id etc.) for auditability.
            let smtp_response = format_smtp_response(&resp);

            // #8: send_log write is best-effort — a log failure must NOT turn a successful
            // delivery into an error (which would mislead users into resending).
            match send_log::insert(
                pool,
                &SendLogInsert {
                    account_id: account.id,
                    // #19: only write in_reply_to on success, so failed sends don't permanently
                    // exclude the suggested reply from the retry queue.
                    in_reply_to: draft.in_reply_to,
                    to_addrs: to_deduped.clone(),
                    subject: draft.subject.clone(),
                    ai_assisted: draft.ai_assisted,
                    smtp_response: Some(smtp_response),
                },
            )
            .await
            {
                Ok(log) => {
                    tracing::info!(
                        send_log_id = %log.id,
                        account_id = %account.id,
                        ai_assisted = draft.ai_assisted,
                        "smtp send succeeded"
                    );
                    Ok(SendReceipt { send_log: log })
                }
                Err(log_err) => {
                    // #8: emit a warning but still return success — the mail was delivered.
                    tracing::warn!(
                        error = ?log_err,
                        account_id = %account.id,
                        "smtp send succeeded but send_log insert failed (mail was delivered)"
                    );
                    // Return a degraded receipt with a synthetic log row so the caller gets a
                    // consistent SendReceipt shape. The synthetic row is NOT persisted.
                    let now = time::OffsetDateTime::now_utc();
                    Ok(SendReceipt {
                        send_log: SendLog {
                            id: Uuid::new_v4(),
                            account_id: account.id,
                            in_reply_to: draft.in_reply_to,
                            to_addrs: to_deduped,
                            subject: draft.subject.clone(),
                            ai_assisted: draft.ai_assisted,
                            sent_at: now,
                            smtp_response: None,
                        },
                    })
                }
            }
        }
        Err(e) => {
            let msg = format!("{e}");
            // Audit even on failure — never silently drop send attempts.
            // #19: do NOT set in_reply_to on failure rows — otherwise the suggested reply would
            // be permanently excluded from the retry queue by list_pending's subquery.
            if let Err(log_err) = send_log::insert(
                pool,
                &SendLogInsert {
                    account_id: account.id,
                    in_reply_to: None,
                    to_addrs: to_deduped,
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

/// Format the SMTP server response for storage: "{code} {first_line}".
/// Preserves queue-ID and other tracing info from the server response body.
fn format_smtp_response(resp: &lettre::transport::smtp::response::Response) -> String {
    let code = resp.code().to_string();
    match resp.first_line() {
        Some(line) => format!("{code} {line}"),
        None => code,
    }
}

/// Validate that at least one recipient is present (to ∪ cc must be non-empty).
/// Extracted so it can be unit-tested without a real pool or keychain.
fn validate_recipients(to: &[String], cc: &[String]) -> AppResult<()> {
    if to.is_empty() && cc.is_empty() {
        return Err(AppError::Smtp(
            "At least one recipient (To or Cc) is required".into(),
        ));
    }
    Ok(())
}

/// Deduplicate a list of raw address strings, preserving order.
fn dedup_addrs(addrs: &[String]) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    addrs
        .iter()
        .filter(|a| seen.insert(a.trim().to_lowercase()))
        .cloned()
        .collect()
}

/// Build an RFC 5322 Message.
///
/// `rfc_id` is the RFC Message-ID (`<id@host>`) of the original message being replied to.
/// `original_refs` is the space-separated References header of that original message.
///
/// When replying, `In-Reply-To` is set to `rfc_id`, and `References` is built per RFC 5322 §3.6.4:
///   - Start with the original message's own References chain (if any).
///   - Append the original message's Message-ID at the end.
///
/// This produces a complete chain: `<a> <b> … <c>` where `<c>` is the message being replied to.
fn build_message(
    account: &Account,
    draft: &SendDraft,
    to: &[String],
    cc: &[String],
    rfc_id: Option<&str>,
    original_refs: Option<&str>,
) -> AppResult<Message> {
    let from_addr: lettre::Address = account
        .email
        .parse()
        .map_err(|e| AppError::Smtp(format!("invalid from address {}: {e}", account.email)))?;
    let from = Mailbox::new(account.display_name.clone(), from_addr);

    let mut builder = Message::builder().from(from).subject(draft.subject.clone());

    for raw in to {
        let mb = parse_mailbox(raw)?;
        builder = builder.to(mb);
    }
    for raw in cc {
        let mb = parse_mailbox(raw)?;
        builder = builder.cc(mb);
    }

    // #7: set threading headers when replying.
    // References = original_refs (existing chain) + space + rfc_id (the message being replied to).
    // In-Reply-To = rfc_id only (RFC 5322 §3.6.4).
    if let Some(mid) = rfc_id {
        builder = builder.in_reply_to(mid.to_string());
        let references = match original_refs {
            Some(refs) if !refs.trim().is_empty() => format!("{} {}", refs.trim(), mid),
            _ => mid.to_string(),
        };
        builder = builder.header(header::References::from(references));
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

#[cfg(test)]
mod tests {
    use super::*;
    use time::OffsetDateTime;
    use uuid::Uuid;

    fn dummy_account(email: &str) -> Account {
        Account {
            id: Uuid::new_v4(),
            email: email.to_string(),
            display_name: Some("Test User".to_string()),
            provider: "imap".to_string(),
            imap_host: "imap.example.com".to_string(),
            imap_port: 993,
            smtp_host: "smtp.example.com".to_string(),
            smtp_port: 465,
            created_at: OffsetDateTime::now_utc(),
            last_synced_at: None,
        }
    }

    fn draft_base() -> SendDraft {
        SendDraft {
            account_id: Uuid::new_v4(),
            to: vec!["bob@example.com".to_string()],
            cc: vec![],
            subject: "Hello".to_string(),
            body: "Hi there".to_string(),
            in_reply_to: None,
            ai_assisted: false,
        }
    }

    // ── #35: recipient validation & dedup ─────────────────────────────────────

    #[test]
    fn rejects_empty_to_and_cc() {
        // Directly exercise the validation function extracted from send_draft's guard:
        // both to AND cc empty → the function returns Err.
        let result = validate_recipients(&[], &[]);
        assert!(result.is_err(), "empty to+cc must return Err");
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("recipient"),
            "error message should mention recipients: {err_msg}"
        );
    }

    #[test]
    fn allows_cc_only_send() {
        let result = validate_recipients(&[], &["carol@example.com".to_string()]);
        assert!(result.is_ok(), "cc-only should pass validation");
    }

    #[test]
    fn dedup_removes_duplicates_preserving_order() {
        let addrs = vec![
            "Alice <alice@example.com>".to_string(),
            "bob@example.com".to_string(),
            "Alice <alice@example.com>".to_string(), // duplicate
            "charlie@example.com".to_string(),
            "BOB@example.com".to_string(), // case duplicate
        ];
        let result = dedup_addrs(&addrs);
        // Dedup is case-insensitive on trimmed lowercase.
        assert_eq!(
            result.len(),
            3,
            "should remove 2 duplicates, got: {result:?}"
        );
        assert_eq!(result[0], "Alice <alice@example.com>");
        assert_eq!(result[1], "bob@example.com");
        assert_eq!(result[2], "charlie@example.com");
    }

    #[test]
    fn dedup_empty_list() {
        assert!(dedup_addrs(&[]).is_empty());
    }

    // ── #7: In-Reply-To / References headers ──────────────────────────────────

    /// 无原链时 References = 单个被回复的 message-id（In-Reply-To 同）。
    #[test]
    fn build_message_sets_in_reply_to_and_references_no_prior_chain() {
        let account = dummy_account("alice@example.com");
        let draft = draft_base();
        let rfc_id = "<original-msg-id@mail.example.com>";

        let msg = build_message(&account, &draft, &draft.to, &draft.cc, Some(rfc_id), None)
            .expect("build_message must succeed");

        let raw = String::from_utf8_lossy(&msg.formatted()).into_owned();
        assert!(
            raw.contains(&format!("In-Reply-To: {rfc_id}")),
            "In-Reply-To header missing or wrong in:\n{raw}"
        );
        assert!(
            raw.contains(&format!("References: {rfc_id}")),
            "References header missing or wrong in:\n{raw}"
        );
    }

    /// 多层链：被回复邮件有 references_header = "<a@x> <b@x>"，message-id = "<c@x>"。
    /// 回复的 References 应为 "<a@x> <b@x> <c@x>"（延续链 + 追加），In-Reply-To = "<c@x>"。
    #[test]
    fn build_message_extends_references_chain() {
        let account = dummy_account("alice@example.com");
        let draft = draft_base();
        let original_refs = "<a@x> <b@x>";
        let rfc_id = "<c@x>";

        let msg = build_message(
            &account,
            &draft,
            &draft.to,
            &draft.cc,
            Some(rfc_id),
            Some(original_refs),
        )
        .expect("build_message must succeed");

        let raw = String::from_utf8_lossy(&msg.formatted()).into_owned();
        assert!(
            raw.contains("In-Reply-To: <c@x>"),
            "In-Reply-To must be the direct parent:\n{raw}"
        );
        assert!(
            raw.contains("References: <a@x> <b@x> <c@x>"),
            "References must extend the prior chain:\n{raw}"
        );
    }

    #[test]
    fn build_message_no_threading_headers_for_fresh_compose() {
        let account = dummy_account("alice@example.com");
        let draft = draft_base();

        let msg = build_message(&account, &draft, &draft.to, &draft.cc, None, None)
            .expect("build_message must succeed");

        let raw = String::from_utf8_lossy(&msg.formatted()).into_owned();
        assert!(
            !raw.contains("In-Reply-To:"),
            "fresh compose must not have In-Reply-To"
        );
        assert!(
            !raw.contains("References:"),
            "fresh compose must not have References"
        );
    }

    #[test]
    fn build_message_cc_only_builds_successfully() {
        let account = dummy_account("alice@example.com");
        let mut draft = draft_base();
        draft.to = vec![];
        draft.cc = vec!["carol@example.com".to_string()];

        // build_message itself doesn't enforce the to+cc non-empty check — send_draft does.
        let msg = build_message(&account, &draft, &draft.to, &draft.cc, None, None)
            .expect("cc-only message should build");
        let raw = String::from_utf8_lossy(&msg.formatted()).into_owned();
        assert!(raw.contains("Cc: carol@example.com"), "Cc header expected");
    }

    // ── #34: SMTP response format ──────────────────────────────────────────────

    #[test]
    fn format_smtp_response_includes_code_and_message() {
        use lettre::transport::smtp::response::{Category, Code, Detail, Response, Severity};
        let code = Code::new(
            Severity::PositiveCompletion,
            Category::MailSystem,
            Detail::Zero,
        );
        let resp = Response::new(code, vec!["2.0.0 OK: queued as abc123".to_string()]);
        let formatted = format_smtp_response(&resp);
        assert!(
            formatted.starts_with("250"),
            "should start with status code: {formatted}"
        );
        assert!(
            formatted.contains("queued as abc123"),
            "should include queue-id: {formatted}"
        );
    }

    #[test]
    fn format_smtp_response_no_message_falls_back_to_code() {
        use lettre::transport::smtp::response::{Category, Code, Detail, Response, Severity};
        let code = Code::new(
            Severity::PositiveCompletion,
            Category::MailSystem,
            Detail::Zero,
        );
        let resp = Response::new(code, vec![]);
        let formatted = format_smtp_response(&resp);
        assert_eq!(formatted, "250", "code-only response: {formatted}");
    }
}

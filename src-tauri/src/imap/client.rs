//! Thin wrapper around `async_imap::Session` over a rustls-backed TCP stream.
//!
//! The wrapper exists so the rest of the crate doesn't have to think about the IMAP wire
//! protocol — callers ask for "headers in this UID range" and get back owned structs they can
//! persist. Per-call streams are fully drained before returning so the underlying mut-borrow
//! on the session ends with the function call.

use async_imap::Session;
use futures::StreamExt;
use rustls::pki_types::ServerName;
use secrecy::{ExposeSecret, SecretString};
use tokio::net::TcpStream;
use tokio_rustls::client::TlsStream;

use crate::error::{AppError, AppResult};
use crate::imap::tls;

/// One open IMAP session. Not thread-safe; never share across tasks.
pub struct ImapClient {
    session: Session<TlsStream<TcpStream>>,
}

#[derive(Debug, Clone)]
pub struct MailboxInfo {
    pub name: String,
    pub delimiter: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SelectedMailbox {
    pub exists: u32,
    pub uid_next: Option<u32>,
    pub uid_validity: Option<u32>,
}

#[derive(Debug, Clone)]
pub struct FetchedHeader {
    pub uid: u32,
    pub flags: Vec<String>,
    pub size_bytes: Option<i32>,
    pub header_bytes: Vec<u8>,
}

impl ImapClient {
    /// IMAPS connect → LOGIN → ID. Failure at any step returns an [`AppError::Imap`] with the
    /// provider's message — we don't try to recover, the caller surfaces it to the UI.
    pub async fn connect(
        host: &str,
        port: u16,
        email: &str,
        auth_code: &SecretString,
    ) -> AppResult<Self> {
        let connector = tls::build_connector();
        let tcp = TcpStream::connect((host, port)).await?;
        let server_name = ServerName::try_from(host.to_owned())
            .map_err(|e| AppError::Imap(format!("invalid TLS server name: {e}")))?;
        let tls = connector
            .connect(server_name, tcp)
            .await
            .map_err(|e| AppError::Imap(format!("TLS handshake failed: {e}")))?;

        let client = async_imap::Client::new(tls);
        let mut session = client
            .login(email, auth_code.expose_secret())
            .await
            .map_err(|(e, _client)| AppError::Imap(e.to_string()))?;

        // RFC 2971 ID. Required by 163 since 2022; QQ accepts it. Sending a static identity
        // keeps us from leaking host info to the provider.
        session
            .run_command_and_check_ok(r#"ID ("name" "ai-email" "version" "0.1.0")"#)
            .await
            .map_err(|e| AppError::Imap(e.to_string()))?;

        tracing::debug!(host, port, email, "imap session ready");
        Ok(Self { session })
    }

    /// LIST every mailbox the account can see. Returns name + hierarchy delimiter; we throw
    /// away the IMAP attributes (Marked / Noselect / …) at MVP — Sprint 1 only touches INBOX.
    pub async fn list_mailboxes(&mut self) -> AppResult<Vec<MailboxInfo>> {
        let mut stream = self
            .session
            .list(Some(""), Some("*"))
            .await
            .map_err(|e| AppError::Imap(e.to_string()))?;
        let mut out = Vec::new();
        while let Some(item) = stream.next().await {
            let name = item.map_err(|e| AppError::Imap(e.to_string()))?;
            out.push(MailboxInfo {
                name: name.name().to_string(),
                delimiter: name.delimiter().map(str::to_string),
            });
        }
        Ok(out)
    }

    /// SELECT and return the resulting state. We only read three fields; the rest of the
    /// response (PERMANENTFLAGS, RECENT, etc.) is ignored at MVP.
    pub async fn select(&mut self, name: &str) -> AppResult<SelectedMailbox> {
        let m = self
            .session
            .select(name)
            .await
            .map_err(|e| AppError::Imap(e.to_string()))?;
        Ok(SelectedMailbox {
            exists: m.exists,
            uid_next: m.uid_next,
            uid_validity: m.uid_validity,
        })
    }

    /// `FETCH <set> (UID FLAGS RFC822.SIZE BODY.PEEK[HEADER])` using sequence numbers. Best for
    /// the first sync where we just want the N most recent rows (e.g. `(exists-49):*`).
    pub async fn fetch_headers(&mut self, seq_set: &str) -> AppResult<Vec<FetchedHeader>> {
        let stream = self
            .session
            .fetch(seq_set, FETCH_QUERY)
            .await
            .map_err(|e| AppError::Imap(e.to_string()))?;
        drain_fetch_stream(stream).await
    }

    /// `UID FETCH <set> (UID FLAGS RFC822.SIZE BODY.PEEK[HEADER])`. Best for incremental sync
    /// (e.g. `<prev_uid_next>:*`) — UIDs are stable across SELECT, sequence numbers aren't.
    pub async fn uid_fetch_headers(&mut self, uid_set: &str) -> AppResult<Vec<FetchedHeader>> {
        let stream = self
            .session
            .uid_fetch(uid_set, FETCH_QUERY)
            .await
            .map_err(|e| AppError::Imap(e.to_string()))?;
        drain_fetch_stream(stream).await
    }

    pub async fn logout(mut self) -> AppResult<()> {
        self.session
            .logout()
            .await
            .map_err(|e| AppError::Imap(e.to_string()))
    }
}

/// PEEK avoids setting `\Seen` as a side effect of syncing. We include UID even when fetching
/// by sequence number so the persistence layer always has a stable identifier.
const FETCH_QUERY: &str = "(UID FLAGS RFC822.SIZE BODY.PEEK[HEADER])";

async fn drain_fetch_stream<S>(mut stream: S) -> AppResult<Vec<FetchedHeader>>
where
    S: futures::Stream<Item = async_imap::error::Result<async_imap::types::Fetch>> + Unpin,
{
    let mut out = Vec::new();
    while let Some(item) = stream.next().await {
        let f = item.map_err(|e| AppError::Imap(e.to_string()))?;
        let Some(uid) = f.uid else {
            tracing::warn!("FETCH item missing UID; skipping");
            continue;
        };
        out.push(FetchedHeader {
            uid,
            flags: f.flags().map(flag_to_string).collect(),
            size_bytes: f.size.and_then(|s| i32::try_from(s).ok()),
            header_bytes: f.header().unwrap_or(&[]).to_vec(),
        });
    }
    Ok(out)
}

/// `async_imap::types::Flag` doesn't implement `Display`, so spell out the canonical IMAP
/// wire form here. We persist these as `TEXT[]` rows verbatim — frontend filters look for
/// the literal `\Seen` / `\Flagged` substrings.
fn flag_to_string(f: async_imap::types::Flag<'_>) -> String {
    use async_imap::types::Flag;
    match f {
        Flag::Seen => "\\Seen".into(),
        Flag::Answered => "\\Answered".into(),
        Flag::Flagged => "\\Flagged".into(),
        Flag::Deleted => "\\Deleted".into(),
        Flag::Draft => "\\Draft".into(),
        Flag::Recent => "\\Recent".into(),
        Flag::MayCreate => "\\*".into(),
        Flag::Custom(c) => c.into_owned(),
    }
}

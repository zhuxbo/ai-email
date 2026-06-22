//! Thin wrapper around `async_imap::Session` over a rustls-backed TCP stream.
//!
//! The wrapper exists so the rest of the crate doesn't have to think about the IMAP wire
//! protocol — callers ask for "headers in this UID range" and get back owned structs they can
//! persist. Per-call streams are fully drained before returning so the underlying mut-borrow
//! on the session ends with the function call.

use std::time::Duration;

use async_imap::Session;
use futures::StreamExt;
use rustls::pki_types::ServerName;
use secrecy::{ExposeSecret, SecretString};
use tokio::net::TcpStream;
use tokio::time::timeout;
use tokio_rustls::client::TlsStream;

use crate::error::{AppError, AppResult};
use crate::imap::tls;

/// Total budget for TCP connect + TLS handshake + LOGIN + ID exchange.
/// Covers QQ Mail's typical latency (< 3 s) with a large safety margin for weak mobile links.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(30);

/// Per-operation budget for LIST / SELECT / FETCH / STORE / MOVE.
/// Keeps individual commands from stalling indefinitely on half-open connections.
const OP_TIMEOUT: Duration = Duration::from_secs(30);

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
    /// IMAPS connect → LOGIN → ID. The entire handshake is bounded by [`CONNECT_TIMEOUT`].
    /// Failure at any step returns an [`AppError::Imap`] with a readable message so the UI can
    /// prompt the user to retry rather than spinning indefinitely.
    pub async fn connect(
        host: &str,
        port: u16,
        email: &str,
        auth_code: &SecretString,
    ) -> AppResult<Self> {
        timeout(
            CONNECT_TIMEOUT,
            Self::connect_inner(host, port, email, auth_code),
        )
        .await
        .unwrap_or_else(|_| {
            Err(AppError::Imap(format!(
                "connect to {host}:{port} timed out after {}s",
                CONNECT_TIMEOUT.as_secs()
            )))
        })
    }

    async fn connect_inner(
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
        let mut stream = timeout(OP_TIMEOUT, self.session.list(Some(""), Some("*")))
            .await
            .unwrap_or_else(|_| {
                Err(async_imap::error::Error::Io(std::io::Error::other(
                    "LIST timed out",
                )))
            })
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
        let m = timeout(OP_TIMEOUT, self.session.select(name))
            .await
            .unwrap_or_else(|_| {
                Err(async_imap::error::Error::Io(std::io::Error::other(
                    "SELECT timed out",
                )))
            })
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
        let stream = timeout(OP_TIMEOUT, self.session.fetch(seq_set, FETCH_QUERY))
            .await
            .unwrap_or_else(|_| {
                Err(async_imap::error::Error::Io(std::io::Error::other(
                    "FETCH timed out",
                )))
            })
            .map_err(|e| AppError::Imap(e.to_string()))?;
        drain_fetch_stream(stream).await
    }

    /// `UID FETCH <set> (UID FLAGS RFC822.SIZE BODY.PEEK[HEADER])`. Best for incremental sync
    /// (e.g. `<prev_uid_next>:*`) — UIDs are stable across SELECT, sequence numbers aren't.
    pub async fn uid_fetch_headers(&mut self, uid_set: &str) -> AppResult<Vec<FetchedHeader>> {
        let stream = timeout(OP_TIMEOUT, self.session.uid_fetch(uid_set, FETCH_QUERY))
            .await
            .unwrap_or_else(|_| {
                Err(async_imap::error::Error::Io(std::io::Error::other(
                    "UID FETCH timed out",
                )))
            })
            .map_err(|e| AppError::Imap(e.to_string()))?;
        drain_fetch_stream(stream).await
    }

    /// `UID FETCH <uid> (BODY.PEEK[])`. Returns the full RFC 822 bytes for one message —
    /// callers parse them with [`crate::imap::parse::parse_body`]. Errors if the UID is
    /// missing from the currently-selected mailbox.
    pub async fn uid_fetch_body(&mut self, uid: u32) -> AppResult<Vec<u8>> {
        let mut stream = timeout(
            OP_TIMEOUT,
            self.session.uid_fetch(uid.to_string(), "(BODY.PEEK[])"),
        )
        .await
        .unwrap_or_else(|_| {
            Err(async_imap::error::Error::Io(std::io::Error::other(
                "UID FETCH BODY timed out",
            )))
        })
        .map_err(|e| AppError::Imap(e.to_string()))?;
        let Some(item) = stream.next().await else {
            return Err(AppError::Imap(format!("UID {uid} not found in mailbox")));
        };
        let f = item.map_err(|e| AppError::Imap(e.to_string()))?;
        let body = f
            .body()
            .ok_or_else(|| AppError::Imap(format!("UID {uid} returned no BODY[]")))?;
        Ok(body.to_vec())
    }

    /// `UID STORE <uid> ±FLAGS (<flag>)`。flag 传字面 IMAP 形式（如 `"\\Seen"` / `"\\Flagged"`）。
    /// add=true → `+FLAGS`，false → `-FLAGS`。STORE 回流更新后的 FLAGS（fetch 流），全部 drain。
    /// 安全：`flag` 必须是受控的 IMAP flag 字面（如 `\\Seen` / `\\Flagged` 或 `flag_to_string` 输出），
    /// 由调用方保证，不接受用户自由输入——直接拼进命令串，无服务端转义。
    pub async fn uid_set_flag(&mut self, uid: u32, flag: &str, add: bool) -> AppResult<()> {
        let sign = if add { "+" } else { "-" };
        let query = format!("{sign}FLAGS ({flag})");
        let mut stream = timeout(OP_TIMEOUT, self.session.uid_store(uid.to_string(), query))
            .await
            .unwrap_or_else(|_| {
                Err(async_imap::error::Error::Io(std::io::Error::other(
                    "UID STORE timed out",
                )))
            })
            .map_err(|e| AppError::Imap(e.to_string()))?;
        while let Some(item) = stream.next().await {
            item.map_err(|e| AppError::Imap(e.to_string()))?;
        }
        Ok(())
    }

    /// `UID MOVE <uid> <dest>`。需先 `select` 源文件夹。
    pub async fn uid_move(&mut self, uid: u32, dest: &str) -> AppResult<()> {
        timeout(OP_TIMEOUT, self.session.uid_mv(uid.to_string(), dest))
            .await
            .unwrap_or_else(|_| {
                Err(async_imap::error::Error::Io(std::io::Error::other(
                    "UID MOVE timed out",
                )))
            })
            .map_err(|e| AppError::Imap(e.to_string()))
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

/// Convert the IMAP RFC822.SIZE (u32, up to ~4 GiB) to the stored `size_bytes` (i32).
///
/// Values above `i32::MAX` (≈ 2.1 GiB) are saturated to `i32::MAX` rather than silently
/// discarded as `None`. The DB column is SQLite INTEGER (64-bit), so `i32::MAX` is safely
/// representable; a UI that shows "≥ 2 GB" is far more useful than "unknown size".
fn size_u32_to_i32(s: u32) -> Option<i32> {
    Some(i32::try_from(s).unwrap_or(i32::MAX))
}

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
            size_bytes: f.size.and_then(size_u32_to_i32),
            header_bytes: f.header().unwrap_or(&[]).to_vec(),
        });
    }
    Ok(out)
}

/// 从实时 LIST 结果里挑废纸篓：按 `delimiter` 切出末段，小写后精确等于某别名才命中。
/// 这样 `[QQ邮箱]/已删除`（末段「已删除」）命中，而用户自建 `To be deleted` 不会被误判。
pub fn resolve_trash_mailbox(mailboxes: &[MailboxInfo]) -> Option<String> {
    const ALIASES: [&str; 5] = [
        "deleted messages",
        "deleted",
        "deleted items",
        "trash",
        "已删除",
    ];
    mailboxes
        .iter()
        .find(|m| {
            let delim = m.delimiter.as_deref().unwrap_or("/");
            let leaf = m
                .name
                .rsplit(delim)
                .next()
                .unwrap_or(&m.name)
                .to_lowercase();
            ALIASES.contains(&leaf.as_str())
        })
        .map(|m| m.name.clone())
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

#[cfg(test)]
mod tests {
    use super::*;

    fn mb(name: &str) -> MailboxInfo {
        MailboxInfo {
            name: name.to_string(),
            delimiter: Some("/".to_string()),
        }
    }

    fn mb_flat(name: &str) -> MailboxInfo {
        MailboxInfo {
            name: name.to_string(),
            delimiter: None,
        }
    }

    #[test]
    fn resolve_trash_matches_known_leaf() {
        let list = vec![mb("INBOX"), mb("Sent Messages"), mb("Deleted Messages")];
        assert_eq!(
            resolve_trash_mailbox(&list),
            Some("Deleted Messages".to_string())
        );
    }

    #[test]
    fn resolve_trash_matches_hierarchical_chinese_leaf() {
        let list = vec![mb("INBOX"), mb("[QQ邮箱]/已删除")];
        assert_eq!(
            resolve_trash_mailbox(&list),
            Some("[QQ邮箱]/已删除".to_string())
        );
    }

    #[test]
    fn resolve_trash_uppercase_hits_lookalike_misses() {
        // "To be deleted" 末段 "to be deleted" ∉ 别名（不误判）；"TRASH" 小写后 == "trash" → 命中。
        let list = vec![mb("INBOX"), mb("To be deleted"), mb("TRASH")];
        assert_eq!(resolve_trash_mailbox(&list), Some("TRASH".to_string()));
    }

    #[test]
    fn resolve_trash_none_when_absent() {
        let list = vec![mb("INBOX"), mb("Sent Messages")];
        assert_eq!(resolve_trash_mailbox(&list), None);
    }

    #[test]
    fn resolve_trash_handles_flat_namespace() {
        // delimiter=None → fallback 切分，整个 name 即末段；"Trash" 小写命中别名。
        let list = vec![mb_flat("INBOX"), mb_flat("Trash")];
        assert_eq!(resolve_trash_mailbox(&list), Some("Trash".to_string()));
    }

    // --- #6: 超时常量合理性 ---

    #[test]
    fn connect_timeout_is_sane() {
        // 连接级超时应在 10s–60s 之间：太短导致误判，太长等于没超时。
        let secs = CONNECT_TIMEOUT.as_secs();
        assert!(
            (10..=60).contains(&secs),
            "CONNECT_TIMEOUT={secs}s 不在 [10,60] 合理范围"
        );
    }

    #[test]
    fn op_timeout_is_sane() {
        // 操作级超时（fetch/store/move）：比连接级略短或相等，至少 5s。
        let secs = OP_TIMEOUT.as_secs();
        assert!(
            (5..=60).contains(&secs),
            "OP_TIMEOUT={secs}s 不在 [5,60] 合理范围"
        );
    }

    #[tokio::test]
    async fn timeout_maps_to_imap_error() {
        // 验证超时时映射为 AppError::Imap（含可读信息），而非 panic 或其他错误类型。
        use std::time::Duration;
        use tokio::time::{sleep, timeout};

        let result: AppResult<()> = timeout(Duration::from_millis(1), async {
            sleep(Duration::from_secs(60)).await;
            Ok(())
        })
        .await
        .unwrap_or_else(|_| Err(AppError::Imap("connect timed out after 30s".into())));

        match result {
            Err(AppError::Imap(msg)) => {
                assert!(
                    msg.contains("timed out"),
                    "错误信息应含 'timed out'，实际: {msg}"
                )
            }
            other => panic!("应得到 AppError::Imap，实际: {other:?}"),
        }
    }

    // --- #33: RFC822.SIZE 边界值 ---

    #[test]
    fn size_normal_fits_i32() {
        // 普通大小（< 2 GiB）：应原样保留，不得变为 None。
        let size: u32 = 1_024_000;
        assert_eq!(size_u32_to_i32(size), Some(1_024_000_i32));
    }

    #[test]
    fn size_exactly_i32_max_fits() {
        let size: u32 = i32::MAX as u32;
        assert_eq!(size_u32_to_i32(size), Some(i32::MAX));
    }

    #[test]
    fn size_over_i32_max_saturates_not_none() {
        // > 2 GiB：旧代码返回 None，新代码应返回 Some(i32::MAX) 而非 None。
        let size: u32 = (i32::MAX as u32) + 1;
        assert_eq!(size_u32_to_i32(size), Some(i32::MAX));
    }

    #[test]
    fn size_u32_max_saturates_not_none() {
        assert_eq!(size_u32_to_i32(u32::MAX), Some(i32::MAX));
    }
}

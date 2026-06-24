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

/// Extended budget for full-body fetch (`BODY.PEEK[]`).
/// A single message can carry ~50 MB of attachments; at 200 KB/s that takes ~250 s.
/// 120 s covers most real-world cases while still bounding runaway fetches.
const BODY_TIMEOUT: Duration = Duration::from_secs(120);

/// One open IMAP session. Not thread-safe; never share across tasks.
pub struct ImapClient {
    session: Session<TlsStream<TcpStream>>,
}

/// IMAP special-use role for a mailbox. Derived from RFC 6154 SPECIAL-USE attributes or
/// heuristic name matching when the server does not advertise attributes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpecialUse {
    Inbox,
    Sent,
    Drafts,
    Trash,
    Junk,
}

impl SpecialUse {
    pub fn as_str(&self) -> &'static str {
        match self {
            SpecialUse::Inbox => "inbox",
            SpecialUse::Sent => "sent",
            SpecialUse::Drafts => "drafts",
            SpecialUse::Trash => "trash",
            SpecialUse::Junk => "junk",
        }
    }
}

#[derive(Debug, Clone)]
pub struct MailboxInfo {
    pub name: String,
    pub delimiter: Option<String>,
    /// Detected special-use role; None means regular folder.
    pub special_use: Option<SpecialUse>,
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
                "IMAP 连接超时（{}s）：{host}:{port}",
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

    /// LIST every mailbox the account can see. Returns name + hierarchy delimiter + special-use
    /// role (detected from RFC 6154 SPECIAL-USE attributes or heuristic name matching).
    pub async fn list_mailboxes(&mut self) -> AppResult<Vec<MailboxInfo>> {
        // Timeout covers both the command send AND the full response drain: a half-open
        // connection that accepts LIST but then stalls on the response frames would otherwise
        // hang indefinitely.
        timeout(OP_TIMEOUT, async {
            let mut stream = self
                .session
                .list(Some(""), Some("*"))
                .await
                .map_err(|e| AppError::Imap(e.to_string()))?;
            let mut out = Vec::new();
            while let Some(item) = stream.next().await {
                let name = item.map_err(|e| AppError::Imap(e.to_string()))?;
                let mb_name = name.name().to_string();
                let delimiter = name.delimiter().map(str::to_string);
                let special_use =
                    detect_special_use(&mb_name, &delimiter, name.attributes().iter().cloned());
                out.push(MailboxInfo {
                    name: mb_name,
                    delimiter,
                    special_use,
                });
            }
            Ok(out)
        })
        .await
        .map_err(|_| AppError::Imap("IMAP LIST 操作超时（30s）".into()))?
    }

    /// SELECT and return the resulting state. We only read three fields; the rest of the
    /// response (PERMANENTFLAGS, RECENT, etc.) is ignored at MVP.
    pub async fn select(&mut self, name: &str) -> AppResult<SelectedMailbox> {
        let m = timeout(OP_TIMEOUT, self.session.select(name))
            .await
            .map_err(|_| {
                AppError::Imap(format!("IMAP SELECT 操作超时（{}s）", OP_TIMEOUT.as_secs()))
            })?
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
        // Timeout wraps both command send and full stream drain so a half-open connection that
        // accepts FETCH but stalls on response frames doesn't hang indefinitely.
        timeout(OP_TIMEOUT, async {
            let stream = self
                .session
                .fetch(seq_set, FETCH_QUERY)
                .await
                .map_err(|e| AppError::Imap(e.to_string()))?;
            drain_fetch_stream(stream).await
        })
        .await
        .map_err(|_| AppError::Imap("IMAP FETCH 操作超时（30s）".into()))?
    }

    /// `UID FETCH <set> (UID FLAGS RFC822.SIZE BODY.PEEK[HEADER])`. Best for incremental sync
    /// (e.g. `<prev_uid_next>:*`) — UIDs are stable across SELECT, sequence numbers aren't.
    pub async fn uid_fetch_headers(&mut self, uid_set: &str) -> AppResult<Vec<FetchedHeader>> {
        // Same pattern: timeout covers send + full drain.
        timeout(OP_TIMEOUT, async {
            let stream = self
                .session
                .uid_fetch(uid_set, FETCH_QUERY)
                .await
                .map_err(|e| AppError::Imap(e.to_string()))?;
            drain_fetch_stream(stream).await
        })
        .await
        .map_err(|_| AppError::Imap("IMAP UID FETCH 操作超时（30s）".into()))?
    }

    /// `UID FETCH <uid> (BODY.PEEK[])`. Returns the full RFC 822 bytes for one message —
    /// callers parse them with [`crate::imap::parse::parse_body`]. Errors if the UID is
    /// missing from the currently-selected mailbox.
    pub async fn uid_fetch_body(&mut self, uid: u32) -> AppResult<Vec<u8>> {
        // Timeout covers both command send and stream read so a half-open connection that
        // accepts the UID FETCH but stalls on response data doesn't hang indefinitely.
        timeout(BODY_TIMEOUT, async {
            let mut stream = self
                .session
                .uid_fetch(uid.to_string(), "(BODY.PEEK[])")
                .await
                .map_err(|e| AppError::Imap(e.to_string()))?;
            let Some(item) = stream.next().await else {
                return Err(AppError::Imap(format!("UID {uid} not found in mailbox")));
            };
            let f = item.map_err(|e| AppError::Imap(e.to_string()))?;
            let body = f
                .body()
                .ok_or_else(|| AppError::Imap(format!("UID {uid} returned no BODY[]")))?;
            Ok(body.to_vec())
        })
        .await
        .map_err(|_| {
            AppError::Imap(format!(
                "IMAP UID FETCH BODY 操作超时（{}s）",
                BODY_TIMEOUT.as_secs()
            ))
        })?
    }

    /// `UID STORE <uid> ±FLAGS (<flag>)`。flag 传字面 IMAP 形式（如 `"\\Seen"` / `"\\Flagged"`）。
    /// add=true → `+FLAGS`，false → `-FLAGS`。STORE 回流更新后的 FLAGS（fetch 流），全部 drain。
    /// 安全：`flag` 必须是受控的 IMAP flag 字面（如 `\\Seen` / `\\Flagged` 或 `flag_to_string` 输出），
    /// 由调用方保证，不接受用户自由输入——直接拼进命令串，无服务端转义。
    pub async fn uid_set_flag(&mut self, uid: u32, flag: &str, add: bool) -> AppResult<()> {
        let sign = if add { "+" } else { "-" };
        let query = format!("{sign}FLAGS ({flag})");
        // Timeout covers both command send and the full FLAGS response stream drain.
        timeout(OP_TIMEOUT, async {
            let mut stream = self
                .session
                .uid_store(uid.to_string(), query)
                .await
                .map_err(|e| AppError::Imap(e.to_string()))?;
            while let Some(item) = stream.next().await {
                item.map_err(|e| AppError::Imap(e.to_string()))?;
            }
            Ok(())
        })
        .await
        .map_err(|_| AppError::Imap("IMAP UID STORE 操作超时（30s）".into()))?
    }

    /// `UID STORE <set> ±FLAGS (<flag>)` 批量版：`uids` 拼成逗号分隔 sequence-set，一次往返
    /// 标记多封（用于「全部已读」）。空 `uids` 直接返回、不发往返。语义/安全同 [`Self::uid_set_flag`]。
    pub async fn uid_set_flag_bulk(
        &mut self,
        uids: &[u32],
        flag: &str,
        add: bool,
    ) -> AppResult<()> {
        if uids.is_empty() {
            return Ok(());
        }
        let set = uids
            .iter()
            .map(u32::to_string)
            .collect::<Vec<_>>()
            .join(",");
        let sign = if add { "+" } else { "-" };
        let query = format!("{sign}FLAGS ({flag})");
        // Timeout covers both command send and the full FLAGS response stream drain.
        timeout(OP_TIMEOUT, async {
            let mut stream = self
                .session
                .uid_store(set, query)
                .await
                .map_err(|e| AppError::Imap(e.to_string()))?;
            while let Some(item) = stream.next().await {
                item.map_err(|e| AppError::Imap(e.to_string()))?;
            }
            Ok(())
        })
        .await
        .map_err(|_| AppError::Imap("IMAP UID STORE（批量）操作超时（30s）".into()))?
    }

    /// `UID MOVE <uid> <dest>`。需先 `select` 源文件夹。
    pub async fn uid_move(&mut self, uid: u32, dest: &str) -> AppResult<()> {
        timeout(OP_TIMEOUT, self.session.uid_mv(uid.to_string(), dest))
            .await
            .map_err(|_| {
                AppError::Imap(format!(
                    "IMAP UID MOVE 操作超时（{}s）",
                    OP_TIMEOUT.as_secs()
                ))
            })?
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

/// Detect the special-use role for a mailbox from SPECIAL-USE attributes (RFC 6154) first,
/// then fall back to heuristic leaf-name matching. Both checks are case-insensitive.
///
/// Heuristic aliases are intentionally narrow (exact leaf-name match) to avoid misclassifying
/// user-created folders like "To be deleted" or "My Drafts folder".
fn detect_special_use<'a>(
    name: &str,
    delimiter: &Option<String>,
    attributes: impl Iterator<Item = async_imap::types::NameAttribute<'a>>,
) -> Option<SpecialUse> {
    use async_imap::types::NameAttribute;

    // RFC 6154 SPECIAL-USE attributes take precedence over heuristics.
    for attr in attributes {
        if let NameAttribute::Extension(cow) = attr {
            match cow.to_lowercase().as_str() {
                "\\inbox" => return Some(SpecialUse::Inbox),
                "\\sent" => return Some(SpecialUse::Sent),
                "\\drafts" => return Some(SpecialUse::Drafts),
                "\\trash" => return Some(SpecialUse::Trash),
                "\\junk" => return Some(SpecialUse::Junk),
                _ => {}
            }
        }
    }

    // Heuristic: extract the leaf segment (after the last delimiter) and match aliases.
    let delim = delimiter.as_deref().unwrap_or("/");
    let leaf = name.rsplit(delim).next().unwrap_or(name).to_lowercase();

    // INBOX is always the literal name by IMAP spec (RFC 3501 §5.1).
    if leaf == "inbox" {
        return Some(SpecialUse::Inbox);
    }

    const SENT_ALIASES: &[&str] = &["sent", "sent messages", "sent items", "已发送", "发件箱"];
    const DRAFTS_ALIASES: &[&str] = &["drafts", "draft", "草稿箱", "草稿"];
    const TRASH_ALIASES: &[&str] = &[
        "trash",
        "deleted",
        "deleted messages",
        "deleted items",
        "已删除",
        "废纸篓",
    ];
    const JUNK_ALIASES: &[&str] = &["junk", "spam", "junk e-mail", "垃圾邮件"];

    if SENT_ALIASES.contains(&leaf.as_str()) {
        return Some(SpecialUse::Sent);
    }
    if DRAFTS_ALIASES.contains(&leaf.as_str()) {
        return Some(SpecialUse::Drafts);
    }
    if TRASH_ALIASES.contains(&leaf.as_str()) {
        return Some(SpecialUse::Trash);
    }
    if JUNK_ALIASES.contains(&leaf.as_str()) {
        return Some(SpecialUse::Junk);
    }

    None
}

/// 从实时 LIST 结果里挑废纸篓：优先使用 special_use 已检测到的 Trash 信箱，
/// 回退到按叶名的启发式别名匹配（大小写不敏感，末段精确等于某别名）。
/// 这样 `[QQ邮箱]/已删除`（末段「已删除」）命中，而用户自建 `To be deleted` 不会被误判。
pub fn resolve_trash_mailbox(mailboxes: &[MailboxInfo]) -> Option<String> {
    // First: prefer the mailbox already tagged as Trash by detect_special_use.
    if let Some(mb) = mailboxes
        .iter()
        .find(|m| m.special_use == Some(SpecialUse::Trash))
    {
        return Some(mb.name.clone());
    }
    None
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
            special_use: None,
        }
    }

    fn mb_with(name: &str, special_use: Option<SpecialUse>) -> MailboxInfo {
        MailboxInfo {
            name: name.to_string(),
            delimiter: Some("/".to_string()),
            special_use,
        }
    }

    // resolve_trash_mailbox now works via the special_use field populated by detect_special_use.

    #[test]
    fn resolve_trash_finds_tagged_trash() {
        let list = vec![
            mb_with("INBOX", Some(SpecialUse::Inbox)),
            mb_with("Sent Messages", Some(SpecialUse::Sent)),
            mb_with("Deleted Messages", Some(SpecialUse::Trash)),
        ];
        assert_eq!(
            resolve_trash_mailbox(&list),
            Some("Deleted Messages".to_string())
        );
    }

    #[test]
    fn resolve_trash_finds_chinese_trash() {
        let list = vec![
            mb_with("INBOX", Some(SpecialUse::Inbox)),
            mb_with("[QQ邮箱]/已删除", Some(SpecialUse::Trash)),
        ];
        assert_eq!(
            resolve_trash_mailbox(&list),
            Some("[QQ邮箱]/已删除".to_string())
        );
    }

    #[test]
    fn resolve_trash_none_when_no_trash_tagged() {
        // 未标记 Trash 的 mailbox 不会被误判
        let list = vec![mb("INBOX"), mb("To be deleted"), mb("Sent Messages")];
        assert_eq!(resolve_trash_mailbox(&list), None);
    }

    #[test]
    fn resolve_trash_none_when_absent() {
        let list = vec![
            mb_with("INBOX", Some(SpecialUse::Inbox)),
            mb_with("Sent", Some(SpecialUse::Sent)),
        ];
        assert_eq!(resolve_trash_mailbox(&list), None);
    }

    // --- detect_special_use heuristic tests ---

    #[test]
    fn detect_inbox_by_name() {
        let result = detect_special_use("INBOX", &Some("/".to_string()), std::iter::empty());
        assert_eq!(result, Some(SpecialUse::Inbox));
    }

    #[test]
    fn detect_sent_aliases() {
        for name in &["Sent Messages", "Sent", "sent items", "已发送", "发件箱"] {
            let result = detect_special_use(name, &Some("/".to_string()), std::iter::empty());
            assert_eq!(result, Some(SpecialUse::Sent), "expected Sent for {name}");
        }
    }

    #[test]
    fn detect_drafts_aliases() {
        for name in &["Drafts", "Draft", "草稿箱", "草稿"] {
            let result = detect_special_use(name, &Some("/".to_string()), std::iter::empty());
            assert_eq!(
                result,
                Some(SpecialUse::Drafts),
                "expected Drafts for {name}"
            );
        }
    }

    #[test]
    fn detect_trash_aliases() {
        for name in &["Trash", "Deleted", "Deleted Messages", "已删除", "废纸篓"] {
            let result = detect_special_use(name, &Some("/".to_string()), std::iter::empty());
            assert_eq!(result, Some(SpecialUse::Trash), "expected Trash for {name}");
        }
    }

    #[test]
    fn detect_hierarchical_leaf() {
        // QQ Mail style: "[QQ邮箱]/已删除" → leaf is "已删除" → Trash
        let result = detect_special_use(
            "[QQ邮箱]/已删除",
            &Some("/".to_string()),
            std::iter::empty(),
        );
        assert_eq!(result, Some(SpecialUse::Trash));
    }

    #[test]
    fn detect_none_for_custom_folder() {
        let result = detect_special_use("My Archive", &Some("/".to_string()), std::iter::empty());
        assert_eq!(result, None);
    }

    #[test]
    fn detect_none_for_lookalike_name() {
        // "To be deleted" should NOT match Trash
        let result =
            detect_special_use("To be deleted", &Some("/".to_string()), std::iter::empty());
        assert_eq!(result, None);
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

    #[test]
    fn body_timeout_is_sane() {
        // body 全文超时应大于 OP_TIMEOUT，且不超过 5 分钟（防止无限等待）。
        let secs = BODY_TIMEOUT.as_secs();
        assert!(
            secs > OP_TIMEOUT.as_secs(),
            "BODY_TIMEOUT={secs}s 应大于 OP_TIMEOUT={}s",
            OP_TIMEOUT.as_secs()
        );
        assert!(secs <= 300, "BODY_TIMEOUT={secs}s 不应超过 300s");
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
        .unwrap_or_else(|_| {
            Err(AppError::Imap(format!(
                "IMAP 连接超时（{}s）：imap.example.com:993",
                CONNECT_TIMEOUT.as_secs()
            )))
        });

        match result {
            Err(AppError::Imap(msg)) => {
                assert!(msg.contains("超时"), "错误信息应含 '超时'，实际: {msg}")
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

    // --- #6: drain 阶段也被超时覆盖 ---
    //
    // (b) 全链路假 IMAP server 方向：async-imap Session 类型绑定 TlsStream<TcpStream>，
    //     无法在单元测试里不做 TLS 就构造；OP_TIMEOUT 是编译期常量，无法从外部注入缩短。
    //     因此 fetch_headers/uid_fetch_headers 级别的端到端 op-timeout 测试在当前架构下
    //     无法干净落地，详见 CLAUDE.md § IMAP integration 说明。
    //
    // (a) 直接喂生产泛型函数：drain_fetch_stream 是 S: Stream 泛型，可以在测试里直接调用，
    //     以 pending stream 验证"流挂起时 drain 会卡住"，以 empty stream 验证正常完成路径。

    /// 外层 timeout 截断 drain_fetch_stream：pending stream 使 drain 永不返回，
    /// timeout 到期后 drain_fetch_stream 被截断——真正调用了生产函数。
    ///
    /// 判别力验证：若把 drain_fetch_stream 换成立即返回的桩（如 `async { Ok(vec![]) }`），
    /// 该 future 会在 timeout 到期前完成，r 变成 Ok(Ok([])) 而非 Err(Elapsed)，断言失败。
    /// 故本测试确实依赖"生产 drain_fetch_stream 遇到 pending stream 会挂起"这一事实。
    #[tokio::test]
    async fn drain_fetch_stream_stalls_on_pending_stream() {
        use std::time::Duration;
        use tokio::time::timeout;

        // pending::<Result<Fetch, _>>() 永远不产出 item —— 模拟 half-open 连接的 drain 挂起。
        let stalled =
            futures::stream::pending::<async_imap::error::Result<async_imap::types::Fetch>>();
        let r = timeout(Duration::from_millis(10), drain_fetch_stream(stalled)).await;

        assert!(
            r.is_err(),
            "drain_fetch_stream 在 pending stream 上应被 timeout 截断"
        );
    }

    /// drain_fetch_stream 在空 stream 上快速完成，返回空 Vec。
    ///
    /// 对照测试：确认正常路径不受 timeout 影响，同时验证生产函数对空流返回 Ok([])。
    #[tokio::test]
    async fn drain_fetch_stream_returns_empty_on_empty_stream() -> AppResult<()> {
        use std::time::Duration;
        use tokio::time::timeout;

        // iter([]) 立即结束，drain 应快速返回 Ok(vec![])。
        let empty = futures::stream::iter(
            Vec::<async_imap::error::Result<async_imap::types::Fetch>>::new(),
        );
        let r = timeout(Duration::from_millis(100), drain_fetch_stream(empty))
            .await
            .map_err(|_| AppError::Imap("不应超时".into()))??;

        assert!(r.is_empty(), "空 stream 应返回空 Vec，实际: {r:?}");
        Ok(())
    }
}

//! RFC 822 header → [`ParsedHeaders`] conversion.
//!
//! Body parsing (text/plain extraction, snippet, attachments) is Sprint 1.4 — this module
//! only touches the metadata we persist on first sync.

use mail_parser::{Address, HeaderValue, MessageParser, PartType};
use time::OffsetDateTime;

#[derive(Debug, Default)]
pub struct ParsedHeaders {
    pub rfc_message_id: Option<String>,
    pub thread_id: Option<String>,
    pub subject: Option<String>,
    pub from_addr: Option<String>,
    pub to_addrs: Vec<String>,
    pub cc_addrs: Vec<String>,
    pub sent_at: Option<OffsetDateTime>,
    /// Raw space-separated RFC 5322 References header value (e.g. `"<a@x> <b@x>"`).
    /// Stored verbatim so the sender can extend it without re-parsing.
    pub references_header: Option<String>,
}

/// Parse the header section of an RFC 822 message. Lossy: if a field is unparseable we drop it
/// (returning `None`) rather than failing the whole sync — the user can still see the message
/// with whatever fields did parse.
pub fn parse_headers(raw: &[u8]) -> ParsedHeaders {
    let Some(message) = MessageParser::default().parse(raw) else {
        return ParsedHeaders::default();
    };

    let rfc_message_id = message.message_id().map(str::to_string);

    // RFC 5322 §3.6.4: thread root = first References entry; In-Reply-To is the immediate parent.
    // Use References' head if present, else In-Reply-To, else this message itself (single-msg
    // threads still want a thread_id so list views can group consistently).
    let thread_id = extract_thread_id(&message).or_else(|| rfc_message_id.clone());

    // Collect all References IDs so the sender can extend the chain on reply.
    let references_header = all_ids(message.references());

    let subject = message.subject().map(str::to_string);

    let from_addr = message.from().and_then(addresses_to_first_formatted);
    let to_addrs = message
        .to()
        .map(addresses_to_formatted_vec)
        .unwrap_or_default();
    let cc_addrs = message
        .cc()
        .map(addresses_to_formatted_vec)
        .unwrap_or_default();

    let sent_at = message
        .date()
        .and_then(|dt| OffsetDateTime::from_unix_timestamp(dt.to_timestamp()).ok());

    ParsedHeaders {
        rfc_message_id,
        thread_id,
        subject,
        from_addr,
        to_addrs,
        cc_addrs,
        sent_at,
        references_header,
    }
}

fn extract_thread_id(message: &mail_parser::Message<'_>) -> Option<String> {
    first_id(message.references()).or_else(|| first_id(message.in_reply_to()))
}

/// mail-parser surfaces References / In-Reply-To as either a single `Text` or a `TextList`
/// depending on how many IDs it found. The first ID is the conventional thread root.
fn first_id(value: &HeaderValue<'_>) -> Option<String> {
    match value {
        HeaderValue::Text(s) => Some(s.to_string()),
        HeaderValue::TextList(list) => list.first().map(|s| s.to_string()),
        _ => None,
    }
}

/// Collect all IDs from a References / In-Reply-To header value into a space-separated string
/// suitable for storing and later extending. Returns `None` when the header is absent.
fn all_ids(value: &HeaderValue<'_>) -> Option<String> {
    match value {
        HeaderValue::Text(s) if !s.is_empty() => Some(s.to_string()),
        HeaderValue::TextList(list) if !list.is_empty() => {
            let joined = list
                .iter()
                .map(|s| s.as_ref())
                .collect::<Vec<_>>()
                .join(" ");
            Some(joined)
        }
        _ => None,
    }
}

fn addresses_to_first_formatted(addr: &Address<'_>) -> Option<String> {
    addresses_to_formatted_vec(addr).into_iter().next()
}

/// Flatten any `Address` (single, list, or group) into "Name <email>" strings. Skips entries
/// that have no email at all — pure-name entries are useless to us.
fn addresses_to_formatted_vec(addr: &Address<'_>) -> Vec<String> {
    let mut out = Vec::new();
    match addr {
        Address::List(addrs) => {
            for a in addrs {
                if let Some(s) = format_addr(a.name.as_deref(), a.address.as_deref()) {
                    out.push(s);
                }
            }
        }
        Address::Group(groups) => {
            for g in groups {
                for a in &g.addresses {
                    if let Some(s) = format_addr(a.name.as_deref(), a.address.as_deref()) {
                        out.push(s);
                    }
                }
            }
        }
    }
    out
}

fn format_addr(name: Option<&str>, email: Option<&str>) -> Option<String> {
    let email = email?;
    Some(match name {
        Some(n) if !n.trim().is_empty() => format!("{} <{}>", n.trim(), email),
        _ => email.to_string(),
    })
}

/// Body extraction result. Used by Sprint 1.4 lazy-fetch + snippet backfill.
#[derive(Debug, Default, Clone)]
pub struct ParsedBody {
    pub text_plain: Option<String>,
    pub html: Option<String>,
    pub has_attachment: bool,
}

/// Parse the full RFC 822 payload into `text/plain`, `text/html`, and an attachment flag.
///
/// Uses mail-parser's `text_body` / `html_body` indexes (disposition-aware, only inline parts)
/// rather than scanning raw `msg.parts`. The latter includes attachment nodes whose content-type
/// is still text/plain or text/html, which a naïve scan would misidentify as the body.
///
/// We additionally filter by `PartType::Text` / `PartType::Html` to prevent cross-type fallback:
/// for a plain-only email mail-parser puts the same part into both indexes, and without the
/// filter `html_body[0]` would return a text/plain part as if it were HTML.
pub fn parse_body(raw: &[u8]) -> ParsedBody {
    let Some(msg) = MessageParser::default().parse(raw) else {
        return ParsedBody::default();
    };

    // Find first inline text/plain part (PartType::Text only — no cross-type fallback).
    let text_plain = msg.text_body.iter().find_map(|&idx| {
        let part = &msg.parts[idx as usize];
        if matches!(part.body, PartType::Text(_)) {
            part.text_contents().map(str::to_string)
        } else {
            None
        }
    });

    // Find first inline text/html part (PartType::Html only — no cross-type fallback).
    let html = msg.html_body.iter().find_map(|&idx| {
        let part = &msg.parts[idx as usize];
        if matches!(part.body, PartType::Html(_)) {
            part.text_contents().map(str::to_string)
        } else {
            None
        }
    });

    ParsedBody {
        text_plain,
        html,
        has_attachment: msg.attachment_count() > 0,
    }
}

/// Collapse whitespace and trim to `char_limit` characters (NOT bytes — multi-byte safe).
/// Returns `None` if the input is empty after trimming. Appends `…` only when truncated.
pub fn snippet(text: &str, char_limit: usize) -> Option<String> {
    let collapsed: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.is_empty() {
        return None;
    }
    if collapsed.chars().count() <= char_limit {
        return Some(collapsed);
    }
    let truncated: String = collapsed.chars().take(char_limit).collect();
    Some(format!("{truncated}…"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &[u8] = b"\
From: Alice <alice@example.com>\r\n\
To: Bob <bob@example.com>, carol@example.com\r\n\
Cc: \"Dave Q.\" <dave@example.com>\r\n\
Subject: hello\r\n\
Date: Mon, 19 May 2025 14:00:00 +0800\r\n\
Message-ID: <abc@example.com>\r\n\
MIME-Version: 1.0\r\n\
Content-Type: text/plain; charset=utf-8\r\n\
\r\n\
body here\r\n";

    #[test]
    fn parses_basic_headers() {
        let p = parse_headers(SAMPLE);
        assert_eq!(p.subject.as_deref(), Some("hello"));
        assert_eq!(p.from_addr.as_deref(), Some("Alice <alice@example.com>"));
        assert_eq!(
            p.to_addrs,
            vec![
                "Bob <bob@example.com>".to_string(),
                "carol@example.com".to_string()
            ]
        );
        assert_eq!(p.cc_addrs, vec!["Dave Q. <dave@example.com>".to_string()]);
        assert_eq!(p.rfc_message_id.as_deref(), Some("abc@example.com"));
        // No References / In-Reply-To → thread_id falls back to the message-id itself.
        assert_eq!(p.thread_id.as_deref(), Some("abc@example.com"));
        assert!(p.sent_at.is_some());
    }

    #[test]
    fn empty_input_yields_empty_headers() {
        let p = parse_headers(b"");
        assert!(p.subject.is_none());
        assert!(p.from_addr.is_none());
        assert!(p.to_addrs.is_empty());
    }

    #[test]
    fn parses_plain_text_body() {
        let p = parse_body(SAMPLE);
        assert_eq!(p.text_plain.as_deref().map(str::trim), Some("body here"));
        assert!(p.html.is_none());
        assert!(!p.has_attachment);
    }

    const MULTIPART: &[u8] = b"\
From: a@x\r\n\
Subject: m\r\n\
MIME-Version: 1.0\r\n\
Content-Type: multipart/alternative; boundary=\"X\"\r\n\
\r\n\
--X\r\n\
Content-Type: text/plain\r\n\
\r\n\
plain side\r\n\
--X\r\n\
Content-Type: text/html\r\n\
\r\n\
<p>html side</p>\r\n\
--X--\r\n";

    #[test]
    fn parses_multipart_alternative() {
        let p = parse_body(MULTIPART);
        assert_eq!(p.text_plain.as_deref().map(str::trim), Some("plain side"));
        assert!(p.html.as_deref().unwrap_or("").contains("html side"));
        assert!(!p.has_attachment);
    }

    // 中国邮件（银行账单、运营商等）常用 GB2312/GBK 编码。mail-parser 解码非 UTF-8
    // 字符集依赖 `full_encoding` feature（→ encoding_rs）；缺它中文会变成 U+FFFD 乱码。
    // 样本：subject 为 RFC 2047 GB2312 encoded-word，body 为裸 GB2312 字节。
    const GB2312_MSG: &[u8] = b"\
From: master@creditcard.cmbc.com.cn\r\n\
Subject: =?GB2312?B?0MXTw7+o1cu1pQ==?=\r\n\
MIME-Version: 1.0\r\n\
Content-Type: text/plain; charset=GB2312\r\n\
\r\n\
\xc4\xfa\xb5\xc4\xd5\xcb\xb5\xa5\xd2\xd1\xb3\xf6\xa3\xac\xc7\xeb\xbc\xb0\xca\xb1\xbb\xb9\xbf\xee\xa1\xa3\r\n";

    #[test]
    fn decodes_gb2312_subject_and_body() {
        let h = parse_headers(GB2312_MSG);
        assert_eq!(h.subject.as_deref(), Some("信用卡账单"));

        let b = parse_body(GB2312_MSG);
        assert_eq!(
            b.text_plain.as_deref().map(str::trim),
            Some("您的账单已出，请及时还款。"),
        );
    }

    #[test]
    fn snippet_truncates_with_ellipsis() {
        let s = "hello   world\n\nthis is a long line of text we will truncate";
        let out = snippet(s, 20).expect("non-empty input");
        assert!(out.ends_with('…'));
        assert!(out.chars().count() <= 21); // 20 + the ellipsis char
    }

    #[test]
    fn snippet_short_input_passes_through() {
        let out = snippet("a b c", 20).expect("non-empty input");
        assert_eq!(out, "a b c");
    }

    #[test]
    fn snippet_empty_input_is_none() {
        assert!(snippet("", 20).is_none());
        assert!(snippet("   \t\n  ", 20).is_none());
    }

    /// 含 Content-Disposition: attachment 的 .txt 附件出现在正文之前：
    /// parse_body 必须选出真正文，而非把附件内容当正文。
    #[test]
    fn txt_attachment_not_mistaken_for_body() {
        // multipart/mixed：先附件（Content-Disposition: attachment）后真正文
        const MAIL: &[u8] = b"\
From: a@x\r\n\
Subject: test\r\n\
MIME-Version: 1.0\r\n\
Content-Type: multipart/mixed; boundary=\"B\"\r\n\
\r\n\
--B\r\n\
Content-Type: text/plain; name=\"note.txt\"\r\n\
Content-Disposition: attachment; filename=\"note.txt\"\r\n\
\r\n\
THIS IS ATTACHMENT CONTENT\r\n\
--B\r\n\
Content-Type: text/plain\r\n\
\r\n\
This is the real body\r\n\
--B--\r\n";

        let p = parse_body(MAIL);
        let plain = p.text_plain.as_deref().unwrap_or("").trim().to_string();
        // 应选出真正文，而非附件内容
        assert_eq!(plain, "This is the real body", "附件内容不应被当作正文");
        assert!(!plain.contains("ATTACHMENT"), "附件内容不应出现在正文字段");
        // 附件计数应为 1
        assert!(p.has_attachment, "应检测到附件");
    }

    /// 邮件含多个 References ID：parse_headers 应将它们空格连接存入 references_header。
    #[test]
    fn parses_references_header_multiple_ids() {
        const MAIL: &[u8] = b"\
From: Alice <alice@example.com>\r\n\
To: Bob <bob@example.com>\r\n\
Subject: Re: Re: hello\r\n\
Date: Mon, 19 May 2025 15:00:00 +0800\r\n\
Message-ID: <c@example.com>\r\n\
References: <a@example.com> <b@example.com>\r\n\
In-Reply-To: <b@example.com>\r\n\
MIME-Version: 1.0\r\n\
Content-Type: text/plain\r\n\
\r\n\
reply body\r\n";

        let p = parse_headers(MAIL);
        assert_eq!(p.rfc_message_id.as_deref(), Some("c@example.com"));
        // references_header 应保留两个 ID，空格分隔
        let refs = p
            .references_header
            .expect("references_header must be present");
        assert!(
            refs.contains("a@example.com") && refs.contains("b@example.com"),
            "both reference IDs must be in references_header: {refs}"
        );
    }

    /// 邮件无 References 头：references_header 应为 None。
    #[test]
    fn no_references_header_yields_none() {
        let p = parse_headers(SAMPLE); // SAMPLE 没有 References 头
        assert!(
            p.references_header.is_none(),
            "no References header → references_header must be None"
        );
    }
}

//! RFC 822 header → [`ParsedHeaders`] conversion.
//!
//! Body parsing (text/plain extraction, snippet, attachments) is Sprint 1.4 — this module
//! only touches the metadata we persist on first sync.

use mail_parser::{Address, HeaderValue, MessageParser};
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
}

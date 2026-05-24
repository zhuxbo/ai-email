//! SMTP send.
//!
//! Implicit-TLS (port 465) by default — that's QQ/163's mode. lettre's `AsyncSmtpTransport`
//! with `Tls::Wrapper` handles the implicit handshake; rustls + webpki-roots keep the TLS
//! story consistent with the rest of the crate.
//!
//! Every send writes a `send_log` row, even on transport failure — per SPEC § 9 audit
//! invariant. The row records the attempt with the SMTP error in `smtp_response`.

pub mod sender;

pub use sender::{send_draft, SendDraft, SendReceipt};

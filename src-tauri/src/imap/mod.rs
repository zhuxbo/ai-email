//! IMAP client + sync logic.
//!
//! Layout:
//!   • [`tls`]    — builds a rustls connector against the Mozilla root CA bundle.
//!   • [`client`] — thin wrapper around `async_imap::Session`, owns connection state.
//!   • [`parse`]  — header bytes → in-memory `ParsedHeaders`.
//!   • [`sync`]   — orchestrates one full INBOX sync (connect → list → select → fetch → persist).
//!
//! TLS is mandatory; we hardcode the `imaps://` profile (port 993 by default in the schema).
//! No STARTTLS fallback — providers we care about (QQ, 163, Gmail) all support implicit TLS.

pub mod client;
pub mod parse;
pub mod sync;
pub mod tls;

//! IMAP client + sync logic.
//!
//! Layout:
//!   • [`tls`]     — builds a rustls connector against the Mozilla root CA bundle.
//!   • [`client`]  — thin wrapper around `async_imap::Session`, owns connection state.
//!   • [`parse`]   — header bytes → in-memory `ParsedHeaders`.
//!   • [`sync`]    — orchestrates one full INBOX sync (connect → list → select → fetch → persist).
//!   • [`manager`] — per-account connection reuse (one long-lived session per account).
//!   • [`backoff`] — auto-sync cooldown after consecutive sync failures.
//!
//! TLS is mandatory; we hardcode the `imaps://` profile (port 993 by default in the schema).
//! No STARTTLS fallback — providers we care about (QQ, 163, Gmail) all support implicit TLS.

pub mod backoff;
pub mod client;
pub mod html_text;
pub mod manager;
pub mod materialize;
pub mod parse;
pub mod sync;
pub mod tls;

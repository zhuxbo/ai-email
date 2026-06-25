//! Tauri command handlers. Every cross-FFI call surface lives here.
//!
//! Naming: `snake_case` in Rust → automatically `camelCase` on the TS side.
//! Every command returns `Result<T, crate::error::AppError>`; the error serializes to a string
//! so the UI gets one shape to render.

pub mod accounts;
pub mod ai;
pub mod ai_config;
pub mod auto_reply;
pub mod mail;
pub mod system;

/// Decide which secret to persist to the keychain on an *update*.
///
/// Returns the value to store, or `None` when the field was absent or blank
/// (whitespace-only) — meaning "keep the existing credential unchanged". The
/// returned value is the original input, untrimmed, so the stored secret is
/// byte-for-byte what the user entered.
pub(crate) fn secret_to_store(input: Option<String>) -> Option<String> {
    input.filter(|s| !s.trim().is_empty())
}

#[cfg(test)]
mod tests {
    use super::secret_to_store;

    #[test]
    fn secret_to_store_skips_absent_and_blank() {
        assert_eq!(secret_to_store(None), None);
        assert_eq!(secret_to_store(Some(String::new())), None);
        assert_eq!(secret_to_store(Some("   ".into())), None);
        assert_eq!(secret_to_store(Some("\t\n ".into())), None);
    }

    #[test]
    fn secret_to_store_keeps_nonblank_untrimmed() {
        assert_eq!(
            secret_to_store(Some("code".into())),
            Some("code".to_string())
        );
        // Surrounding whitespace is preserved — only the blank check trims.
        assert_eq!(
            secret_to_store(Some("  abc  ".into())),
            Some("  abc  ".to_string())
        );
    }
}

//! rustls connector builder. Loaded once per sync; reuse across reconnects in the future.
//!
//! We use the Mozilla CA bundle from [`webpki_roots`] rather than the OS trust store so the
//! binary behaves identically on macOS, Windows, and Linux without pulling in `native-tls`.

use std::sync::Arc;

use rustls::ClientConfig;
use tokio_rustls::TlsConnector;

/// Build a fresh `TlsConnector` rooted at the Mozilla CA bundle. Cheap (no I/O), so callers
/// build a new one per connection.
pub fn build_connector() -> TlsConnector {
    let mut roots = rustls::RootCertStore::empty();
    roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    let config = ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();
    TlsConnector::from(Arc::new(config))
}

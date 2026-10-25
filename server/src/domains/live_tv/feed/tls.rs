// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::FeedError;
use rustls::{ClientConfig, RootCertStore};
use std::sync::Arc;
use tokio_rustls::TlsConnector;

pub(super) fn connector(fixture: bool) -> Result<TlsConnector, FeedError> {
    let roots = if fixture {
        let mut roots = RootCertStore::empty();
        roots
            .add(rustls::pki_types::CertificateDer::from(
                include_bytes!("../../../../tests/fixtures/iptv-feed/ca.der").to_vec(),
            ))
            .map_err(|_| FeedError::Connection)?;
        roots
    } else {
        RootCertStore::from_iter(webpki_roots::TLS_SERVER_ROOTS.iter().cloned())
    };
    let mut config =
        ClientConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
            .with_safe_default_protocol_versions()
            .map_err(|_| FeedError::Connection)?
            .with_root_certificates(roots)
            .with_no_client_auth();
    config.alpn_protocols = vec![b"http/1.1".to_vec()];
    config.resumption = rustls::client::Resumption::disabled();
    Ok(TlsConnector::from(Arc::new(config)))
}

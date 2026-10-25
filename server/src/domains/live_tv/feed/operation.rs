// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::{FeedError, connection, source};
use crate::{
    domains::live_tv::assignment::secrets::FeedField, services::encryption::EncryptionKey,
};
use sqlx::PgPool;
use std::time::Duration;
use tokio_rustls::TlsConnector;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

pub(super) async fn run(
    pool: &PgPool,
    key: &EncryptionKey,
    tls: &TlsConnector,
    user: Uuid,
    feed: FeedField,
    index: usize,
    cancel: &CancellationToken,
) -> Result<Vec<u8>, FeedError> {
    let work = async {
        let source = source::load(pool, key, user, feed, index).await?;
        source.fence.current(pool).await?;
        let monitor = async {
            loop {
                tokio::time::sleep(Duration::from_millis(200)).await;
                if let Err(error) = source.fence.current(pool).await {
                    break error;
                }
            }
        };
        let bytes = tokio::select! {
            biased;
            error = monitor => return Err(error),
            result = connection::fetch(&source, pool, tls) => result?,
        };
        source.fence.current(pool).await?;
        Ok(bytes)
    };
    tokio::select! {
        biased;
        _ = cancel.cancelled() => Err(FeedError::Cancelled),
        result = tokio::time::timeout(Duration::from_secs(15), work) => result.map_err(|_| FeedError::Deadline)?,
    }
}

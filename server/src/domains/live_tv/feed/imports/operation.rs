// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::super::{connection, source::Source};
use super::{
    ImportError, codec, compression,
    limits::{self, Work},
    m3u,
    model::{Candidate, Catalog},
    monitor::Monitor,
    publication,
    receipt::Receipt,
    xml,
};
use crate::services::encryption::EncryptionKey;
use sqlx::PgPool;
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use tokio_rustls::TlsConnector;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

#[cfg(feature = "playback-qualification")]
pub(super) async fn run(
    pool: &PgPool,
    key: Arc<EncryptionKey>,
    tls: &TlsConnector,
    user: Uuid,
    pins: (usize, usize),
    cancel: &CancellationToken,
) -> Result<Uuid, ImportError> {
    let deadline = Instant::now() + Duration::from_secs(180);
    if cancel.is_cancelled() {
        return Err(ImportError::Cancelled);
    }
    let playlist = super::super::source::load(
        pool,
        &key,
        user,
        crate::domains::live_tv::assignment::secrets::FeedField::Playlist,
        pins.0,
    )
    .await?;
    let guide = super::super::source::load(
        pool,
        &key,
        user,
        crate::domains::live_tv::assignment::secrets::FeedField::Guide,
        pins.1,
    )
    .await?;
    if playlist.fence != guide.fence {
        return Err(ImportError::Authority);
    }
    let receipt = super::receipt::claim(pool, playlist.fence).await?;
    execute(pool, key, tls, receipt, (playlist, guide), cancel, deadline).await
}

pub(crate) async fn execute(
    pool: &PgPool,
    key: Arc<EncryptionKey>,
    tls: &TlsConnector,
    receipt: Receipt,
    sources: (Source, Source),
    cancel: &CancellationToken,
    deadline: Instant,
) -> Result<Uuid, ImportError> {
    let (playlist, guide) = sources;
    let mut monitor = Monitor::start(pool.clone(), receipt);
    let result = async {
        let playlist = fetch(
            pool,
            tls,
            &playlist,
            &mut monitor,
            limits::PLAYLIST,
            cancel,
            deadline,
        )
        .await?;
        let guide = fetch(
            pool,
            tls,
            &guide,
            &mut monitor,
            limits::GUIDE,
            cancel,
            deadline,
        )
        .await?;
        parse(
            &mut monitor,
            key,
            receipt,
            playlist,
            guide,
            cancel,
            deadline,
        )
        .await
    }
    .await;
    let stopped = monitor.stop().await;
    let result = match (result, stopped) {
        (_, Err(ImportError::Cleanup)) => Err(ImportError::Cleanup),
        (Err(error), _) => Err(error),
        (Ok(_), Err(error)) => Err(error),
        (Ok(candidate), Ok(())) => {
            publication::publish(pool, receipt, candidate, cancel, deadline).await
        }
    };
    if let Err(error) = result
        && publication::failed(pool, receipt, error).await.is_err()
        && error != ImportError::Indeterminate
    {
        return Err(ImportError::Cleanup);
    }
    result
}

async fn fetch(
    pool: &PgPool,
    tls: &TlsConnector,
    source: &Source,
    monitor: &mut Monitor,
    limit: usize,
    cancel: &CancellationToken,
    deadline: Instant,
) -> Result<Vec<u8>, ImportError> {
    monitor.current().await?;
    let deadline = deadline.min(Instant::now() + Duration::from_secs(60));
    tokio::select! {
        biased;
        _=cancel.cancelled()=>Err(ImportError::Cancelled),
        error=monitor.error()=>Err(error),
        result=tokio::time::timeout_at(tokio::time::Instant::from_std(deadline),connection::fetch_bounded(source,pool,tls,limit))=>result.map_err(|_|ImportError::Deadline)?.map_err(Into::into),
    }
}

async fn parse(
    monitor: &mut Monitor,
    key: Arc<EncryptionKey>,
    receipt: Receipt,
    playlist: Vec<u8>,
    guide: Vec<u8>,
    cancel: &CancellationToken,
    deadline: Instant,
) -> Result<Candidate, ImportError> {
    let deadline = deadline.min(Instant::now() + Duration::from_secs(60));
    let token = cancel.child_token();
    let worker_cancel = token.clone();
    let mut task = tokio::task::spawn_blocking(move || {
        let mut work = Work {
            cancel: worker_cancel,
            deadline,
            plaintext: 0,
            ciphertext: 0,
        };
        let channels = m3u::parse(&playlist, receipt.fence.assignment, &mut work)?;
        drop(playlist);
        let xml = compression::decode(guide, &work)?;
        let guides = xml::parse(&xml, &channels, &mut work)?;
        drop(xml);
        codec::encode(Catalog { channels, guides }, &key, receipt, &mut work)
    });
    let error = tokio::select! {
        biased;
        _=cancel.cancelled()=>ImportError::Cancelled,
        error=monitor.error()=>error,
        _=tokio::time::sleep_until(tokio::time::Instant::from_std(deadline))=>ImportError::Deadline,
        result=&mut task=>return result.map_err(|_|ImportError::Cleanup)?,
    };
    token.cancel();
    task.await.map_err(|_| ImportError::Cleanup)?.ok();
    Err(error)
}

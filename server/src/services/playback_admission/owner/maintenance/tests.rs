// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use crate::services::{
    encryption::EncryptionKey,
    playback_admission::{AdmissionError, AdmissionOwner},
};
use crate::{
    config::BootstrapConfig,
    services::runtime::{self, HttpServer},
    state::AppState,
};
use sqlx::postgres::PgPoolOptions;
use std::{
    sync::{Arc, atomic::Ordering},
    time::Duration,
};

#[tokio::test]
#[ignore = "requires an owned disposable PostgreSQL18 fixture"]
async fn lost_maintenance_completion_is_retained_at_root() -> anyhow::Result<()> {
    uuid::Uuid::parse_str(&std::env::var("DUSKCUE_TEST_RESOURCE_ID")?)?;
    anyhow::ensure!(std::env::var("DUSKCUE_TRANSCODE_ACCESS_TESTS")? == "disposable");
    let url = std::env::var("DUSKCUE_DATABASE_URL")?;
    let parsed = url::Url::parse(&url)?;
    anyhow::ensure!(
        parsed.host_str() == Some("127.0.0.1")
            && parsed.path().starts_with("/duskcue_transcode_test")
    );
    let pool = PgPoolOptions::new()
        .max_connections(4)
        .connect(&url)
        .await?;
    sqlx::migrate!("./migrations").run(&pool).await?;
    let owner =
        AdmissionOwner::new(pool.clone(), Arc::new(EncryptionKey::generate().0), None).await?;
    let regular = owner.commands.clone().acquire_many_owned(7).await?;
    tokio::time::timeout(Duration::from_secs(4), async {
        loop {
            match owner.maintain().await {
                Err(AdmissionError::Pending) => tokio::time::sleep(Duration::from_millis(10)).await,
                result => break result,
            }
        }
    })
    .await??;
    assert_eq!(owner.commands.available_permits(), 0);
    drop(regular);
    let mut lock = pool.begin().await?;
    sqlx::query("SELECT id FROM playback_retention WHERE id FOR UPDATE")
        .execute(&mut *lock)
        .await?;
    let running = owner.clone();
    let observation = tokio::spawn(async move { running.maintain().await });
    let result = async {
        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                let waiting: i64 = sqlx::query_scalar("SELECT count(*) FROM pg_stat_activity WHERE datname=current_database() AND wait_event_type='Lock' AND query LIKE 'UPDATE playback_retention%'").fetch_one(&pool).await?;
                if waiting > 0 { break Ok::<_,sqlx::Error>(()); }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        }).await??;
        owner.maintenance_task.lock().expect("test maintenance task").take().expect("registered task").abort();
        tokio::time::timeout(Duration::from_secs(2), async {
            while !owner.maintenance_lost.load(Ordering::SeqCst) { tokio::task::yield_now().await; }
        }).await?;
        assert!(!owner.failed.load(Ordering::SeqCst));
        assert_eq!(owner.maintain().await.unwrap_err(), AdmissionError::Closed);
        Ok::<_,anyhow::Error>(())
    }.await;
    lock.rollback().await?;
    assert!(observation.await?.is_err());
    owner.close();
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(4), owner.drain())
            .await?
            .unwrap_err(),
        AdmissionError::Cleanup
    );
    let root =
        std::env::temp_dir().join(format!("duskcue-retention-root-{}", uuid::Uuid::now_v7()));
    let bootstrap = BootstrapConfig {
        database_url: None,
        bind_address: "127.0.0.1".into(),
        port: 0,
        data_dir: root.join("data"),
        cache_dir: root.join("cache"),
        log_level: "error".into(),
        environment: "test".into(),
        encryption_key: None,
        geoip_license_key: None,
    };
    let recorder = metrics_exporter_prometheus::PrometheusBuilder::new().build_recorder();
    let state = AppState::new(
        pool.clone(),
        bootstrap,
        recorder.handle(),
        EncryptionKey::generate().0,
    );
    assert!(state.playback_admission.set(owner.clone()).is_ok());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let mut http = HttpServer::start(
        listener,
        axum::Router::new(),
        tokio_util::sync::CancellationToken::new(),
    );
    assert!(
        runtime::shutdown(&state, &tokio_util::task::TaskTracker::new(), &mut http)
            .await
            .is_err()
    );
    assert!(!pool.is_closed());
    http.wait().await?;
    pool.close().await;
    result
}

// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use duskcue::{config::BootstrapConfig, services::encryption::EncryptionKey, state::AppState};
use sqlx::{AssertSqlSafe, PgPool, postgres::PgPoolOptions};
use std::{
    future::{Future, poll_fn},
    panic::{AssertUnwindSafe, catch_unwind},
    path::PathBuf,
    task::Poll,
    time::Duration,
};
use uuid::Uuid;

pub struct Database {
    pub pool: PgPool,
    pub url: String,
    root: PathBuf,
}
impl Database {
    pub fn state(&self) -> AppState {
        let bootstrap = BootstrapConfig {
            database_url: None,
            bind_address: "127.0.0.1".into(),
            port: 0,
            data_dir: self.root.join("data"),
            cache_dir: self.root.join("cache"),
            log_level: "error".into(),
            environment: "test".into(),
            encryption_key: None,
            geoip_license_key: None,
        };
        let recorder = metrics_exporter_prometheus::PrometheusBuilder::new().build_recorder();
        AppState::new(
            self.pool.clone(),
            bootstrap,
            recorder.handle(),
            EncryptionKey::generate().0,
        )
    }
}

pub async fn run(case: impl AsyncFnOnce(&Database) -> anyhow::Result<()>) -> anyhow::Result<()> {
    Uuid::parse_str(&std::env::var("DUSKCUE_TEST_RESOURCE_ID")?)?;
    anyhow::ensure!(std::env::var("DUSKCUE_TRANSCODE_ACCESS_TESTS")? == "disposable");
    let raw = std::env::var("DUSKCUE_DATABASE_URL")?;
    let mut url = url::Url::parse(&raw)?;
    anyhow::ensure!(
        url.host_str() == Some("127.0.0.1") && url.path().starts_with("/duskcue_transcode_test")
    );
    let admin = PgPoolOptions::new()
        .max_connections(1)
        .acquire_timeout(Duration::from_secs(3))
        .connect(&raw)
        .await?;
    let actual: String = sqlx::query_scalar("SELECT current_database()")
        .fetch_one(&admin)
        .await?;
    anyhow::ensure!(actual.starts_with("duskcue_transcode_test"));
    let name = format!("duskcue_retention_test_{}", Uuid::now_v7().simple());
    anyhow::ensure!(
        name.len() <= 63
            && name
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
    );
    sqlx::query(AssertSqlSafe(format!("CREATE DATABASE {name}")))
        .execute(&admin)
        .await?;
    url.set_path(&name);
    let parent = std::env::temp_dir().canonicalize()?;
    let root = parent.join(&name);
    let mut root_created = false;
    let result = async {
        tokio::fs::create_dir(&root).await?;
        root_created = true;
        let pool = PgPoolOptions::new()
            .max_connections(4)
            .acquire_timeout(Duration::from_secs(3))
            .connect(url.as_str())
            .await?;
        let db = Database {
            pool,
            url: url.to_string(),
            root: root.clone(),
        };
        let mut future = Box::pin(case(&db));
        let outcome = poll_fn(|context| {
            match catch_unwind(AssertUnwindSafe(|| future.as_mut().poll(context))) {
                Ok(Poll::Pending) => Poll::Pending,
                Ok(Poll::Ready(result)) => Poll::Ready(result),
                Err(_) => Poll::Ready(Err(anyhow::anyhow!(
                    "retention fixture panicked; original assertion is reported above"
                ))),
            }
        })
        .await;
        drop(future);
        db.pool.close().await;
        outcome
    }
    .await;
    let removed = sqlx::query(AssertSqlSafe(format!("DROP DATABASE {name}")))
        .execute(&admin)
        .await;
    admin.close().await;
    let files = if root_created {
        let canonical = root.canonicalize()?;
        anyhow::ensure!(
            canonical.parent() == Some(parent.as_path())
                && canonical.file_name().and_then(|v| v.to_str()) == Some(name.as_str())
        );
        tokio::fs::remove_dir_all(canonical).await
    } else {
        Ok(())
    };
    removed?;
    files?;
    result
}

pub async fn legacy(pool: &PgPool) -> anyhow::Result<()> {
    let all = sqlx::migrate!("./migrations");
    let migrations = all
        .iter()
        .filter(|m| m.version < 20261027010000)
        .cloned()
        .collect();
    sqlx::migrate::Migrator::with_migrations(migrations)
        .run(pool)
        .await?;
    Ok(())
}
pub async fn upgrade(pool: &PgPool) -> anyhow::Result<()> {
    sqlx::migrate!("./migrations").run(pool).await?;
    sqlx::query("CREATE TABLE audit_retention_test PARTITION OF audit_log DEFAULT")
        .execute(pool)
        .await?;
    Ok(())
}
pub async fn counts(pool: &PgPool) -> anyhow::Result<i64> {
    let (retained,actual,households):(i64,i64,i64) = sqlx::query_as("SELECT retained_count,(SELECT count(*) FROM playback_reservations),(SELECT COALESCE(sum(retained_count),0)::bigint FROM playback_receipt_households) FROM playback_retention WHERE id").fetch_one(pool).await?;
    assert_eq!(retained, actual);
    assert_eq!(retained, households);
    let wrong: i64 = sqlx::query_scalar("SELECT count(*) FROM playback_receipt_households h WHERE retained_count<>(SELECT count(*) FROM playback_reservations r WHERE r.household_id=h.household_id)").fetch_one(pool).await?;
    assert_eq!(wrong, 0);
    Ok(retained)
}

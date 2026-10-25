// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::docker;
use duskcue::{config::BootstrapConfig, services::encryption::EncryptionKey, state::AppState};
use sqlx::{PgPool, postgres::PgPoolOptions};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};
use uuid::Uuid;

pub struct Database {
    pub resource: String,
    pub image: String,
    pub password: String,
    pub url: String,
    pub root: PathBuf,
    pub pool: PgPool,
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
        let (key, _) = EncryptionKey::generate();
        AppState::new(self.pool.clone(), bootstrap, recorder.handle(), key)
    }
}

pub async fn run<F>(case: F) -> anyhow::Result<()>
where
    F: for<'a> std::ops::AsyncFnOnce(&'a Database) -> anyhow::Result<()>,
{
    let resource = std::env::var("DUSKCUE_TEST_RESOURCE_ID")?;
    Uuid::parse_str(&resource)?;
    let endpoint = docker::command(&[
        "context".into(),
        "inspect".into(),
        "--format".into(),
        "{{.Endpoints.docker.Host}}".into(),
    ])
    .await?;
    anyhow::ensure!(
        endpoint.status.success(),
        "local Docker endpoint unavailable"
    );
    let endpoint = String::from_utf8(endpoint.stdout)?;
    let endpoint = endpoint.trim();
    anyhow::ensure!(
        endpoint.starts_with("unix:///")
            || (endpoint.starts_with("npipe://")
                && endpoint.ends_with("/pipe/dockerDesktopLinuxEngine")),
        "household contract requires a local Docker engine"
    );
    let container = docker::GuardedContainer::registered()?;
    let image = docker::command(&[
        "image".into(),
        "inspect".into(),
        "postgres:18".into(),
        "--format".into(),
        "{{.Id}}".into(),
    ])
    .await?;
    anyhow::ensure!(image.status.success(), "local PostgreSQL18 image required");
    let image = String::from_utf8(image.stdout)?.trim().to_string();
    anyhow::ensure!(
        image.starts_with("sha256:") && image.len() == 71,
        "immutable PostgreSQL fixture required"
    );
    let password = duskcue::domains::auth::service::generate_session_token();
    let database = format!("duskcue_household_{}", resource.replace('-', ""));
    let temp = std::env::temp_dir().canonicalize()?;
    let root_name = format!("duskcue-household-{resource}");
    let root = temp.join(&root_name);
    std::fs::create_dir(&root)?;
    let outcome = async {
        let mut args = container.run_args();
        args.extend([
            "-d".into(),
            "-p".into(),
            "127.0.0.1::5432".into(),
            "-e".into(),
            format!("POSTGRES_PASSWORD={password}"),
            "-e".into(),
            format!("POSTGRES_DB={database}"),
            image.clone(),
            "postgres".into(),
            "-c".into(),
            "shared_buffers=16MB".into(),
            "-c".into(),
            "max_connections=16".into(),
        ]);
        anyhow::ensure!(
            docker::command(&args).await?.status.success(),
            "owned PostgreSQL start failed"
        );
        let port =
            docker::command(&["port".into(), container.name.clone(), "5432/tcp".into()]).await?;
        anyhow::ensure!(port.status.success(), "owned fixture port unavailable");
        let port = String::from_utf8(port.stdout)?
            .trim()
            .strip_prefix("127.0.0.1:")
            .ok_or_else(|| anyhow::anyhow!("nonlocal fixture port"))?
            .parse::<u16>()?;
        let url = format!("postgres://postgres:{password}@127.0.0.1:{port}/{database}");
        let deadline = Instant::now() + Duration::from_secs(30);
        let pool = loop {
            match PgPoolOptions::new()
                .max_connections(4)
                .acquire_timeout(Duration::from_secs(2))
                .connect(&url)
                .await
            {
                Ok(pool) => break pool,
                Err(_) if Instant::now() < deadline => {
                    tokio::time::sleep(Duration::from_millis(200)).await
                }
                Err(_) => anyhow::bail!("owned PostgreSQL readiness failed"),
            }
        };
        let result = async {
            sqlx::migrate!("./migrations").run(&pool).await?;
            sqlx::query("CREATE TABLE audit_household_test PARTITION OF audit_log DEFAULT")
                .execute(&pool)
                .await?;
            case(&Database {
                resource,
                image,
                password,
                url,
                root: root.clone(),
                pool: pool.clone(),
            })
            .await
        }
        .await;
        pool.close().await;
        result
    }
    .await;
    let removed = container.remove().await;
    let resolved = root.canonicalize()?;
    anyhow::ensure!(
        resolved.parent() == Some(temp.as_path())
            && resolved.file_name().and_then(|v| v.to_str()) == Some(root_name.as_str()),
        "owned fixture directory identity changed"
    );
    let root_removed = tokio::fs::remove_dir_all(resolved).await;
    removed?;
    root_removed?;
    outcome
}

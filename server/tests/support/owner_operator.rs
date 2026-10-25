// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use duskcue::domains::household_security::operator::fingerprint;
use sqlx::PgPool;
use std::{path::Path, time::Duration};
use tokio::process::Command;
use uuid::Uuid;

async fn command(url: &str, root: &Path, user: Uuid) -> anyhow::Result<std::process::Output> {
    let mut command = Command::new(env!("CARGO_BIN_EXE_duskcue"));
    command
        .arg("--config")
        .arg(root.join("unused-config.toml"))
        .arg("owner-handoff")
        .arg("--user")
        .arg(user.to_string())
        .env("DUSKCUE_DATABASE_URL", url)
        .env("DUSKCUE_ENVIRONMENT", "production")
        .env("DUSKCUE_DATA_DIR", root.join("data"))
        .env("DUSKCUE_CACHE_DIR", root.join("cache"))
        .env("TOKIO_WORKER_THREADS", "2")
        .stdin(std::process::Stdio::null())
        .kill_on_drop(true);
    #[cfg(target_os = "windows")]
    command.creation_flags(0x08000000);
    let output = tokio::time::timeout(Duration::from_secs(30), command.output()).await??;
    anyhow::ensure!(
        output.stdout.len() + output.stderr.len() <= 4096,
        "operator command output exceeded bound"
    );
    Ok(output)
}

pub async fn run(
    url: &str,
    root: &Path,
    pool: &PgPool,
    user: Uuid,
    secrets: &mut Vec<String>,
) -> anyhow::Result<()> {
    let output = command(url, root, user).await?;
    anyhow::ensure!(
        output.status.success() && output.stderr.is_empty(),
        "operator command failed"
    );
    let token = String::from_utf8(output.stdout)?.trim().to_string();
    anyhow::ensure!(
        token.len() == 57 && token.starts_with("duskcue-owner-"),
        "operator handoff format changed"
    );
    let valid:bool=sqlx::query_scalar("SELECT token_hash=$2 AND expires_at>clock_timestamp() AND expires_at<=clock_timestamp()+interval '10 minutes' FROM household_owner_handoffs WHERE user_id=$1")
        .bind(user).bind(fingerprint(token.as_bytes())).fetch_one(pool).await?;
    anyhow::ensure!(
        valid,
        "operator command did not persist bounded hashed handoff"
    );
    secrets.push(token);
    let output = command(url, root, Uuid::now_v7()).await?;
    anyhow::ensure!(
        !output.status.success()
            && output.stdout.is_empty()
            && !String::from_utf8_lossy(&output.stderr).contains(url),
        "operator refusal leaked secret or reported success"
    );
    Ok(())
}

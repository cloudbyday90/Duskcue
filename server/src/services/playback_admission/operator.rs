// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::{AdmissionError, database};
use sqlx::PgPool;
use uuid::Uuid;

pub async fn fence(pool: &PgPool, instance: Uuid, expected: i64) -> Result<i64, AdmissionError> {
    database::transaction(pool,async |conn| {
        let revision:Option<i64>=sqlx::query_scalar("UPDATE playback_instances SET fenced=true,revision=revision+1,fenced_at=clock_timestamp() WHERE id=$1 AND revision=$2 AND NOT fenced RETURNING revision")
            .bind(instance).bind(expected).fetch_optional(&mut *conn).await?;
        let revision=revision.ok_or(AdmissionError::Conflict)?;
        sqlx::query("UPDATE playback_reservations SET state='retiring' WHERE instance_id=$1 AND state='held'").bind(instance).execute(&mut *conn).await?;
        sqlx::query("INSERT INTO playback_operator_events(instance_id,revision,operation) VALUES ($1,$2,'fence')").bind(instance).bind(revision).execute(conn).await?;
        Ok(revision)
    }).await
}

pub async fn reconcile(
    pool: &PgPool,
    instance: Uuid,
    revision: i64,
    resources_gone: bool,
) -> Result<u64, AdmissionError> {
    if !resources_gone {
        return Err(AdmissionError::Cleanup);
    }
    database::transaction(pool,async |conn| {
        let valid:Option<bool>=sqlx::query_scalar("SELECT fenced AND revision=$2 FROM playback_instances WHERE id=$1 FOR UPDATE").bind(instance).bind(revision).fetch_optional(&mut *conn).await?;
        if valid!=Some(true) {return Err(AdmissionError::Conflict);}
        sqlx::query("SELECT playback_vod_complete_outputs(id) FROM playback_reservations WHERE instance_id=$1 AND state<>'released' ORDER BY id").bind(instance).execute(&mut *conn).await?;
        sqlx::query("UPDATE playback_vod_commands c SET resources_completed=true,state=CASE WHEN c.state='preparing' THEN 'failed' WHEN c.state='ready' THEN 'stopping' ELSE c.state END FROM playback_reservations r WHERE r.id=c.reservation_id AND r.instance_id=$1 AND r.state<>'released'").bind(instance).execute(&mut *conn).await?;
        let count=sqlx::query("UPDATE playback_reservations SET state='released',released_at=clock_timestamp() WHERE instance_id=$1 AND state<>'released'").bind(instance).execute(&mut *conn).await?.rows_affected();
        sqlx::query("UPDATE playback_instances SET reconciled_at=clock_timestamp() WHERE id=$1").bind(instance).execute(&mut *conn).await?;
        sqlx::query("INSERT INTO playback_operator_events(instance_id,revision,operation) VALUES ($1,$2,'reconcile')").bind(instance).bind(revision).execute(conn).await?;
        Ok(count)
    }).await
}

pub async fn run(
    bootstrap: &crate::config::BootstrapConfig,
    command: crate::config::OperatorCommand,
) -> Result<(), AdmissionError> {
    let url = bootstrap
        .database_url
        .as_deref()
        .ok_or(AdmissionError::Storage)?;
    let pool = tokio::time::timeout(
        std::time::Duration::from_secs(3),
        sqlx::postgres::PgPoolOptions::new()
            .max_connections(1)
            .acquire_timeout(std::time::Duration::from_secs(3))
            .connect(url),
    )
    .await
    .map_err(|_| AdmissionError::Storage)??;
    let result = match command {
        crate::config::OperatorCommand::PlaybackFence { instance, revision } => {
            fence(&pool, instance, revision)
                .await
                .map(|revision| format!("instance={instance} fence_revision={revision}"))
        }
        crate::config::OperatorCommand::PlaybackReconcile {
            instance,
            fence_revision,
            resources_gone,
        } => reconcile(&pool, instance, fence_revision, resources_gone)
            .await
            .map(|count| format!("instance={instance} released={count}")),
        _ => Err(AdmissionError::Conflict),
    };
    tokio::time::timeout(std::time::Duration::from_secs(3), pool.close())
        .await
        .map_err(|_| AdmissionError::Storage)?;
    println!("{}", result?);
    Ok(())
}

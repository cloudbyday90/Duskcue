// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::{
    database::Database,
    fixture::{self, Fixture},
};
use duskcue::services::{
    playback_admission::{AdmissionError as E, AdmissionOwner},
    runtime::{self, HttpServer},
};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    process::Command,
    sync::oneshot,
};
use tokio_util::{sync::CancellationToken, task::TaskTracker};

pub async fn run(db: &Database) -> anyhow::Result<()> {
    let f = Fixture::new(db).await?;
    let owner = f.owner().await?;
    sqlx::query("UPDATE users SET max_streams=1 WHERE id=$1")
        .bind(f.actor.user)
        .execute(&db.pool)
        .await?;
    let r = f.request();
    let path = db.root.join("owned-viewer-output");
    let file = tokio::fs::File::create(&path).await?;
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let client = TcpStream::connect(listener.local_addr()?).await?;
    let (peer, _) = listener.accept().await?;
    drop(listener);
    let (release, wait) = oneshot::channel();
    let (cancelled, observed) = oneshot::channel();
    let gone = Arc::new(AtomicBool::new(false));
    let done = gone.clone();
    let output = path.clone();
    owner
        .start(r, move |cancel| async move {
            let mut file = file;
            let mut client = client;
            file.write_all(b"owned viewer output")
                .await
                .map_err(|_| E::Cleanup)?;
            client.write_all(b"owned").await.map_err(|_| E::Cleanup)?;
            cancel.cancelled().await;
            let _ = cancelled.send(());
            let _ = wait.await;
            drop(file);
            drop(client);
            tokio::fs::remove_file(output)
                .await
                .map_err(|_| E::Cleanup)?;
            done.store(true, Ordering::SeqCst);
            Ok(())
        })
        .await?;
    owner.stop(r.id)?;
    observed.await?;
    assert!(!gone.load(Ordering::SeqCst));
    assert!(path.exists());
    assert_eq!(
        fixture::start(&owner, f.request()).await.unwrap_err(),
        E::Capacity
    );
    assert_ne!(fixture::state(&db.pool, r.id).await?, "released");
    release
        .send(())
        .map_err(|_| anyhow::anyhow!("owned release observer lost"))?;
    fixture::released(&db.pool, r.id).await?;
    assert!(!path.exists());
    assert!(gone.load(Ordering::SeqCst));
    let mut peer = peer;
    let mut bytes = Vec::new();
    peer.read_to_end(&mut bytes).await?;
    assert_eq!(bytes, b"owned");
    fixture::close(&owner).await?;

    let f = Fixture::new(db).await?;
    let owner = f.owner().await?;
    let r = f.request();
    let mut command = Command::new(std::env::current_exe()?);
    command
        .args([
            "--exact",
            "owned_resource_child",
            "--ignored",
            "--nocapture",
        ])
        .env("DUSKCUE_ADMISSION_CHILD", "owned")
        .kill_on_drop(true);
    #[cfg(windows)]
    command.creation_flags(0x08000000);
    let (began, started) = oneshot::channel();
    let (exited, exit) = oneshot::channel();
    owner
        .start(r, move |cancel| async move {
            let mut child = command.spawn().map_err(|_| E::Cleanup)?;
            let _ = began.send(());
            cancel.cancelled().await;
            child.kill().await.map_err(|_| E::Cleanup)?;
            let status = child.wait().await.map_err(|_| E::Cleanup)?;
            let _ = exited.send(status);
            Ok(())
        })
        .await?;
    started.await?;
    owner.stop(r.id)?;
    tokio::time::timeout(Duration::from_secs(5), exit).await??;
    fixture::released(&db.pool, r.id).await?;
    fixture::close(&owner).await?;

    let f = Fixture::new(db).await?;
    let owner = f.owner().await?;
    let r = f.request();
    owner
        .start(r, |cancel| async move {
            cancel.cancelled().await;
            Err(E::Cleanup)
        })
        .await?;
    owner.stop(r.id)?;
    owner.close();
    assert_eq!(owner.drain().await.unwrap_err(), E::Cleanup);
    assert_ne!(fixture::state(&db.pool, r.id).await?, "released");
    sqlx::query("UPDATE users SET max_streams=1 WHERE id=$1")
        .bind(f.actor.user)
        .execute(&db.pool)
        .await?;
    let replacement = f.owner().await?;
    assert_eq!(
        fixture::start(&replacement, f.request()).await.unwrap_err(),
        E::Capacity
    );
    cli(
        db,
        &[
            "playback-fence",
            "--instance",
            &owner.instance().to_string(),
            "--revision",
            "1",
        ],
        true,
    )
    .await?;
    cli(
        db,
        &[
            "playback-reconcile",
            "--instance",
            &owner.instance().to_string(),
            "--fence-revision",
            "1",
            "--resources-gone",
        ],
        false,
    )
    .await?;
    cli(
        db,
        &[
            "playback-reconcile",
            "--instance",
            &owner.instance().to_string(),
            "--fence-revision",
            "2",
        ],
        false,
    )
    .await?;
    cli(
        db,
        &[
            "playback-reconcile",
            "--instance",
            &owner.instance().to_string(),
            "--fence-revision",
            "2",
            "--resources-gone",
        ],
        true,
    )
    .await?;
    assert_eq!(fixture::state(&db.pool, r.id).await?, "released");
    fixture::start(&replacement, f.request()).await?;
    fixture::close(&replacement).await?;

    let f = Fixture::new(db).await?;
    let owner = f.owner().await?;
    let r = f.request();
    let (cancelled, observed) = oneshot::channel();
    owner
        .start(r, move |cancel| async move {
            cancel.cancelled().await;
            let _ = cancelled.send(());
            Ok(())
        })
        .await?;
    sqlx::query("UPDATE playback_reservations SET deadline=clock_timestamp()+interval '10 minutes' WHERE id=$1").bind(r.id).execute(&db.pool).await?;
    tokio::time::timeout(Duration::from_secs(62), observed).await??;
    fixture::released(&db.pool, r.id).await?;
    fixture::close(&owner).await?;

    let f = Fixture::new(db).await?;
    let owner = AdmissionOwner::attach(&f.state).await?;
    let r = f.request();
    sqlx::query(
        "UPDATE user_sessions SET last_active_at=clock_timestamp()-interval '2 hours' WHERE id=$1",
    )
    .bind(r.actor.session)
    .execute(&db.pool)
    .await?;
    fixture::start(&owner, r).await?;
    let mut config = (*f.state.runtime_config.load_full()).clone();
    config.auth.session_idle_timeout_hours = Some(1);
    f.state.runtime_config.store(Arc::new(config));
    assert!(owner.renew(r).await.is_err());
    fixture::released(&db.pool, r.id).await?;
    fixture::close(&owner).await?;

    let f = Fixture::new(db).await?;
    let owner = AdmissionOwner::attach(&f.state).await?;
    let r = f.request();
    let (release, wait) = oneshot::channel();
    owner
        .start(r, move |cancel| async move {
            cancel.cancelled().await;
            let _ = wait.await;
            Ok(())
        })
        .await?;
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let mut http = HttpServer::start(listener, axum::Router::new(), CancellationToken::new());
    let began = tokio::time::Instant::now();
    assert!(
        runtime::shutdown(&f.state, &TaskTracker::new(), &mut http)
            .await
            .is_err()
    );
    assert!(began.elapsed() >= Duration::from_secs(29));
    assert!(!db.pool.is_closed());
    release
        .send(())
        .map_err(|_| anyhow::anyhow!("cleanup ownership lost"))?;
    assert_eq!(owner.drain().await.unwrap_err(), E::Cleanup);
    fixture::released(&db.pool, r.id).await?;
    http.wait().await?;

    let f = Fixture::new(db).await?;
    let owner = AdmissionOwner::attach(&f.state).await?;
    let r = f.request();
    fixture::start(&owner, r).await?;
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let mut http = HttpServer::start(listener, axum::Router::new(), CancellationToken::new());
    runtime::shutdown(&f.state, &TaskTracker::new(), &mut http).await?;
    assert!(db.pool.is_closed());
    assert!(http.finished());
    assert_eq!(
        fixture::start(&owner, f.request()).await.unwrap_err(),
        E::Closed
    );
    Ok(())
}

async fn cli(db: &Database, args: &[&str], success: bool) -> anyhow::Result<()> {
    let mut command = Command::new(env!("CARGO_BIN_EXE_duskcue"));
    command
        .env("DUSKCUE_DATABASE_URL", &db.url)
        .env("DUSKCUE_DATA_DIR", db.root.join("operator-data"))
        .env("DUSKCUE_CACHE_DIR", db.root.join("operator-cache"))
        .args(args)
        .kill_on_drop(true);
    #[cfg(windows)]
    command.creation_flags(0x08000000);
    let output = tokio::time::timeout(Duration::from_secs(10), command.output()).await??;
    anyhow::ensure!(
        output.status.success() == success,
        "operator command result incorrect"
    );
    anyhow::ensure!(
        !String::from_utf8_lossy(&output.stdout).contains(&db.password),
        "operator output leaked credential"
    );
    Ok(())
}

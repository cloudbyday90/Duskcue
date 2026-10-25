// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::{
    assignment, database,
    http::Case,
    import_fixture as fixture,
    peer::{self, Behavior, Peer, Security},
    refresh_cases,
};
use axum::http::StatusCode;
use serde_json::{Value, json};
use std::{net::SocketAddr, process::Stdio, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    process::Command,
};

pub(super) async fn request(
    address: SocketAddr,
    token: &str,
    method: &str,
    path: &str,
    body: Option<Value>,
    etag: Option<&str>,
) -> anyhow::Result<(u16, Value)> {
    let body = body.map(|v| v.to_string()).unwrap_or_default();
    let extra = etag
        .map(|v| format!("If-Match: {v}\r\n"))
        .unwrap_or_default();
    let bytes = format!(
        "{method} {path} HTTP/1.1\r\nHost: {address}\r\nAuthorization: Bearer {token}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n{extra}\r\n{body}",
        body.len()
    );
    let mut stream = TcpStream::connect(address).await?;
    stream.write_all(bytes.as_bytes()).await?;
    let mut data = Vec::new();
    tokio::time::timeout(
        Duration::from_secs(5),
        stream.take(32769).read_to_end(&mut data),
    )
    .await??;
    anyhow::ensure!(data.len() < 32769, "root response exceeded bound");
    let head = data
        .windows(4)
        .position(|v| v == b"\r\n\r\n")
        .ok_or_else(|| anyhow::anyhow!("root response head absent"))?;
    let header = std::str::from_utf8(&data[..head])?;
    let status = header
        .split_whitespace()
        .nth(1)
        .ok_or_else(|| anyhow::anyhow!("root status absent"))?
        .parse()?;
    let body: Value = serde_json::from_slice(&data[head + 4..])?;
    Ok((
        status,
        json!({"body":body,"etag":header.lines().find_map(|v|v.split_once(':').filter(|(k,_)|k.eq_ignore_ascii_case("etag")).map(|(_,v)|v.trim()))}),
    ))
}

pub async fn run(db: &database::Database, case: &mut Case, key: &str) -> anyhow::Result<()> {
    for mode in ["normal", "connected", "forced"] {
        process(db, case, key, mode).await?;
    }
    Ok(())
}

async fn process(
    db: &database::Database,
    case: &mut Case,
    key: &str,
    mode: &str,
) -> anyhow::Result<()> {
    case.admin_proof(false).await?;
    refresh_cases::enable(case, true).await?;
    let a = Peer::new(
        "127.0.0.1".parse()?,
        Security::Plain,
        Behavior::HeadersStall,
    )
    .await?;
    let b = Peer::new(
        "127.0.0.1".parse()?,
        Security::Plain,
        Behavior::Reply(peer::response(&fixture::guide())),
    )
    .await?;
    let playlist = format!(
        "{}/private-feed?credential=feed-query-private",
        a.origin("http", "127.0.0.1")
    );
    let guide = format!(
        "{}/private-feed?credential=feed-query-private",
        b.origin("http", "127.0.0.1")
    );
    case.secrets.extend([
        playlist.clone(),
        guide.clone(),
        a.origin("http", "127.0.0.1"),
        b.origin("http", "127.0.0.1"),
    ]);
    assignment::patch(
        case,
        json!({"operation":"rotate_credentials","playlist_url":playlist,"guide_url":guide}),
    )
    .await?
    .expect(StatusCode::OK)?;
    for (feed, origin) in [
        ("playlist", a.origin("http", "127.0.0.1")),
        ("guide", b.origin("http", "127.0.0.1")),
    ] {
        assignment::patch(case,json!({"operation":"approve_destination","feed":feed,"expected_origin":origin,"addresses":["127.0.0.1"]})).await?.expect(StatusCode::OK)?;
    }
    let mut auth = case.state.runtime_config.load().auth.clone();
    auth.setup_complete = true;
    sqlx::query("UPDATE server_config SET auth=$1")
        .bind(serde_json::to_value(auth)?)
        .execute(&case.pool)
        .await?;
    duskcue::services::scheduler::seed_default_tasks(&case.pool).await?;
    sqlx::query("UPDATE scheduled_tasks SET is_enabled=false")
        .execute(&case.pool)
        .await?;
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let address = listener.local_addr()?;
    drop(listener);
    sqlx::query("UPDATE live_tv_import_state SET state='running',category=NULL,finished_at=NULL")
        .execute(&case.pool)
        .await?;
    let root = db.root.join(format!("process-{mode}"));
    tokio::fs::create_dir_all(&root).await?;
    let mut command = Command::new(env!("CARGO_BIN_EXE_duskcue"));
    command
        .arg("--config")
        .arg(root.join("unused-config.toml"))
        .env("DUSKCUE_DATABASE_URL", &db.url)
        .env("DUSKCUE_ENCRYPTION_KEY", key)
        .env("DUSKCUE_DATA_DIR", root.join("data"))
        .env("DUSKCUE_CACHE_DIR", root.join("cache"))
        .env("DUSKCUE_BIND_ADDRESS", "127.0.0.1")
        .env("DUSKCUE_PORT", address.port().to_string())
        .env("DUSKCUE_ENVIRONMENT", "production")
        .env("TOKIO_WORKER_THREADS", "2")
        .env("RUST_LOG", "info")
        .env("DUSKCUE_LOG_LEVEL", "info")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    #[cfg(windows)]
    command.creation_flags(0x00000200);
    let mut child = command.spawn()?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| anyhow::anyhow!("root log absent"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| anyhow::anyhow!("root log absent"))?;
    let output = tokio::spawn(async move {
        let mut out = Vec::new();
        stdout.take(1024 * 1024 + 1).read_to_end(&mut out).await?;
        Ok::<_, std::io::Error>(out)
    });
    let errors = tokio::spawn(async move {
        let mut out = Vec::new();
        stderr.take(1024 * 1024 + 1).read_to_end(&mut out).await?;
        Ok::<_, std::io::Error>(out)
    });
    let mut pending = None;
    let outcome = async {
        let path = refresh_cases::path(case);
        let representation = tokio::time::timeout(Duration::from_secs(30), async {
            loop {
                if let Ok((200, value)) =
                    request(address, &case.admin.token, "GET", &path, None, None).await
                {
                    return Ok::<_, anyhow::Error>(value);
                }
                if child.try_wait()?.is_some() {
                    anyhow::bail!("root process exited before readiness");
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        })
        .await??;
        let (code, prior) = request(
            address,
            &case.admin.token,
            "GET",
            &format!("{path}/status"),
            None,
            None,
        )
        .await?;
        anyhow::ensure!(
            code == 200
                && prior["body"]["observation"] == "unfinished_uncertain"
                && prior["body"]["locally_owned"] == false,
            "restart did not preserve uncertain unowned evidence"
        );
        let tag = representation["etag"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("root ETag absent"))?;
        anyhow::ensure!(
            a.hits() == 0 && b.hits() == 0,
            "root startup fetched without a manual command"
        );
        let (status, accepted) = request(
            address,
            &case.admin.token,
            "POST",
            &path,
            Some(json!({"playlist_pin":0,"guide_pin":0})),
            Some(tag),
        )
        .await?;
        anyhow::ensure!(status == 202, "root did not accept refresh");
        a.wait_phase(3).await?;
        let (status, observed) = request(
            address,
            &case.admin.token,
            "GET",
            accepted["body"]["status_url"].as_str().unwrap(),
            None,
            None,
        )
        .await?;
        anyhow::ensure!(
            status == 200 && observed["body"]["locally_owned"] == true,
            "disconnected accepted request was not retained"
        );
        let held = if mode == "normal" {
            None
        } else {
            Some(hold_events(address, &case.admin.token).await?)
        };
        if mode == "forced" {
            pending = Some(
                super::stop_request::HeldRequest::start(
                    case,
                    &db.url,
                    address,
                    format!("{path}/status"),
                )
                .await?,
            );
        }
        let pid = child
            .id()
            .ok_or_else(|| anyhow::anyhow!("owned root PID absent"))?;
        super::process_signal::send(pid).await?;
        if mode == "forced" {
            tokio::time::sleep(Duration::from_millis(150)).await;
            anyhow::ensure!(
                child.try_wait()?.is_none(),
                "held HTTP request did not retain shutdown"
            );
            super::process_signal::send(pid).await?;
        }
        let exit = tokio::time::timeout(Duration::from_secs(35), child.wait()).await??;
        anyhow::ensure!(
            exit.code() == Some(if mode == "forced" { 1 } else { 0 }),
            "root shutdown exit contract failed"
        );
        if let Some(stream) = held
            && mode == "connected"
        {
            let mut bytes = Vec::new();
            tokio::time::timeout(
                Duration::from_secs(3),
                stream.take(65537).read_to_end(&mut bytes),
            )
            .await??;
            anyhow::ensure!(
                bytes.len() <= 65536 && bytes.ends_with(b"0\r\n\r\n"),
                "connected SSE did not finish its HTTP response"
            );
        }
        Ok::<_, anyhow::Error>(())
    }
    .await;
    if outcome.is_err() {
        child.kill().await.ok();
        let _ = child.wait().await;
    }
    if let Some(held) = pending {
        held.finish().await?;
    }
    let out = output.await??;
    let err = errors.await??;
    anyhow::ensure!(
        out.len() <= 1024 * 1024 && err.len() <= 1024 * 1024,
        "root logs exceeded bound"
    );
    let text = String::from_utf8_lossy(&out).to_string() + &String::from_utf8_lossy(&err);
    anyhow::ensure!(
        !case.secrets.iter().any(|secret| text.contains(secret)) && !text.contains(&db.url),
        "root log leaked secrets"
    );
    outcome?;
    anyhow::ensure!(
        text.contains("Duskcue ready"),
        "root readiness evidence absent"
    );
    let completed = text.contains("Shutdown complete");
    let failed = text.contains("shutdown_failed");
    let forced = text.contains("forced_shutdown");
    anyhow::ensure!(
        match mode {
            "normal" | "connected" => completed && !failed && !forced,
            _ => !completed && forced,
        },
        "root completion evidence differs from exit contract: completed={completed}, failed={failed}, forced={forced}"
    );
    a.finish(true).await?;
    b.finish(false).await?;
    println!("IPTV_REFRESH_STAGE actual_root_{mode}_signal_and_disconnect_passed");
    Ok(())
}

async fn hold_events(address: SocketAddr, token: &str) -> anyhow::Result<TcpStream> {
    let mut stream = TcpStream::connect(address).await?;
    stream.write_all(format!("GET /api/v1/events HTTP/1.1\r\nHost: {address}\r\nAuthorization: Bearer {token}\r\nAccept: text/event-stream\r\n\r\n").as_bytes()).await?;
    let head = tokio::time::timeout(Duration::from_secs(5), async {
        let mut head = Vec::new();
        while !head.ends_with(b"\r\n\r\n") && head.len() < 32768 {
            head.push(stream.read_u8().await?);
        }
        Ok::<_, std::io::Error>(head)
    })
    .await??;
    anyhow::ensure!(
        head.starts_with(b"HTTP/1.1 200")
            && String::from_utf8_lossy(&head).contains("text/event-stream"),
        "held production HTTP response was not an SSE stream"
    );
    Ok(stream)
}

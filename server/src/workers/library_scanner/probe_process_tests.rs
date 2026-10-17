// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use std::io::Write;
use std::process::Stdio;
use std::time::{Duration, Instant};

use tokio::process::{Child, Command};

use super::{MAX_STDERR, capture};

fn child(mode: &str) -> Child {
    let module = module_path!().split_once("::").unwrap().1;
    Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            &format!("{module}::fixture_child"),
            "--nocapture",
        ])
        .env("DUSKCUE_PROBE_TEST_CHILD", mode)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .unwrap()
}

#[test]
fn fixture_child() {
    match std::env::var("DUSKCUE_PROBE_TEST_CHILD").ok().as_deref() {
        Some("stall") => std::thread::sleep(Duration::from_secs(10)),
        Some("flood") => {
            std::io::stderr()
                .write_all(&vec![b'x'; MAX_STDERR + 1])
                .unwrap();
            std::thread::sleep(Duration::from_secs(10));
        }
        _ => {}
    }
}

#[tokio::test]
async fn deadline_kills_and_reaps_the_owned_probe_child() {
    let child = child("stall");
    let id = child.id().unwrap();
    let started = Instant::now();
    let result = capture(child, Duration::from_millis(500)).await;
    let error = match result {
        Err(error) => error,
        Ok(_) => panic!("stalled child was accepted"),
    };
    assert_eq!(error.message, "ffprobe deadline exceeded");
    assert!(error.retryable);
    assert!(started.elapsed() < Duration::from_secs(5));
    #[cfg(target_os = "linux")]
    assert!(!std::path::Path::new("/proc").join(id.to_string()).exists());
    #[cfg(not(target_os = "linux"))]
    let _ = id;
}

#[tokio::test]
async fn stderr_flood_kills_and_reaps_the_owned_probe_without_a_second_attempt() {
    let child = child("flood");
    let id = child.id().unwrap();
    let started = Instant::now();
    let result = capture(child, Duration::from_secs(5)).await;
    let error = match result {
        Err(error) => error,
        Ok(_) => panic!("unbounded diagnostics were accepted"),
    };
    assert!(error.message.contains("output byte limit exceeded"));
    assert!(!error.retryable);
    assert!(started.elapsed() < Duration::from_secs(5));
    #[cfg(target_os = "linux")]
    assert!(!std::path::Path::new("/proc").join(id.to_string()).exists());
    #[cfg(not(target_os = "linux"))]
    let _ = id;
}

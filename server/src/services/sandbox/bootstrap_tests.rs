// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use std::os::unix::process::ExitStatusExt;

use super::*;
use crate::services::sandbox::SandboxConfig;

struct Fixture {
    root: PathBuf,
    source: PathBuf,
    cache: PathBuf,
    outside: PathBuf,
}

impl Fixture {
    async fn new() -> anyhow::Result<Self> {
        uuid::Uuid::parse_str(&std::env::var("DUSKCUE_TEST_RESOURCE_ID")?)?;
        let root = std::env::current_dir()?
            .join(".cache/tonight-bootstrap")
            .join(uuid::Uuid::now_v7().to_string());
        for broad_path in ["/tmp", "/usr", "/lib", "/etc", "/dev/dri"] {
            anyhow::ensure!(
                !root.starts_with(broad_path),
                "negative filesystem fixture must be outside existing broad grants"
            );
        }
        let cache = root.join("cache");
        tokio::fs::create_dir_all(&cache).await?;
        let source = root.join("source");
        let outside = root.join("outside");
        tokio::fs::write(&source, b"S").await?;
        tokio::fs::write(&outside, b"O").await?;
        anyhow::ensure!(
            tokio::fs::read(&outside).await? == b"O",
            "outside control must be readable before sandboxing"
        );
        Ok(Self {
            root,
            source,
            cache,
            outside,
        })
    }

    fn prepared(&self) -> io::Result<PreparedLaunch> {
        PreparedLaunch::probe(&SandboxConfig {
            media_path: &self.source,
            transcode_dir: &self.cache,
        })
    }

    async fn invoke(&self, mode: &str, arguments: &[PathBuf]) -> io::Result<std::process::Output> {
        let mut prepared = self.prepared()?;
        let mut command = prepared.command();
        command.arg(mode).args(arguments);
        output(command, prepared, self.cache.join("observer-output")).await
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires fresh packaged Linux bootstrap and the root-owned native probe fixture"]
async fn actual_mandatory_bootstrap_denies_later_exec_network_and_outside_paths()
-> anyhow::Result<()> {
    let fixture = Fixture::new().await?;
    let status = fixture.invoke("status", &[]).await?;
    assert!(status.status.success());
    assert_eq!(status.stdout, b"no_new_privs=1\nseccomp=2\n");
    let numa = fixture.invoke("numa-query", &[]).await?;
    assert!(numa.status.success());
    assert_eq!(numa.stdout, b"NUMA_QUERY_DENIED\n");
    let registration = fixture.invoke("barrier-registration", &[]).await?;
    assert!(registration.status.success());
    assert_eq!(registration.stdout, b"BARRIER_REGISTRATION_DENIED\n");
    for mode in ["exec", "execat", "network"] {
        let result = fixture.invoke(mode, &[]).await?;
        assert_eq!(result.status.signal(), Some(libc::SIGSYS), "mode {mode}");
    }
    let denied = fixture
        .invoke("path", std::slice::from_ref(&fixture.outside))
        .await?;
    assert!(denied.status.success());
    assert_eq!(denied.stdout, b"OUTSIDE_DENIED\n");
    let written = fixture.cache.join("allowed-output");
    let allowed = fixture
        .invoke("allowed", &[fixture.source.clone(), written.clone()])
        .await?;
    assert!(allowed.status.success());
    assert_eq!(allowed.stdout, b"ALLOWED_SOURCE_CACHE\n");
    assert_eq!(tokio::fs::read(written).await?, b"S");
    tokio::fs::remove_dir_all(fixture.root).await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires fresh packaged Linux bootstrap and the root-owned native probe fixture"]
async fn actual_constructor_refuses_missing_corrupt_and_wrong_arch_policy_before_main()
-> anyhow::Result<()> {
    let fixture = Fixture::new().await?;
    for offset in [None, Some(0), Some(12), Some(16), Some(24), Some(40)] {
        let marker = fixture.cache.join("main-marker");
        let mut prepared = fixture.prepared()?;
        if let Some(offset) = offset {
            prepared.corrupt_policy(offset)?;
        }
        let mut command = prepared.command();
        command.arg("main").arg(&marker);
        if offset.is_none() {
            command.env_remove("DUSKCUE_FFMPEG_POLICY_FD");
        }
        let result = tokio::time::timeout(
            Duration::from_secs(10),
            output(command, prepared, marker.clone()),
        )
        .await?;
        assert!(result.is_err(), "offset {offset:?}");
        assert!(!marker.exists(), "main was entered for offset {offset:?}");
    }
    tokio::fs::remove_dir_all(fixture.root).await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires fresh packaged Linux bootstrap and the root-owned native probe fixture"]
async fn abandoned_managed_observer_terminates_after_stdout_eof_before_output_cleanup()
-> anyhow::Result<()> {
    let fixture = Fixture::new().await?;
    let marker = fixture.cache.join("sleeping-worker");
    let mut prepared = fixture.prepared()?;
    let mut command = prepared.command();
    command.arg("close-sleep").arg(&marker);
    let cleanup_path = marker.clone();
    let task = tokio::spawn(output(command, prepared, cleanup_path));
    let pid: u32 = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Ok(text) = tokio::fs::read_to_string(&marker).await
                && let Ok(pid) = text.trim().parse::<u32>()
            {
                break pid;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await?;
    assert!(PathBuf::from(format!("/proc/{pid}")).exists());
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    tokio::time::timeout(Duration::from_secs(10), async {
        while marker.exists() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await?;
    assert!(!PathBuf::from(format!("/proc/{pid}")).exists());
    tokio::fs::remove_dir_all(fixture.root).await?;
    Ok(())
}

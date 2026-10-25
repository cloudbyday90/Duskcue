// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use tokio::process::Command;

pub async fn send(pid: u32) -> anyhow::Result<()> {
    #[cfg(windows)]
    {
        let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("scripts/testing-memory/send-owned-console-break.ps1");
        let signal = Command::new("pwsh.exe")
            .arg("-NoProfile")
            .arg("-File")
            .arg(script)
            .arg("-TargetPid")
            .arg(pid.to_string())
            .creation_flags(0x08000000)
            .output()
            .await?;
        anyhow::ensure!(
            signal.status.success(),
            "owned console signal failed: {}",
            String::from_utf8_lossy(&signal.stderr)
        );
    }
    #[cfg(unix)]
    anyhow::ensure!(
        Command::new("kill")
            .arg("-TERM")
            .arg(pid.to_string())
            .status()
            .await?
            .success(),
        "owned SIGTERM failed"
    );
    Ok(())
}

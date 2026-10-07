use std::path::{Path, PathBuf};
use std::process::Output;
use std::time::Duration;

use serde_json::{Value, json};
use tokio::process::Command;
use uuid::Uuid;

pub struct GuardedContainer {
    pub name: String,
    resource_id: String,
}

pub async fn command(args: &[String]) -> anyhow::Result<Output> {
    let mut command = Command::new("docker");
    command
        .args(args)
        .kill_on_drop(true)
        .stdin(std::process::Stdio::null());
    #[cfg(target_os = "windows")]
    command.creation_flags(0x08000000);
    let output = tokio::time::timeout(Duration::from_secs(60), command.output()).await??;
    anyhow::ensure!(
        output.stdout.len() + output.stderr.len() <= 2_000_000,
        "fixture command output exceeded its limit"
    );
    Ok(output)
}

impl GuardedContainer {
    pub fn registered() -> anyhow::Result<Self> {
        let resource_id = std::env::var("DUSKCUE_TEST_RESOURCE_ID")?;
        Uuid::parse_str(&resource_id)?;
        let marker = PathBuf::from(std::env::var("DUSKCUE_TEST_RESOURCE_CONTAINERS_FILE")?);
        let expected = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join(".cache/testing-memory")
            .canonicalize()?;
        anyhow::ensure!(
            marker
                .parent()
                .ok_or_else(|| anyhow::anyhow!("missing marker parent"))?
                .canonicalize()?
                == expected
                && marker.file_name().and_then(|value| value.to_str())
                    == Some(format!("{resource_id}.containers.jsonl").as_str()),
            "fixture marker is outside the guarded workflow"
        );
        let name = format!("duskcue-playback-{}", Uuid::now_v7());
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .create(true)
            .open(marker)?;
        use std::io::Write;
        writeln!(file, "{}", json!({"resourceId":resource_id,"name":name}))?;
        Ok(Self { name, resource_id })
    }

    pub fn run_args(&self) -> Vec<String> {
        vec![
            "run".into(),
            "--rm".into(),
            "--pull=never".into(),
            "--name".into(),
            self.name.clone(),
            "--label".into(),
            format!("duskcue.test.resource-id={}", self.resource_id),
            "--memory=512m".into(),
            "--memory-swap=512m".into(),
            "--cpus=2".into(),
            "--pids-limit=128".into(),
        ]
    }

    pub async fn remove(&self) -> anyhow::Result<()> {
        let output = command(&[
            "inspect".into(),
            "--format".into(),
            "{{json .Config.Labels}}".into(),
            self.name.clone(),
        ])
        .await?;
        if !output.status.success() {
            let listed = command(&[
                "ps".into(),
                "-aq".into(),
                "--filter".into(),
                format!("name=^{}$", self.name),
            ])
            .await?;
            anyhow::ensure!(
                listed.status.success() && listed.stdout.is_empty(),
                "owned fixture could not be inspected"
            );
            return Ok(());
        }
        let labels: Value = serde_json::from_slice(&output.stdout)?;
        anyhow::ensure!(
            labels["duskcue.test.resource-id"].as_str() == Some(&self.resource_id),
            "fixture container ownership changed"
        );
        let removed = command(&["rm".into(), "--force".into(), self.name.clone()]).await?;
        anyhow::ensure!(
            removed.status.success(),
            "owned fixture could not be removed"
        );
        Ok(())
    }
}

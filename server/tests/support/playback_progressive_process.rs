// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::MetadataExt;
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::process::Command;

const OUTPUT_LIMIT: u64 = 1_048_576;

#[derive(Default)]
pub(super) struct FixtureProcesses {
    pending: AtomicUsize,
}

impl FixtureProcesses {
    pub(super) fn exited(&self) -> bool {
        self.pending.load(Ordering::Acquire) == 0
    }
    fn spawned(&self) {
        self.pending.fetch_add(1, Ordering::AcqRel);
    }
    fn confirmed_exit(&self) {
        self.pending.fetch_sub(1, Ordering::AcqRel);
    }
}

pub(super) async fn original(
    owned: &FixtureProcesses,
    program: &str,
    args: &[String],
) -> anyhow::Result<Vec<u8>> {
    anyhow::ensure!(
        matches!(program, "ffmpeg" | "ffprobe"),
        "unsupported fixture tool"
    );
    let mut child = Command::new(format!("/usr/bin/{program}"))
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()?;
    owned.spawned();
    let (Some(stdout), Some(stderr)) = (child.stdout.take(), child.stderr.take()) else {
        child.start_kill()?;
        tokio::time::timeout(Duration::from_secs(5), child.wait()).await??;
        owned.confirmed_exit();
        anyhow::bail!("fixture pipes unavailable");
    };
    let outcome = tokio::time::timeout(Duration::from_secs(30), async {
        let (stdout, _, status) = tokio::try_join!(bounded(stdout), bounded(stderr), child.wait())?;
        anyhow::ensure!(status.success(), "owned {program} fixture command failed");
        Ok::<_, anyhow::Error>(stdout)
    })
    .await;
    match outcome {
        Ok(Ok(output)) => {
            owned.confirmed_exit();
            Ok(output)
        }
        failed => {
            if child.try_wait()?.is_none() {
                child.start_kill()?;
                tokio::time::timeout(Duration::from_secs(5), child.wait()).await??;
            }
            owned.confirmed_exit();
            match failed {
                Ok(Err(error)) => Err(error),
                Err(error) => Err(error.into()),
                _ => unreachable!(),
            }
        }
    }
}

async fn bounded(reader: impl AsyncRead + Unpin) -> std::io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    reader
        .take(OUTPUT_LIMIT + 1)
        .read_to_end(&mut bytes)
        .await?;
    if bytes.len() as u64 > OUTPUT_LIMIT {
        return Err(std::io::Error::other(
            "fixture process output exceeded its bound",
        ));
    }
    Ok(bytes)
}

#[derive(Clone)]
pub(super) struct Encoder {
    pub(super) pid: u32,
    parent: u32,
    started: u64,
    executable: PathBuf,
    device: u64,
    inode: u64,
    source: PathBuf,
    manifest: PathBuf,
}

impl Encoder {
    pub(super) fn recorded_identity(&self) -> serde_json::Value {
        serde_json::json!({"pid": self.pid, "parent_pid": self.parent, "started_ticks": self.started})
    }

    pub(super) fn observe(
        source: &Path,
        manifest: &Path,
        seek_ms: Option<i64>,
    ) -> anyhow::Result<Self> {
        let expected = Path::new("/usr/local/libexec/duskcue-ffmpeg").canonicalize()?;
        let expected_meta = std::fs::metadata(&expected)?;
        let mut candidates = Vec::new();
        let mut visited = 0;
        for entry in std::fs::read_dir("/proc")? {
            visited += 1;
            anyhow::ensure!(visited <= 8192, "process inventory exceeded its bound");
            let entry = entry?;
            let Some(pid) = entry
                .file_name()
                .to_str()
                .and_then(|value| value.parse::<u32>().ok())
            else {
                continue;
            };
            let Some((parent, started)) = identity(pid)? else {
                continue;
            };
            if parent != std::process::id() {
                continue;
            }
            let executable = match std::fs::read_link(entry.path().join("exe")) {
                Ok(path) => path,
                Err(_) => continue,
            };
            if executable != expected {
                continue;
            }
            let arguments = limited(&entry.path().join("cmdline"), 65536)?;
            let args = arguments.split(|byte| *byte == 0).collect::<Vec<_>>();
            if !args
                .windows(2)
                .any(|pair| pair[0] == b"-i" && pair[1] == source.as_os_str().as_bytes())
                || !args
                    .iter()
                    .any(|argument| *argument == manifest.as_os_str().as_bytes())
            {
                continue;
            }
            let seek = args
                .windows(2)
                .find(|pair| pair[0] == b"-ss")
                .map(|pair| pair[1]);
            match seek_ms {
                Some(ms) => anyhow::ensure!(
                    seek == Some(format!("{:.3}", ms as f64 / 1000.0).as_bytes()),
                    "actual encoder seek offset differs from HTTP target"
                ),
                None => anyhow::ensure!(seek.is_none(), "unexpected source seek on HTTP start"),
            }
            candidates.push(Self {
                pid,
                parent,
                started,
                executable,
                device: expected_meta.dev(),
                inode: expected_meta.ino(),
                source: source.to_owned(),
                manifest: manifest.to_owned(),
            });
        }
        anyhow::ensure!(
            candidates.len() == 1,
            "requires one physically live exact owned encoder, observed {}",
            candidates.len()
        );
        Ok(candidates.remove(0))
    }

    pub(super) fn alive(&self) -> anyhow::Result<()> {
        anyhow::ensure!(
            identity(self.pid)? == Some((self.parent, self.started)),
            "owned encoder exited or its identity changed"
        );
        let directory = PathBuf::from(format!("/proc/{}", self.pid));
        anyhow::ensure!(
            std::fs::read_link(directory.join("exe"))? == self.executable,
            "owned encoder executable changed"
        );
        let metadata = std::fs::metadata(directory.join("exe"))?;
        anyhow::ensure!(
            metadata.dev() == self.device && metadata.ino() == self.inode,
            "owned encoder executable inode changed"
        );
        let args = limited(&directory.join("cmdline"), 65536)?;
        let args = args.split(|byte| *byte == 0).collect::<Vec<_>>();
        anyhow::ensure!(
            args.windows(2)
                .any(|pair| pair[0] == b"-i" && pair[1] == self.source.as_os_str().as_bytes())
                && args
                    .iter()
                    .any(|argument| *argument == self.manifest.as_os_str().as_bytes()),
            "owned encoder source/cache identity changed"
        );
        Ok(())
    }

    pub(super) fn gone(&self) -> anyhow::Result<()> {
        anyhow::ensure!(
            identity(self.pid)? != Some((self.parent, self.started)),
            "exact owned encoder is still physically present"
        );
        Ok(())
    }
}

fn identity(pid: u32) -> anyhow::Result<Option<(u32, u64)>> {
    let bytes = match limited(Path::new(&format!("/proc/{pid}/stat")), 4096) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    let text = std::str::from_utf8(&bytes)?;
    let (_, values) = text
        .rsplit_once(") ")
        .ok_or_else(|| anyhow::anyhow!("invalid process identity"))?;
    let fields = values.split_ascii_whitespace().collect::<Vec<_>>();
    anyhow::ensure!(fields.len() >= 20, "incomplete process identity");
    Ok(Some((fields[1].parse()?, fields[19].parse()?)))
}

fn limited(path: &Path, limit: u64) -> std::io::Result<Vec<u8>> {
    use std::io::Read;
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(limit + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err(std::io::Error::other("process metadata exceeded its bound"));
    }
    Ok(bytes)
}

// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

use std::path::PathBuf;

use super::*;
use tokio::sync::watch;

const EVENT: &str = "#EXTM3U\n#EXT-X-VERSION:7\n#EXT-X-TARGETDURATION:2\n#EXT-X-PLAYLIST-TYPE:EVENT\n#EXT-X-MAP:URI=\"init.mp4\"\n#EXTINF:2.000,\nseg_0000.m4s\n";

struct Fixture {
    root: PathBuf,
    cache: PathBuf,
    parent: PathBuf,
}

impl Fixture {
    async fn new() -> anyhow::Result<Self> {
        let parent = std::env::current_dir()?.join(".cache/tonight-readiness");
        tokio::fs::create_dir_all(&parent).await?;
        let parent = parent.canonicalize()?;
        let root = parent.join(uuid::Uuid::now_v7().to_string());
        let cache = root.join("cache");
        tokio::fs::create_dir_all(&cache).await?;
        Ok(Self {
            root,
            cache,
            parent,
        })
    }

    async fn assets(&self) -> anyhow::Result<()> {
        tokio::fs::write(self.cache.join("init.mp4"), initialization()).await?;
        tokio::fs::write(self.cache.join("seg_0000.m4s"), fragment()).await?;
        Ok(())
    }

    async fn owned(&self) -> anyhow::Result<OwnedCache> {
        Ok(OwnedCache::new(&self.cache, &self.cache.join("manifest.m3u8")).await?)
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        if let Ok(path) = self.root.canonicalize()
            && path.parent() == Some(self.parent.as_path())
            && path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| uuid::Uuid::parse_str(name).is_ok())
        {
            let _ = std::fs::remove_dir_all(path);
        }
    }
}

fn box_bytes(kind: &[u8; 4], payload: &[u8]) -> Vec<u8> {
    let mut bytes = ((8 + payload.len()) as u32).to_be_bytes().to_vec();
    bytes.extend_from_slice(kind);
    bytes.extend_from_slice(payload);
    bytes
}

fn initialization() -> Vec<u8> {
    [
        box_bytes(b"ftyp", b"iso6\0\0\0\0"),
        box_bytes(b"moov", b"mvex-data"),
    ]
    .concat()
}

fn fragment() -> Vec<u8> {
    [
        box_bytes(b"moof", b"fragment"),
        box_bytes(b"mdat", b"sample"),
    ]
    .concat()
}

fn monitor(
    state: ExecutionState,
) -> (
    watch::Sender<ExecutionState>,
    watch::Sender<u64>,
    ExecutionMonitor,
) {
    let (execution, receiver) = watch::channel(state);
    let (cancellation, requests) = watch::channel(0);
    (
        execution,
        cancellation,
        ExecutionMonitor::new(receiver, requests),
    )
}

#[test]
fn event_and_complete_vod_references_keep_only_flat_owned_assets() {
    let event = playlist::parse(EVENT).unwrap().unwrap();
    assert_eq!(event.publication, playlist::Publication::Event);
    assert!(!event.ended);
    assert_eq!(event.initialization, "init.mp4");
    assert_eq!(event.segment, "seg_0000.m4s");
    let vod = EVENT.replace("EVENT", "VOD");
    assert!(playlist::parse(&vod).unwrap().is_none());
    assert!(
        playlist::parse(&(vod + "#EXT-X-ENDLIST\n"))
            .unwrap()
            .unwrap()
            .ended
    );
    assert!(playlist::parse(EVENT.trim_end()).unwrap().is_none());
}

#[test]
fn malformed_external_traversal_and_extra_resources_are_rejected() {
    for name in [
        "../seg_0000.m4s",
        "https://foreign.invalid/file.m4s",
        "seg_0.m4s?token=secret",
        "seg_%2e.m4s",
        "child/seg_0.m4s",
        "seg_.m4s",
        "seg_0.m4s\\child",
    ] {
        assert!(playlist::parse(&EVENT.replace("seg_0000.m4s", name)).is_err());
    }
    for addition in [
        "#EXT-X-KEY:METHOD=AES-128,URI=\"key.bin\"\n",
        "#EXT-X-STREAM-INF:BANDWIDTH=1\n",
        "#EXT-X-BYTERANGE:100@0\n",
    ] {
        assert!(playlist::parse(&(EVENT.to_string() + addition)).is_err());
    }
    for duration in ["0", "-1", "NaN", "inf"] {
        assert!(playlist::parse(&EVENT.replace("2.000", duration)).is_err());
    }
    assert!(playlist::parse(&EVENT.replace("init.mp4", "../init.mp4")).is_err());
}

#[tokio::test]
async fn atomic_missing_to_ready_publication_is_observed_while_producer_runs() -> anyhow::Result<()>
{
    let fixture = Fixture::new().await?;
    let (_execution, _cancellation, mut monitor) = monitor(ExecutionState::Running);
    let publish = async {
        tokio::time::sleep(Duration::from_millis(20)).await;
        fixture.assets().await?;
        let temporary = fixture.cache.join("manifest.m3u8.tmp");
        tokio::fs::write(&temporary, EVENT).await?;
        tokio::fs::rename(temporary, fixture.cache.join("manifest.m3u8")).await?;
        Ok::<_, anyhow::Error>(())
    };
    let manifest = fixture.cache.join("manifest.m3u8");
    let (ready, published) = tokio::join!(
        wait(
            &fixture.cache,
            &manifest,
            &mut monitor,
            Instant::now() + Duration::from_secs(2)
        ),
        publish
    );
    published?;
    ready?;
    assert_eq!(monitor.state(), ExecutionState::Running);
    Ok(())
}

#[tokio::test]
async fn referenced_assets_require_complete_box_extents_and_successful_vod_publisher()
-> anyhow::Result<()> {
    let fixture = Fixture::new().await?;
    tokio::fs::write(fixture.cache.join("manifest.m3u8"), EVENT).await?;
    let cache = fixture.owned().await?;
    assert!(!probe(&cache, ExecutionState::Running).await?);
    fixture.assets().await?;
    let mut partial = fragment();
    partial.pop();
    tokio::fs::write(fixture.cache.join("seg_0000.m4s"), partial).await?;
    assert!(!probe(&cache, ExecutionState::Running).await?);
    fixture.assets().await?;
    assert!(probe(&cache, ExecutionState::Running).await?);
    let vod = EVENT.replace("EVENT", "VOD") + "#EXT-X-ENDLIST\n";
    tokio::fs::write(fixture.cache.join("manifest.m3u8"), vod).await?;
    assert!(!probe(&cache, ExecutionState::Running).await?);
    assert!(probe(&cache, ExecutionState::Succeeded).await?);
    Ok(())
}

#[tokio::test]
async fn finished_failure_cancellation_and_missing_assets_never_acknowledge() -> anyhow::Result<()>
{
    let fixture = Fixture::new().await?;
    for state in [
        ExecutionState::Failed,
        ExecutionState::Cancelled,
        ExecutionState::Succeeded,
    ] {
        let (_execution, _cancellation, mut monitor) = monitor(state);
        assert!(
            wait(
                &fixture.cache,
                &fixture.cache.join("manifest.m3u8"),
                &mut monitor,
                Instant::now() + Duration::from_secs(2)
            )
            .await
            .is_err()
        );
    }
    fixture.assets().await?;
    tokio::fs::write(fixture.cache.join("manifest.m3u8"), EVENT).await?;
    let (execution, cancellation, mut monitor) = monitor(ExecutionState::Succeeded);
    cancellation.send_replace(1);
    execution.send_replace(ExecutionState::Succeeded);
    assert_eq!(monitor.state(), ExecutionState::Cancelled);
    assert!(
        wait(
            &fixture.cache,
            &fixture.cache.join("manifest.m3u8"),
            &mut monitor,
            Instant::now() + Duration::from_secs(2)
        )
        .await
        .is_err()
    );
    Ok(())
}

#[tokio::test]
async fn owned_reader_refuses_unbounded_and_unowned_resources() -> anyhow::Result<()> {
    let fixture = Fixture::new().await?;
    let cache = fixture.owned().await?;
    assert!(cache.open("../outside").await.is_err());
    let file = tokio::fs::File::create(fixture.cache.join("manifest.m3u8")).await?;
    file.set_len(MAX_PLAYLIST_BYTES + 1).await?;
    assert!(probe(&cache, ExecutionState::Running).await.is_err());
    drop(file);
    #[cfg(unix)]
    {
        let outside = fixture.root.join("outside");
        tokio::fs::write(&outside, initialization()).await?;
        std::os::unix::fs::symlink(outside, fixture.cache.join("init.mp4"))?;
        assert!(cache.open("init.mp4").await.is_err());
    }
    Ok(())
}

// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use std::time::{Duration, UNIX_EPOCH};

use super::media_source::MediaFileDetails;
use crate::services::source_capabilities::ScanIdentity;

pub(super) async fn matches(details: &MediaFileDetails, scan: &ScanIdentity) -> bool {
    if super::inspection::active() {
        let details = details.clone();
        let scan = scan.clone();
        return super::inspection::run(move || {
            let mut options = std::fs::OpenOptions::new();
            options.read(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
            }
            let file = options.open(&details.file_path).ok()?;
            Some(current(&details, &scan, &file.metadata().ok()?))
        })
        .await
        .unwrap_or(false);
    }
    tokio::time::timeout(Duration::from_secs(2), inspect(details, scan))
        .await
        .unwrap_or(false)
}

async fn inspect(details: &MediaFileDetails, scan: &ScanIdentity) -> bool {
    let mut options = tokio::fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    let Ok(file) = options.open(&details.file_path).await else {
        return false;
    };
    let Ok(metadata) = file.metadata().await else {
        return false;
    };
    current(details, scan, &metadata)
}

fn current(details: &MediaFileDetails, scan: &ScanIdentity, metadata: &std::fs::Metadata) -> bool {
    let Some(scanned_modified) = details.file_modified_at else {
        return false;
    };
    let Some(modified) = metadata
        .modified()
        .ok()
        .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
    else {
        return false;
    };
    metadata.is_file()
        && metadata.len() > 0
        && u64::try_from(details.file_size).ok() == Some(metadata.len())
        && scan.size == metadata.len()
        && scan.modified_ns == modified.as_nanos().to_string()
        && i128::from(scanned_modified.timestamp_micros())
            == i128::try_from(modified.as_micros()).unwrap_or(-1)
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Directory(std::path::PathBuf);

    impl Drop for Directory {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[tokio::test]
    async fn current_file_must_match_both_scanner_snapshot_and_database_precision()
    -> anyhow::Result<()> {
        let directory = Directory(
            std::env::temp_dir().join(format!("duskcue-source-freshness-{}", uuid::Uuid::now_v7())),
        );
        tokio::fs::create_dir(&directory.0).await?;
        let path = directory.0.join("source.mp4");
        tokio::fs::write(&path, b"media").await?;
        let metadata = tokio::fs::metadata(&path).await?;
        let scan = ScanIdentity::from_metadata(&metadata).unwrap();
        let modified: chrono::DateTime<chrono::Utc> = metadata.modified()?.into();
        let mut details = crate::domains::playback::media_source::MediaFileDetails {
            id: uuid::Uuid::now_v7(),
            file_size: 5,
            updated_at: chrono::Utc::now(),
            file_modified_at: Some(modified),
            file_path: path.to_string_lossy().into_owned(),
            container_format: "mp4".into(),
            video_codec: None,
            video_resolution: None,
            video_bitrate: None,
            video_dynamic_range: None,
            video_frame_rate: None,
            audio_codec: None,
            audio_channels: None,
            audio_language: None,
            audio_bitrate: None,
            runtime_seconds: 0,
            additional_streams: serde_json::json!({}),
        };
        assert!(matches(&details, &scan).await);
        details.file_modified_at = Some(modified - chrono::Duration::microseconds(1));
        assert!(!matches(&details, &scan).await);
        details.file_modified_at = Some(modified);
        tokio::fs::write(&path, b"changed file").await?;
        assert!(!matches(&details, &scan).await);
        details.file_size = 12;
        assert!(!matches(&details, &scan).await);
        details.file_modified_at = None;
        assert!(!matches(&details, &scan).await);
        Ok(())
    }
}

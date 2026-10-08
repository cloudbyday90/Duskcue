// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use std::os::unix::fs::{MetadataExt, PermissionsExt};

use super::*;

#[derive(Debug, PartialEq, Eq)]
struct DirectorySnapshot {
    inode: u64,
    mode: u32,
    modified_seconds: i64,
    modified_nanoseconds: i64,
    entries: Vec<std::ffi::OsString>,
}

async fn snapshot(parent: &std::path::Path) -> anyhow::Result<DirectorySnapshot> {
    let metadata = tokio::fs::metadata(parent).await?;
    let mut children = tokio::fs::read_dir(parent).await?;
    let mut entries = Vec::new();
    while let Some(child) = children.next_entry().await? {
        anyhow::ensure!(
            entries.len() < 64,
            "owned parent inventory exceeded its bound"
        );
        entries.push(child.file_name());
    }
    entries.sort();
    Ok(DirectorySnapshot {
        inode: metadata.ino(),
        mode: metadata.permissions().mode() & 0o7777,
        modified_seconds: metadata.mtime(),
        modified_nanoseconds: metadata.mtime_nsec(),
        entries,
    })
}

pub(super) async fn verify_no_cache_directory_creation(fixture: &Fixture) -> anyhow::Result<()> {
    for parent in [&fixture.cache, &fixture.root] {
        let original = snapshot(parent).await?;
        let source_mode = tokio::fs::metadata(&fixture.source)
            .await?
            .permissions()
            .mode();
        let outside_mode = tokio::fs::metadata(&fixture.outside)
            .await?
            .permissions()
            .mode();
        for probe in ["directory-at", "directory-legacy"] {
            let destination = parent.join(probe);
            assert!(!destination.exists());
            let denied = fixture
                .invoke(probe, std::slice::from_ref(&destination))
                .await?;
            anyhow::ensure!(denied.status.success(), "directory creation denial {probe}");
            assert_eq!(denied.stdout, b"DIRECTORY_CREATION_DENIED\n");
            assert!(!destination.exists());
            assert_eq!(snapshot(parent).await?, original);
            assert_eq!(tokio::fs::read(&fixture.source).await?, b"S");
            assert_eq!(tokio::fs::read(&fixture.outside).await?, b"O");
            assert_eq!(
                tokio::fs::metadata(&fixture.source)
                    .await?
                    .permissions()
                    .mode(),
                source_mode
            );
            assert_eq!(
                tokio::fs::metadata(&fixture.outside)
                    .await?
                    .permissions()
                    .mode(),
                outside_mode
            );
        }
        for probe in [
            "directory-at-mode",
            "directory-at-fd",
            "directory-legacy-mode",
        ] {
            let destination = parent.join(probe);
            assert!(!destination.exists());
            let denied = fixture
                .invoke(probe, std::slice::from_ref(&destination))
                .await?;
            assert_eq!(denied.status.signal(), Some(libc::SIGSYS), "probe {probe}");
            assert!(!destination.exists());
            assert_eq!(snapshot(parent).await?, original);
            assert_eq!(tokio::fs::read(&fixture.source).await?, b"S");
            assert_eq!(tokio::fs::read(&fixture.outside).await?, b"O");
            assert_eq!(
                tokio::fs::metadata(&fixture.source)
                    .await?
                    .permissions()
                    .mode(),
                source_mode
            );
            assert_eq!(
                tokio::fs::metadata(&fixture.outside)
                    .await?
                    .permissions()
                    .mode(),
                outside_mode
            );
        }
    }
    Ok(())
}

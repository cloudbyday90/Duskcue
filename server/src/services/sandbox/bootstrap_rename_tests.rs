// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use std::os::unix::fs::PermissionsExt;

use super::*;

async fn unchanged_file(path: &std::path::Path, contents: &[u8], mode: u32) -> anyhow::Result<()> {
    assert_eq!(tokio::fs::read(path).await?, contents);
    assert_eq!(
        tokio::fs::metadata(path).await?.permissions().mode() & 0o7777,
        mode
    );
    Ok(())
}

async fn refused(
    fixture: &Fixture,
    mode: &str,
    source: &std::path::Path,
    destination: &std::path::Path,
) -> anyhow::Result<()> {
    let result = fixture
        .invoke(mode, &[source.to_path_buf(), destination.to_path_buf()])
        .await?;
    anyhow::ensure!(result.status.success(), "rename denial probe {mode}");
    assert_eq!(result.stdout, b"RENAME_DENIED\n");
    Ok(())
}

pub(super) async fn verify_atomic_file_publication(fixture: &Fixture) -> anyhow::Result<()> {
    for mode in ["rename-file", "rename-legacy"] {
        let source = fixture.cache.join(format!("{mode}.tmp"));
        let destination = fixture.cache.join(format!("{mode}.m3u8"));
        tokio::fs::write(&source, b"NEW").await?;
        tokio::fs::write(&destination, b"OLD").await?;
        let result = fixture
            .invoke(mode, &[source.clone(), destination.clone()])
            .await?;
        anyhow::ensure!(result.status.success(), "atomic publication probe {mode}");
        assert_eq!(result.stdout, b"FILE_PUBLISHED\n");
        assert!(!source.exists());
        assert_eq!(tokio::fs::read(destination).await?, b"NEW");
    }
    let source = fixture.cache.join("preserved-publication-source");
    let destination = fixture.cache.join("preserved-publication-destination");
    tokio::fs::write(&source, b"S").await?;
    tokio::fs::write(&destination, b"D").await?;
    tokio::fs::set_permissions(&source, std::fs::Permissions::from_mode(0o600)).await?;
    tokio::fs::set_permissions(&destination, std::fs::Permissions::from_mode(0o640)).await?;
    for mode in ["rename-denied", "rename-legacy-denied"] {
        refused(fixture, mode, &source, &fixture.outside).await?;
        refused(fixture, mode, &fixture.outside, &destination).await?;
        unchanged_file(&source, b"S", 0o600).await?;
        unchanged_file(&destination, b"D", 0o640).await?;
        unchanged_file(&fixture.outside, b"O", 0o600).await?;
    }
    for mode in ["rename-source-fd", "rename-destination-fd", "rename-flags"] {
        let result = fixture
            .invoke(mode, &[source.clone(), destination.clone()])
            .await?;
        assert_eq!(result.status.signal(), Some(libc::SIGSYS), "probe {mode}");
        unchanged_file(&source, b"S", 0o600).await?;
        unchanged_file(&destination, b"D", 0o640).await?;
    }
    let temporary =
        PathBuf::from("/tmp").join(format!("duskcue-publication-{}", uuid::Uuid::now_v7()));
    tokio::fs::create_dir(&temporary).await?;
    let proof = async {
        for parent in [&fixture.cache, &temporary] {
            let source = parent.join("directory-source");
            let destination = parent.join("directory-destination");
            tokio::fs::create_dir(&source).await?;
            tokio::fs::create_dir(&destination).await?;
            tokio::fs::write(source.join("preserved-content"), b"F").await?;
            tokio::fs::set_permissions(
                source.join("preserved-content"),
                std::fs::Permissions::from_mode(0o600),
            )
            .await?;
            tokio::fs::set_permissions(&source, std::fs::Permissions::from_mode(0o700)).await?;
            tokio::fs::set_permissions(&destination, std::fs::Permissions::from_mode(0o750))
                .await?;
            for mode in ["rename-denied", "rename-legacy-denied"] {
                refused(fixture, mode, &source, &destination).await?;
                assert!(source.is_dir() && destination.is_dir());
                unchanged_file(&source.join("preserved-content"), b"F", 0o600).await?;
                assert_eq!(
                    tokio::fs::metadata(&source).await?.permissions().mode() & 0o7777,
                    0o700
                );
                assert_eq!(
                    tokio::fs::metadata(&destination)
                        .await?
                        .permissions()
                        .mode()
                        & 0o7777,
                    0o750
                );
                assert!(
                    tokio::fs::read_dir(&destination)
                        .await?
                        .next_entry()
                        .await?
                        .is_none()
                );
            }
        }
        Ok::<_, anyhow::Error>(())
    }
    .await;
    let cleanup = tokio::fs::remove_dir_all(&temporary).await;
    proof?;
    cleanup?;
    assert!(!temporary.exists());
    Ok(())
}

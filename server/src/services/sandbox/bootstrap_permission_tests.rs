// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use std::os::unix::fs::PermissionsExt;

use super::*;

pub(super) async fn verify_unchanged_permissions(fixture: &Fixture) -> anyhow::Result<()> {
    let inside = fixture.cache.join("permission-file");
    tokio::fs::write(&inside, b"P").await?;
    let inside_directory = fixture.cache.join("permission-directory");
    let outside_directory = fixture.root.join("outside-permission-directory");
    tokio::fs::create_dir(&inside_directory).await?;
    tokio::fs::create_dir(&outside_directory).await?;
    let targets = [
        (&inside, 0o640, Some(b"P".as_slice())),
        (&fixture.outside, 0o600, Some(b"O".as_slice())),
        (&inside_directory, 0o700, None),
        (&outside_directory, 0o700, None),
    ];
    for (target, mode, contents) in targets {
        tokio::fs::set_permissions(target, std::fs::Permissions::from_mode(mode)).await?;
        for probe in ["permission-at", "permission-legacy"] {
            let denied = fixture.invoke(probe, std::slice::from_ref(target)).await?;
            anyhow::ensure!(denied.status.success(), "probe {probe} at {target:?}");
            assert_eq!(denied.stdout, b"PERMISSION_CHANGE_DENIED\n");
            assert_eq!(
                tokio::fs::metadata(target).await?.permissions().mode() & 0o7777,
                mode
            );
            if let Some(contents) = contents {
                assert_eq!(tokio::fs::read(target).await?, contents);
            } else {
                assert!(target.is_dir());
            }
        }
    }
    for probe in [
        "permission-at-mode",
        "permission-at-fd",
        "permission-legacy-mode",
        "permission-fd",
    ] {
        let denied = fixture.invoke(probe, std::slice::from_ref(&inside)).await?;
        assert_eq!(denied.status.signal(), Some(libc::SIGSYS), "probe {probe}");
        assert_eq!(
            tokio::fs::metadata(&inside).await?.permissions().mode() & 0o7777,
            0o640
        );
        assert_eq!(tokio::fs::read(&inside).await?, b"P");
    }
    Ok(())
}

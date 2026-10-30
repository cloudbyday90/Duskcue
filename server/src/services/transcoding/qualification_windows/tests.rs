// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::{api, files::PinnedFile, process::OwnedEncoder};
use std::path::PathBuf;

struct Directory {
    root: PathBuf,
    source: PathBuf,
}
impl Directory {
    fn new() -> anyhow::Result<Self> {
        let parent = std::env::temp_dir().canonicalize()?;
        let root = parent.join(format!("duskcue-windows-pacing-{}", uuid::Uuid::now_v7()));
        std::fs::create_dir(&root)?;
        let source = root.join("source.mkv");
        std::fs::write(&source, b"owned source")?;
        Ok(Self { root, source })
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.source);
        let _ = std::fs::remove_dir(&self.root);
    }
}

#[tokio::test]
async fn pinned_file_refuses_writes_deletion_and_releases_after_actual_drop() -> anyhow::Result<()>
{
    let f = Directory::new()?;
    let pinned = PinnedFile::open(&f.source, 1024).await?;
    assert!(
        std::fs::OpenOptions::new()
            .write(true)
            .open(&f.source)
            .is_err()
    );
    assert!(std::fs::remove_file(&f.source).is_err());
    pinned.verify(1024).await?;
    drop(pinned);
    std::fs::write(&f.source, b"released")?;
    std::fs::remove_file(&f.source)?;
    Ok(())
}

#[tokio::test]
async fn pinned_file_refuses_alias_empty_oversized_and_directory_inputs() -> anyhow::Result<()> {
    let f = Directory::new()?;
    let ordinary = f.source.to_str().unwrap().strip_prefix(r"\\?\").unwrap();
    assert!(
        PinnedFile::open(std::path::Path::new(ordinary), 1024)
            .await
            .is_err()
    );
    assert!(PinnedFile::open(&f.root, 1024).await.is_err());
    assert!(PinnedFile::open(&f.source, 1).await.is_err());
    std::fs::write(&f.source, [])?;
    assert!(PinnedFile::open(&f.source, 1024).await.is_err());
    Ok(())
}

#[test]
fn process_identity_and_thread_owner_checks_refuse_unowned_targets() -> anyhow::Result<()> {
    let pid = std::process::id();
    let process = api::process(pid)?;
    assert!(api::alive(&process)?);
    assert!(api::created(&process)? > 0);
    assert_eq!(
        api::image(&process)?.canonicalize()?,
        std::env::current_exe()?.canonicalize()?
    );
    assert_ne!(api::parent(pid)?, pid);
    assert!(
        OwnedEncoder::capture(
            pid,
            &std::env::current_exe()?.canonicalize()?,
            std::path::Path::new("source"),
            std::path::Path::new("manifest"),
            &[]
        )
        .is_err()
    );
    let threads = api::threads(pid)?;
    assert!(!threads.is_empty());
    assert!(api::thread(threads[0], pid.wrapping_add(1)).is_err());
    assert!(api::process(0).is_err());
    assert!(api::alive(&process)?);
    Ok(())
}

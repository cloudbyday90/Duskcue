// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use std::fs::Metadata;
use std::path::Path;
use std::time::UNIX_EPOCH;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScanIdentity {
    pub size: u64,
    pub modified_ns: String,
}

impl ScanIdentity {
    pub fn from_metadata(metadata: &Metadata) -> Option<Self> {
        if !metadata.is_file() || metadata.len() == 0 {
            return None;
        }
        Some(Self {
            size: metadata.len(),
            modified_ns: metadata
                .modified()
                .ok()?
                .duration_since(UNIX_EPOCH)
                .ok()?
                .as_nanos()
                .to_string(),
        })
    }

    pub fn matches_metadata(&self, metadata: &Metadata) -> bool {
        Self::from_metadata(metadata).as_ref() == Some(self)
    }

    pub(super) fn valid(&self) -> bool {
        self.modified_ns.len() <= 30
            && self
                .modified_ns
                .parse::<u128>()
                .is_ok_and(|value| value.to_string() == self.modified_ns)
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct ScanSnapshot {
    pub identity: ScanIdentity,
    created: Option<std::time::SystemTime>,
    #[cfg(unix)]
    unix: (u64, u64, i64, i64),
}

impl ScanSnapshot {
    pub(crate) async fn read(path: &Path) -> Option<Self> {
        let mut options = tokio::fs::OpenOptions::new();
        options.read(true);
        #[cfg(unix)]
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
        let file = options.open(path).await.ok()?;
        let metadata = file.metadata().await.ok()?;
        Some(Self {
            identity: ScanIdentity::from_metadata(&metadata)?,
            created: metadata.created().ok(),
            #[cfg(unix)]
            unix: {
                use std::os::unix::fs::MetadataExt;
                (
                    metadata.dev(),
                    metadata.ino(),
                    metadata.ctime(),
                    metadata.ctime_nsec(),
                )
            },
        })
    }
}

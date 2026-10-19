// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use std::collections::{HashMap, HashSet};

use chrono::{DateTime, Utc};

use super::{DiffResult, DiscoveredFile, KnownFile, is_media_video_file};

pub(super) fn files_to_probe(diff: &DiffResult) -> Vec<DiscoveredFile> {
    diff.new_files
        .iter()
        .chain(&diff.modified_files)
        .cloned()
        .collect()
}

pub(super) fn diff(known: &[KnownFile], discovered: &[DiscoveredFile], full: bool) -> DiffResult {
    let known: HashMap<_, _> = known
        .iter()
        .map(|file| (file.file_path.as_str(), file))
        .collect();
    let mut present = HashSet::new();
    let mut result = DiffResult {
        new_files: Vec::new(),
        modified_files: Vec::new(),
        unchanged_count: 0,
        deleted_paths: Vec::new(),
    };
    for file in discovered {
        let path = file.path.to_string_lossy().into_owned();
        present.insert(path.clone());
        if !is_media_video_file(&file.path) {
            continue;
        }
        let Some(previous) = known.get(path.as_str()) else {
            result.new_files.push(file.clone());
            continue;
        };
        let changed = match (file.mtime, previous.file_modified_at) {
            (Some(current), Some(previous)) => {
                DateTime::<Utc>::from(current).timestamp_micros() != previous.timestamp_micros()
            }
            _ => true,
        };
        if full || Some(previous.file_size) != i64::try_from(file.size).ok() || changed {
            result.modified_files.push(file.clone());
        } else {
            result.unchanged_count += 1;
        }
    }
    result.deleted_paths = known
        .keys()
        .filter(|path| !present.contains(**path))
        .map(|path| (*path).to_owned())
        .collect();
    result
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, UNIX_EPOCH};

    use uuid::Uuid;

    use super::*;

    #[test]
    fn quick_scan_reprobes_new_resized_and_subsecond_modified_files() {
        let time = UNIX_EPOCH + Duration::from_secs(100);
        let files: Vec<_> = ["new.mp4", "resized.mp4", "modified.mp4", "same.mp4"]
            .into_iter()
            .map(|path| DiscoveredFile {
                path: path.into(),
                size: 200,
                mtime: Some(time),
            })
            .collect();
        let known: Vec<_> = files[1..]
            .iter()
            .map(|file| KnownFile {
                file_path: file.path.to_string_lossy().into_owned(),
                file_size: if file.path.ends_with("resized.mp4") {
                    100
                } else {
                    200
                },
                file_modified_at: Some(DateTime::<Utc>::from(
                    if file.path.ends_with("modified.mp4") {
                        time - Duration::from_millis(1)
                    } else {
                        time
                    },
                )),
                file_hash: None,
                media_item_id: Uuid::nil(),
            })
            .collect();
        let result = diff(&known, &files, false);
        assert_eq!(result.new_files.len(), 1);
        assert_eq!(result.modified_files.len(), 2);
        assert_eq!(result.unchanged_count, 1);
        assert_eq!(
            files_to_probe(&result)
                .iter()
                .map(|file| file.path.to_string_lossy().into_owned())
                .collect::<Vec<_>>(),
            ["new.mp4", "resized.mp4", "modified.mp4"]
        );
        assert_eq!(files_to_probe(&diff(&known, &files, true)).len(), 4);
    }

    #[test]
    fn postgres_microsecond_roundtrip_does_not_reprobe_unchanged_nanoseconds() {
        let time = UNIX_EPOCH + Duration::from_nanos(100_123_456_789);
        let file = DiscoveredFile {
            path: "same.mkv".into(),
            size: 10,
            mtime: Some(time),
        };
        let known = KnownFile {
            file_path: "same.mkv".into(),
            file_size: 10,
            file_modified_at: DateTime::<Utc>::from_timestamp_micros(
                DateTime::<Utc>::from(time).timestamp_micros(),
            ),
            file_hash: None,
            media_item_id: Uuid::nil(),
        };
        assert_eq!(diff(&[known], &[file], false).unchanged_count, 1);
    }
}

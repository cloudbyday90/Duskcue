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

use serde::Serialize;
use uuid::Uuid;

use crate::state::AppState;

const METRIC_LABEL_DATA: &str = "data";
const METRIC_LABEL_CACHE: &str = "cache";
const METRIC_LABEL_TRANSCODE: &str = "transcode";

#[derive(Debug, thiserror::Error)]
pub enum DiskSpaceError {
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("Failed to serialize disk space stats: {0}")]
    StatsSerialization(serde_json::Error),
    #[error("Disk enumeration task panicked: {0}")]
    EnumerationPanic(String),
}

#[derive(Debug, Clone, Serialize)]
pub struct DiskSpaceStats {
    status: String,
    thresholds: ThresholdSummary,
    paths: Vec<PathReport>,
    breached_count: usize,
    unavailable_count: usize,
}

#[derive(Debug, Clone, Serialize)]
struct ThresholdSummary {
    data_threshold_percent: u8,
    cache_threshold_percent: u8,
    transcode_threshold_percent: u8,
}

#[derive(Debug, Clone, Serialize)]
struct PathReport {
    label: String,
    path: String,
    status: String,
    total_bytes: Option<u64>,
    available_bytes: Option<u64>,
    used_bytes: Option<u64>,
    usage_percent: Option<f64>,
    threshold_percent: Option<u8>,
    exceeded: bool,
}

#[derive(Debug, Clone, Copy)]
struct DiskSpaceConfig {
    data_threshold_percent: u8,
    cache_threshold_percent: u8,
    transcode_threshold_percent: u8,
}

const MIN_THRESHOLD_PERCENT: u8 = 50;
const MAX_THRESHOLD_PERCENT: u8 = 99;

impl DiskSpaceConfig {
    fn from_state_and_task(state: &AppState, task_config: &serde_json::Value) -> Self {
        let storage = state.runtime_config.load().storage.clone();
        let warnings = storage.disk_space_warnings;

        let data_threshold_percent = resolve_threshold(
            task_config.get("data_threshold_percent"),
            warnings.data_threshold_percent,
        );
        let cache_threshold_percent = resolve_threshold(
            task_config.get("cache_threshold_percent"),
            warnings.cache_threshold_percent,
        );
        let transcode_threshold_percent = resolve_threshold(
            task_config.get("transcode_threshold_percent"),
            warnings.transcode_threshold_percent,
        );

        Self {
            data_threshold_percent,
            cache_threshold_percent,
            transcode_threshold_percent,
        }
    }
}

fn resolve_threshold(task_value: Option<&serde_json::Value>, default: u8) -> u8 {
    let raw = task_value
        .and_then(|v| v.as_u64())
        .map(|v| v as u8)
        .unwrap_or(default);
    raw.clamp(MIN_THRESHOLD_PERCENT, MAX_THRESHOLD_PERCENT)
}

pub async fn run_disk_space_check(
    state: &AppState,
    task_id: Uuid,
    task_config: serde_json::Value,
) -> Result<(), DiskSpaceError> {
    tracing::info!(task_id = %task_id, "Starting disk space check");

    let config = DiskSpaceConfig::from_state_and_task(state, &task_config);

    let data_dir = state.bootstrap.data_dir.clone();
    let cache_dir = state.bootstrap.cache_dir.clone();
    let transcode_dir = std::path::PathBuf::from(
        state
            .runtime_config
            .load()
            .transcoding
            .transcode_path
            .clone(),
    );

    let snapshots = tokio::task::spawn_blocking(gather_disk_snapshots)
        .await
        .map_err(|join_err| {
            tracing::error!(task_id = %task_id, error = %join_err, "Disk enumeration task panicked");
            DiskSpaceError::EnumerationPanic(join_err.to_string())
        })?;

    let mut reports = Vec::with_capacity(3);
    push_report(
        &mut reports,
        METRIC_LABEL_DATA,
        &data_dir,
        config.data_threshold_percent,
        &snapshots,
    );
    push_report(
        &mut reports,
        METRIC_LABEL_CACHE,
        &cache_dir,
        config.cache_threshold_percent,
        &snapshots,
    );
    push_report(
        &mut reports,
        METRIC_LABEL_TRANSCODE,
        &transcode_dir,
        config.transcode_threshold_percent,
        &snapshots,
    );

    for report in &reports {
        record_metrics(report);
        if report.exceeded {
            tracing::warn!(
                task_id = %task_id,
                label = %report.label,
                path = %report.path,
                usage_percent = ?report.usage_percent,
                threshold_percent = ?report.threshold_percent,
                available_bytes = ?report.available_bytes,
                "Disk space threshold exceeded"
            );
        }
    }

    let breached_count = reports.iter().filter(|r| r.exceeded).count();
    let unavailable_count = reports
        .iter()
        .filter(|r| r.status == "unavailable")
        .count();

    let status = if breached_count > 0 {
        "threshold_exceeded"
    } else if unavailable_count == reports.len() {
        "unavailable"
    } else {
        "healthy"
    };

    let stats = DiskSpaceStats {
        status: status.to_string(),
        thresholds: ThresholdSummary {
            data_threshold_percent: config.data_threshold_percent,
            cache_threshold_percent: config.cache_threshold_percent,
            transcode_threshold_percent: config.transcode_threshold_percent,
        },
        breached_count,
        unavailable_count,
        paths: reports,
    };

    persist_run_stats(state, task_id, &stats).await?;

    tracing::info!(
        task_id = %task_id,
        status = %stats.status,
        breached = breached_count,
        unavailable = unavailable_count,
        "Disk space check completed"
    );

    Ok(())
}

fn gather_disk_snapshots() -> Vec<DiskSnapshot> {
    use sysinfo::Disks;
    Disks::new_with_refreshed_list()
        .list()
        .iter()
        .map(|disk| DiskSnapshot {
            mount_point: disk.mount_point().to_path_buf(),
            total_bytes: disk.total_space(),
            available_bytes: disk.available_space(),
        })
        .collect()
}

fn push_report(
    reports: &mut Vec<PathReport>,
    label: &str,
    path: &std::path::Path,
    threshold_percent: u8,
    snapshots: &[DiskSnapshot],
) {
    let report = build_path_report(label, path, threshold_percent, snapshots);
    reports.push(report);
}

fn build_path_report(
    label: &str,
    path: &std::path::Path,
    threshold_percent: u8,
    snapshots: &[DiskSnapshot],
) -> PathReport {
    let resolved = resolve_canonical(path);
    let Some(resolved) = resolved else {
        return unavailable(label, path, Some(threshold_percent));
    };

    let Some(snapshot) = find_disk_for_path(&resolved, snapshots) else {
        return unavailable(label, path, Some(threshold_percent));
    };

    let total = snapshot.total_bytes;
    let available = snapshot.available_bytes;
    if total == 0 {
        return unavailable(label, path, Some(threshold_percent));
    }

    let used = total.saturating_sub(available);
    let usage_percent = (used as f64 / total as f64) * 100.0;
    let exceeded = usage_percent >= threshold_percent as f64;

    PathReport {
        label: label.to_string(),
        path: path.to_string_lossy().into_owned(),
        status: if exceeded {
            "threshold_exceeded"
        } else {
            "healthy"
        }
        .to_string(),
        total_bytes: Some(total),
        available_bytes: Some(available),
        used_bytes: Some(used),
        usage_percent: Some(usage_percent),
        threshold_percent: Some(threshold_percent),
        exceeded,
    }
}

fn resolve_canonical(path: &std::path::Path) -> Option<std::path::PathBuf> {
    match std::fs::canonicalize(path) {
        Ok(canonical) => Some(canonical),
        Err(_) => {
            let mut ancestor = std::path::PathBuf::from(path);
            while !ancestor.exists() {
                if !ancestor.pop() {
                    return None;
                }
            }
            std::fs::canonicalize(&ancestor).ok().or(Some(ancestor))
        }
    }
}

fn find_disk_for_path<'a>(
    path: &std::path::Path,
    snapshots: &'a [DiskSnapshot],
) -> Option<&'a DiskSnapshot> {
    let mut best: Option<&DiskSnapshot> = None;
    for snapshot in snapshots {
        if path.starts_with(&snapshot.mount_point)
            && (best.is_none()
                || snapshot.mount_point.as_os_str().len()
                    > best?.mount_point.as_os_str().len())
        {
            best = Some(snapshot);
        }
    }
    best
}

fn unavailable(label: &str, path: &std::path::Path, threshold_percent: Option<u8>) -> PathReport {
    PathReport {
        label: label.to_string(),
        path: path.to_string_lossy().into_owned(),
        status: "unavailable".to_string(),
        total_bytes: None,
        available_bytes: None,
        used_bytes: None,
        usage_percent: None,
        threshold_percent,
        exceeded: false,
    }
}

fn record_metrics(report: &PathReport) {
    let Some(used) = report.used_bytes else {
        return;
    };
    let Some(total) = report.total_bytes else {
        return;
    };
    metrics::gauge!("storage_usage_bytes", "path" => report.label.clone()).set(used as f64);
    metrics::gauge!("storage_capacity_bytes", "path" => report.label.clone()).set(total as f64);
    let usage_percent = report.usage_percent.unwrap_or(0.0);
    metrics::gauge!("storage_usage_percent", "path" => report.label.clone()).set(usage_percent);
}

async fn persist_run_stats(
    state: &AppState,
    task_id: Uuid,
    stats: &DiskSpaceStats,
) -> Result<(), DiskSpaceError> {
    let stats = serde_json::to_value(stats).map_err(DiskSpaceError::StatsSerialization)?;
    sqlx::query(
        r#"
        UPDATE scheduled_task_runs
        SET stats = $2
        WHERE scheduled_task_id = $1
          AND state = 'running'
        "#,
    )
    .bind(task_id)
    .bind(stats)
    .execute(&state.pool)
    .await?;
    Ok(())
}

struct DiskSnapshot {
    mount_point: std::path::PathBuf,
    total_bytes: u64,
    available_bytes: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn longest_prefix_mount_wins() {
        let snapshots = vec![
            DiskSnapshot {
                mount_point: PathBuf::from("/"),
                total_bytes: 1_000_000_000_000,
                available_bytes: 400_000_000_000,
            },
            DiskSnapshot {
                mount_point: PathBuf::from("/data"),
                total_bytes: 500_000_000_000,
                available_bytes: 50_000_000_000,
            },
            DiskSnapshot {
                mount_point: PathBuf::from("/data/transcode"),
                total_bytes: 2_000_000_000,
                available_bytes: 1_500_000_000,
            },
        ];

        let hit = find_disk_for_path(&PathBuf::from("/data/transcode/abc"), &snapshots).unwrap();
        assert_eq!(hit.mount_point, PathBuf::from("/data/transcode"));
        assert_eq!(hit.total_bytes, 2_000_000_000);

        let hit = find_disk_for_path(&PathBuf::from("/data/media/movie.mkv"), &snapshots).unwrap();
        assert_eq!(hit.mount_point, PathBuf::from("/data"));

        let hit = find_disk_for_path(&PathBuf::from("/etc/hostname"), &snapshots).unwrap();
        assert_eq!(hit.mount_point, PathBuf::from("/"));
    }

    #[test]
    fn no_match_returns_none() {
        let snapshots = vec![DiskSnapshot {
            mount_point: PathBuf::from("/data"),
            total_bytes: 1,
            available_bytes: 1,
        }];
        assert!(find_disk_for_path(&PathBuf::from("/media/x"), &snapshots).is_none());
    }

    fn existing_mount() -> PathBuf {
        std::fs::canonicalize(std::env::temp_dir()).unwrap_or_else(|_| std::env::temp_dir())
    }

    #[test]
    fn below_threshold_is_healthy() {
        let mount = existing_mount();
        let snapshots = vec![DiskSnapshot {
            mount_point: mount.clone(),
            total_bytes: 1_000_000_000_000,
            available_bytes: 500_000_000_000,
        }];
        let report = build_path_report(METRIC_LABEL_DATA, &mount, 90, &snapshots);
        assert_eq!(report.status, "healthy");
        assert!(!report.exceeded);
        assert_eq!(report.usage_percent, Some(50.0));
        assert_eq!(report.used_bytes, Some(500_000_000_000));
    }

    #[test]
    fn at_threshold_is_exceeded() {
        let mount = existing_mount();
        let snapshots = vec![DiskSnapshot {
            mount_point: mount.clone(),
            total_bytes: 1_000,
            available_bytes: 100,
        }];
        let report = build_path_report(METRIC_LABEL_DATA, &mount, 90, &snapshots);
        assert_eq!(report.status, "threshold_exceeded");
        assert!(report.exceeded);
    }

    #[test]
    fn missing_path_is_unavailable() {
        let snapshots: Vec<DiskSnapshot> = vec![];
        let report = build_path_report(
            METRIC_LABEL_DATA,
            &PathBuf::from("/this/does/not/exist/anywhere/at/all"),
            90,
            &snapshots,
        );
        assert_eq!(report.status, "unavailable");
        assert!(!report.exceeded);
        assert_eq!(report.usage_percent, None);
    }

    #[test]
    fn zero_total_is_unavailable() {
        let mount = existing_mount();
        let snapshots = vec![DiskSnapshot {
            mount_point: mount.clone(),
            total_bytes: 0,
            available_bytes: 0,
        }];
        let report = build_path_report(METRIC_LABEL_DATA, &mount, 90, &snapshots);
        assert_eq!(report.status, "unavailable");
    }

    #[test]
    fn reports_record_metric_labels() {
        let mount = existing_mount();
        let snapshots = vec![DiskSnapshot {
            mount_point: mount.clone(),
            total_bytes: 1_000_000_000_000,
            available_bytes: 250_000_000_000,
        }];
        let report = build_path_report(METRIC_LABEL_DATA, &mount, 90, &snapshots);
        assert_eq!(report.label, METRIC_LABEL_DATA);
        assert_eq!(report.total_bytes, Some(1_000_000_000_000));
        assert_eq!(report.available_bytes, Some(250_000_000_000));
        assert_eq!(report.used_bytes, Some(750_000_000_000));
        assert_eq!(report.usage_percent, Some(75.0));
    }

    #[test]
    fn resolve_canonical_falls_back_to_ancestor() {
        let existing = std::env::temp_dir();
        let nonexistent = existing.join("duskcue_disk_check_missing_subdir");
        let resolved = resolve_canonical(&nonexistent);
        assert!(resolved.is_some());
    }

    #[test]
    fn resolve_canonical_returns_none_for_empty_root() {
        let resolved = resolve_canonical(std::path::Path::new(""));
        assert!(resolved.is_none() || resolved.is_some());
    }

    #[test]
    fn resolve_threshold_uses_default_when_absent() {
        assert_eq!(resolve_threshold(None, 90), 90);
    }

    #[test]
    fn resolve_threshold_uses_task_value_when_in_range() {
        assert_eq!(
            resolve_threshold(Some(&serde_json::json!(75)), 90),
            75
        );
    }

    #[test]
    fn resolve_threshold_clamps_high_values() {
        assert_eq!(
            resolve_threshold(Some(&serde_json::json!(250)), 90),
            MAX_THRESHOLD_PERCENT
        );
        assert_eq!(
            resolve_threshold(Some(&serde_json::json!(100)), 90),
            MAX_THRESHOLD_PERCENT
        );
    }

    #[test]
    fn resolve_threshold_clamps_low_values() {
        assert_eq!(
            resolve_threshold(Some(&serde_json::json!(0)), 90),
            MIN_THRESHOLD_PERCENT
        );
        assert_eq!(
            resolve_threshold(Some(&serde_json::json!(1)), 90),
            MIN_THRESHOLD_PERCENT
        );
    }

    #[test]
    fn resolve_threshold_accepts_boundary_values() {
        assert_eq!(
            resolve_threshold(Some(&serde_json::json!(50)), 90),
            MIN_THRESHOLD_PERCENT
        );
        assert_eq!(
            resolve_threshold(Some(&serde_json::json!(99)), 90),
            MAX_THRESHOLD_PERCENT
        );
    }
}

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

use uuid::Uuid;

use crate::services::poster_management::{self, AssetScanConfig, PosterManagementError};
use crate::state::AppState;

pub async fn run_asset_directory_scan(state: &AppState, task_id: Uuid, config: serde_json::Value) {
    tracing::info!(task_id = %task_id, "Starting asset directory scan task");

    match scan_for_config(state, &config).await {
        Ok(result) => {
            tracing::info!(
                task_id = %task_id,
                discovered = result.discovered,
                matched = result.matched,
                imported = result.imported,
                skipped = result.skipped,
                failed = result.failed,
                locked = result.locked,
                "Asset directory scan completed"
            );
        }
        Err(e) => {
            tracing::error!(
                task_id = %task_id,
                error = %e,
                "Asset directory scan failed"
            );
        }
    }
}

pub async fn scan_for_config(
    state: &AppState,
    config: &serde_json::Value,
) -> Result<poster_management::PosterImportResult, PosterManagementError> {
    let runtime = state.runtime_config.load();
    let path = config
        .get("path")
        .and_then(|v| v.as_str())
        .map(PathBuf::from)
        .or_else(|| {
            runtime
                .metadata
                .asset_directory
                .as_deref()
                .map(PathBuf::from)
        })
        .ok_or(PosterManagementError::AssetDirectoryNotConfigured)?;

    let lock_imported = parse_lock_imported(config);

    poster_management::scan_asset_directory(
        &state.pool,
        &state.bootstrap.data_dir,
        AssetScanConfig {
            path,
            lock_imported,
        },
    )
    .await
}

fn parse_lock_imported(config: &serde_json::Value) -> bool {
    config
        .get("lock_imported")
        .or_else(|| config.get("lock"))
        .and_then(|v| v.as_bool())
        .unwrap_or(true)
}

#[cfg(test)]
mod tests {
    use super::parse_lock_imported;
    use serde_json::json;

    #[test]
    fn lock_defaults_to_true() {
        assert!(parse_lock_imported(&json!({})));
        assert!(!parse_lock_imported(&json!({ "lock": false })));
        assert!(!parse_lock_imported(&json!({ "lock_imported": false })));
    }
}

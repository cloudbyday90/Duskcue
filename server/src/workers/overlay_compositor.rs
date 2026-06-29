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

use sqlx::{PgPool, Row};
use tokio::task::JoinSet;
use uuid::Uuid;

use crate::domains::overlays::{OverlayError, service as overlay_service};
use crate::state::AppState;

const DEFAULT_MAX_CONCURRENT: usize = 2;
const MAX_CONCURRENT_LIMIT: usize = 8;
const VALID_ARTWORK_TYPES: &[&str] = &["poster", "backdrop", "season_poster", "episode_thumb"];

pub async fn run_overlay_application(state: &AppState, task_id: Uuid, config: serde_json::Value) {
    tracing::info!(task_id = %task_id, "Starting overlay application task");

    match apply_overlays_for_config(state, &config).await {
        Ok(result) => {
            tracing::info!(
                task_id = %task_id,
                candidates = result.candidates,
                composited = result.composited,
                current = result.current,
                no_match = result.no_match,
                failed = result.failed,
                applied_overlays = result.applied_overlays,
                "Overlay application task completed"
            );
        }
        Err(e) => {
            tracing::error!(
                task_id = %task_id,
                error = %e,
                "Overlay application task failed"
            );
        }
    }
}

pub async fn apply_overlays_for_config(
    state: &AppState,
    config: &serde_json::Value,
) -> Result<OverlayApplicationResult, OverlayError> {
    let resolved = match ResolvedConfig::from_runtime_and_task(state, config) {
        ResolvedConfigResult::Skip(reason) => {
            tracing::info!(reason = %reason, "Overlay application skipped");
            return Ok(OverlayApplicationResult::default());
        }
        ResolvedConfigResult::Run(config) => config,
    };

    let targets = fetch_targets(&state.pool, &resolved).await?;
    if targets.is_empty() {
        tracing::info!("No artwork targets need overlay application");
        return Ok(OverlayApplicationResult::default());
    }

    apply_targets(
        state,
        targets,
        resolved.reapply_all,
        resolved.max_concurrent,
    )
    .await
}

pub async fn apply_overlays_now(
    state: &AppState,
    library_id: Option<Uuid>,
    reapply_all: bool,
    max_concurrent: Option<i32>,
) -> Result<OverlayApplicationResult, OverlayError> {
    let config = serde_json::json!({
        "library_id": library_id,
        "reapply_all": reapply_all,
        "max_concurrent": max_concurrent,
    });
    apply_overlays_for_config(state, &config).await
}

#[derive(Debug, Clone, Default)]
pub struct OverlayApplicationResult {
    pub candidates: u64,
    pub composited: u64,
    pub current: u64,
    pub no_match: u64,
    pub failed: u64,
    pub applied_overlays: u64,
}

impl OverlayApplicationResult {
    fn record_success(&mut self, result: overlay_service::CompositeResult) {
        if result.applied_count == 0 {
            self.no_match += 1;
        } else if result.composited {
            self.composited += 1;
        } else {
            self.current += 1;
        }
        self.applied_overlays += result.applied_count as u64;
    }
}

#[derive(Debug, Clone)]
struct OverlayTarget {
    media_item_id: Uuid,
    artwork_type: String,
}

#[derive(Debug, Clone)]
struct ResolvedConfig {
    library_id: Option<Uuid>,
    media_item_id: Option<Uuid>,
    artwork_types: Vec<String>,
    reapply_all: bool,
    max_concurrent: usize,
    batch_limit: Option<i64>,
}

enum ResolvedConfigResult {
    Skip(&'static str),
    Run(ResolvedConfig),
}

impl ResolvedConfig {
    fn from_runtime_and_task(state: &AppState, config: &serde_json::Value) -> ResolvedConfigResult {
        let runtime = state.runtime_config.load();
        if !runtime.metadata.overlays_enabled {
            return ResolvedConfigResult::Skip("metadata.overlays_enabled is false");
        }

        if !config
            .get("apply_overlays")
            .and_then(|v| v.as_bool())
            .unwrap_or(true)
        {
            return ResolvedConfigResult::Skip("task config apply_overlays is false");
        }

        let artwork_types = parse_artwork_types(config);
        let max_concurrent = config
            .get("max_concurrent")
            .and_then(|v| v.as_u64())
            .unwrap_or(DEFAULT_MAX_CONCURRENT as u64)
            .clamp(1, MAX_CONCURRENT_LIMIT as u64) as usize;

        let batch_limit = config
            .get("batch_limit")
            .or_else(|| config.get("limit"))
            .and_then(|v| v.as_i64())
            .filter(|v| *v > 0);

        ResolvedConfigResult::Run(ResolvedConfig {
            library_id: parse_uuid_field(config, "library_id"),
            media_item_id: parse_uuid_field(config, "media_item_id"),
            artwork_types,
            reapply_all: config
                .get("reapply_all")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
            max_concurrent,
            batch_limit,
        })
    }
}

async fn apply_targets(
    state: &AppState,
    targets: Vec<OverlayTarget>,
    reapply_all: bool,
    max_concurrent: usize,
) -> Result<OverlayApplicationResult, OverlayError> {
    let mut result = OverlayApplicationResult {
        candidates: targets.len() as u64,
        ..Default::default()
    };
    let mut tasks = JoinSet::new();

    for target in targets {
        spawn_target(&mut tasks, state.clone(), target, reapply_all);
        if tasks.len() >= max_concurrent {
            drain_one(&mut tasks, &mut result).await;
        }
    }

    while !tasks.is_empty() {
        drain_one(&mut tasks, &mut result).await;
    }

    Ok(result)
}

fn spawn_target(
    tasks: &mut JoinSet<(
        OverlayTarget,
        Result<overlay_service::CompositeResult, OverlayError>,
    )>,
    state: AppState,
    target: OverlayTarget,
    reapply_all: bool,
) {
    tasks.spawn(async move {
        let outcome = overlay_service::composite_and_persist(
            &state,
            target.media_item_id,
            &target.artwork_type,
            reapply_all,
        )
        .await;
        (target, outcome)
    });
}

async fn drain_one(
    tasks: &mut JoinSet<(
        OverlayTarget,
        Result<overlay_service::CompositeResult, OverlayError>,
    )>,
    aggregate: &mut OverlayApplicationResult,
) {
    let Some(joined) = tasks.join_next().await else {
        return;
    };

    match joined {
        Ok((target, Ok(result))) => {
            tracing::debug!(
                media_item_id = %target.media_item_id,
                artwork_type = %target.artwork_type,
                composited = result.composited,
                applied = result.applied_count,
                "Overlay target processed"
            );
            aggregate.record_success(result);
        }
        Ok((target, Err(e))) => {
            aggregate.failed += 1;
            tracing::warn!(
                media_item_id = %target.media_item_id,
                artwork_type = %target.artwork_type,
                error = %e,
                "Overlay target failed"
            );
        }
        Err(e) => {
            aggregate.failed += 1;
            tracing::warn!(error = %e, "Overlay target task panicked or was cancelled");
        }
    }
}

async fn fetch_targets(
    pool: &PgPool,
    config: &ResolvedConfig,
) -> Result<Vec<OverlayTarget>, sqlx::Error> {
    let rows = sqlx::query(
        r#"WITH candidates AS (
               SELECT DISTINCT
                      mi.id AS media_item_id,
                      CASE a.artwork_type
                          WHEN 'thumbnail' THEN 'episode_thumb'
                          ELSE a.artwork_type
                      END AS artwork_type,
                      aos.updated_at AS state_updated_at,
                      mi.created_at AS item_created_at
               FROM media_items mi
               JOIN libraries l ON l.id = mi.library_id
               JOIN artwork a ON a.media_item_id = mi.id
                             AND a."order" = 0
                             AND a.local_path IS NOT NULL
                             AND a.local_path <> ''
               LEFT JOIN artwork_overlay_state aos
                      ON aos.media_item_id = mi.id
                     AND aos.artwork_type = CASE a.artwork_type
                          WHEN 'thumbnail' THEN 'episode_thumb'
                          ELSE a.artwork_type
                      END
               WHERE l.deleted_at IS NULL
                 AND l.scan_enabled = true
                 AND ($1::uuid IS NULL OR mi.library_id = $1)
                 AND ($2::uuid IS NULL OR mi.id = $2)
                 AND CASE a.artwork_type
                          WHEN 'thumbnail' THEN 'episode_thumb'
                          ELSE a.artwork_type
                     END = ANY($3)
           )
           SELECT media_item_id, artwork_type
           FROM candidates
           ORDER BY state_updated_at ASC NULLS FIRST, item_created_at ASC
           LIMIT $4"#,
    )
    .bind(config.library_id)
    .bind(config.media_item_id)
    .bind(&config.artwork_types)
    .bind(config.batch_limit)
    .fetch_all(pool)
    .await?;

    rows.iter()
        .map(|row| {
            Ok(OverlayTarget {
                media_item_id: row.try_get("media_item_id")?,
                artwork_type: row.try_get("artwork_type")?,
            })
        })
        .collect()
}

fn parse_uuid_field(config: &serde_json::Value, field: &str) -> Option<Uuid> {
    config
        .get(field)
        .and_then(|v| v.as_str())
        .and_then(|s| Uuid::parse_str(s).ok())
}

fn parse_artwork_types(config: &serde_json::Value) -> Vec<String> {
    let parsed = config
        .get("artwork_types")
        .and_then(|v| v.as_array())
        .map(|values| {
            values
                .iter()
                .filter_map(|v| v.as_str())
                .filter(|v| VALID_ARTWORK_TYPES.contains(v))
                .map(String::from)
                .collect::<Vec<_>>()
        })
        .filter(|values| !values.is_empty());

    parsed.unwrap_or_else(|| {
        VALID_ARTWORK_TYPES
            .iter()
            .map(|s| (*s).to_string())
            .collect()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parse_uuid_field_ignores_invalid_values() {
        let id = Uuid::now_v7();
        let config = json!({
            "library_id": id.to_string(),
            "media_item_id": "invalid"
        });

        assert_eq!(parse_uuid_field(&config, "library_id"), Some(id));
        assert_eq!(parse_uuid_field(&config, "media_item_id"), None);
        assert_eq!(parse_uuid_field(&config, "missing"), None);
    }

    #[test]
    fn parse_artwork_types_defaults_to_all_supported_types() {
        let types = parse_artwork_types(&json!({}));
        assert_eq!(
            types,
            vec!["poster", "backdrop", "season_poster", "episode_thumb"]
        );
    }

    #[test]
    fn parse_artwork_types_filters_unknown_values() {
        let types = parse_artwork_types(&json!({
            "artwork_types": ["poster", "logo", "episode_thumb"]
        }));
        assert_eq!(types, vec!["poster", "episode_thumb"]);
    }

    #[test]
    fn application_result_classifies_successes() {
        let mut result = OverlayApplicationResult::default();
        result.record_success(overlay_service::CompositeResult {
            media_item_id: Uuid::nil(),
            artwork_type: "poster".into(),
            composited: true,
            applied_count: 2,
        });
        result.record_success(overlay_service::CompositeResult {
            media_item_id: Uuid::nil(),
            artwork_type: "poster".into(),
            composited: false,
            applied_count: 2,
        });
        result.record_success(overlay_service::CompositeResult {
            media_item_id: Uuid::nil(),
            artwork_type: "poster".into(),
            composited: false,
            applied_count: 0,
        });

        assert_eq!(result.composited, 1);
        assert_eq!(result.current, 1);
        assert_eq!(result.no_match, 1);
        assert_eq!(result.applied_overlays, 4);
    }
}

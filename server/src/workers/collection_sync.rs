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

use std::time::Duration;

use chrono::Utc;
use serde_json::json;
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::services::collections::{
    BuilderConfig, CollectionBuilderError, CollectionSyncResult, sync_dynamic_collection,
};
use crate::services::tmdb_client::TmdbClient;
use crate::state::AppState;

pub async fn run_collection_sync(state: &AppState, task_id: Uuid, config: serde_json::Value) {
    tracing::info!(task_id = %task_id, "Starting collection sync task");

    let resolved = match ResolvedConfig::from_runtime_and_task(state, &config) {
        ResolvedConfigResult::Skip(reason) => {
            tracing::info!(task_id = %task_id, reason = %reason, "Collection sync skipped");
            return;
        }
        ResolvedConfigResult::Run(config) => config,
    };

    let targets =
        match fetch_targets(&state.pool, resolved.library_id, resolved.collection_id).await {
            Ok(targets) => targets,
            Err(e) => {
                tracing::error!(
                    task_id = %task_id,
                    error = %e,
                    "Failed to fetch dynamic collections for sync"
                );
                return;
            }
        };

    if targets.is_empty() {
        tracing::info!(task_id = %task_id, "No enabled dynamic collections to sync");
        return;
    }

    let tmdb_client = resolved.tmdb_client.as_ref();
    let mut total = SyncAggregate::default();
    let mut previous_external = false;

    for target in targets {
        total.collections_seen += 1;

        let builder = match BuilderConfig::from_value(&target.dynamic_config) {
            Ok(builder) => builder,
            Err(e) => {
                total.failed += 1;
                record_collection_sync_failure(&state.pool, target.id, &e.to_string()).await;
                tracing::warn!(
                    task_id = %task_id,
                    collection_id = %target.id,
                    error = %e,
                    "Skipping collection with invalid dynamic config"
                );
                continue;
            }
        };

        let external = is_external_builder(&builder.builder_type);
        if external && !resolved.sync_external {
            total.skipped_external += 1;
            tracing::debug!(
                task_id = %task_id,
                collection_id = %target.id,
                builder_type = %builder.builder_type,
                "Skipping external collection builder because sync_external is false"
            );
            continue;
        }

        if external && previous_external {
            tokio::time::sleep(resolved.external_delay).await;
        }

        match sync_dynamic_collection(
            &state.pool,
            tmdb_client,
            target.id,
            resolved.sync_external,
            resolved.reprocess_all,
        )
        .await
        {
            Ok(result) => {
                tracing::info!(
                    task_id = %task_id,
                    collection_id = %result.collection_id,
                    builder_type = %result.builder_type,
                    added = result.added,
                    removed = result.removed,
                    total_matched = result.total_matched,
                    missing = result.missing,
                    "Dynamic collection synced"
                );
                total.add_success(&result);
                if external {
                    total.external_synced += 1;
                    previous_external = true;
                }
            }
            Err(e) => {
                total.failed += 1;
                record_collection_sync_failure(&state.pool, target.id, &e.to_string()).await;
                tracing::warn!(
                    task_id = %task_id,
                    collection_id = %target.id,
                    builder_type = %builder.builder_type,
                    error = %e,
                    "Dynamic collection sync failed"
                );

                if matches!(e, CollectionBuilderError::ExternalRateLimited) {
                    total.rate_limited_abort = true;
                    tracing::warn!(
                        task_id = %task_id,
                        collection_id = %target.id,
                        "External collection source rate limited; aborting remaining collection sync until next run"
                    );
                    break;
                }

                if external {
                    previous_external = true;
                }
            }
        }
    }

    tracing::info!(
        task_id = %task_id,
        collections = total.collections_seen,
        synced = total.synced,
        skipped_external = total.skipped_external,
        failed = total.failed,
        external_synced = total.external_synced,
        rate_limited_abort = total.rate_limited_abort,
        added = total.added,
        removed = total.removed,
        total_matched = total.total_matched,
        missing = total.missing,
        "Collection sync task completed"
    );
}

#[derive(Debug, Clone)]
struct CollectionSyncTarget {
    id: Uuid,
    dynamic_config: serde_json::Value,
}

#[derive(Clone)]
struct ResolvedConfig {
    library_id: Option<Uuid>,
    collection_id: Option<Uuid>,
    sync_external: bool,
    reprocess_all: bool,
    external_delay: Duration,
    tmdb_client: Option<TmdbClient>,
}

enum ResolvedConfigResult {
    Skip(&'static str),
    Run(ResolvedConfig),
}

impl ResolvedConfig {
    fn from_runtime_and_task(state: &AppState, config: &serde_json::Value) -> ResolvedConfigResult {
        let runtime = state.runtime_config.load();
        let metadata = runtime.metadata.clone();

        if !metadata.collections_enabled {
            return ResolvedConfigResult::Skip("metadata.collections_enabled is false");
        }

        if !config
            .get("sync_dynamic")
            .and_then(|v| v.as_bool())
            .unwrap_or(true)
        {
            return ResolvedConfigResult::Skip("task config sync_dynamic is false");
        }

        let sync_external = config
            .get("sync_external")
            .or_else(|| config.get("include_external"))
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        let max_external_per_minute = config
            .get("max_external_requests_per_minute")
            .and_then(|v| v.as_u64())
            .unwrap_or(metadata.collection_external_rate_limit_per_minute.max(1) as u64)
            .clamp(1, 120);

        let tmdb_client = if metadata.providers.tmdb.enabled
            && !metadata.providers.tmdb.access_token.is_empty()
        {
            Some(TmdbClient::new(
                &metadata.providers.tmdb,
                metadata.metadata_language.clone(),
            ))
        } else {
            None
        };

        ResolvedConfigResult::Run(ResolvedConfig {
            library_id: parse_uuid_field(config, "library_id"),
            collection_id: parse_uuid_field(config, "collection_id"),
            sync_external,
            reprocess_all: config
                .get("reprocess_all")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
            external_delay: external_request_delay(max_external_per_minute),
            tmdb_client,
        })
    }
}

async fn fetch_targets(
    pool: &PgPool,
    library_id: Option<Uuid>,
    collection_id: Option<Uuid>,
) -> Result<Vec<CollectionSyncTarget>, sqlx::Error> {
    let rows = sqlx::query(
        r#"SELECT id, dynamic_config
           FROM collections
           WHERE is_dynamic = true
             AND is_enabled = true
             AND ($1::uuid IS NULL OR library_id = $1)
             AND ($2::uuid IS NULL OR id = $2)
           ORDER BY last_synced_at ASC NULLS FIRST, sort_order ASC, name ASC"#,
    )
    .bind(library_id)
    .bind(collection_id)
    .fetch_all(pool)
    .await?;

    rows.iter()
        .map(|row| {
            Ok(CollectionSyncTarget {
                id: row.try_get("id")?,
                dynamic_config: row.try_get("dynamic_config")?,
            })
        })
        .collect()
}

async fn record_collection_sync_failure(pool: &PgPool, collection_id: Uuid, error: &str) {
    let sync_result = json!({
        "status": "failed",
        "error": error,
        "attempted_at": Utc::now(),
    });

    if let Err(e) = sqlx::query(
        r#"UPDATE collections
           SET last_sync_result = $2,
               updated_at = now()
           WHERE id = $1"#,
    )
    .bind(collection_id)
    .bind(sync_result)
    .execute(pool)
    .await
    {
        tracing::warn!(
            collection_id = %collection_id,
            error = %e,
            "Failed to persist collection sync failure result"
        );
    }
}

fn parse_uuid_field(config: &serde_json::Value, field: &str) -> Option<Uuid> {
    config
        .get(field)
        .and_then(|v| v.as_str())
        .and_then(|s| Uuid::parse_str(s).ok())
}

fn is_external_builder(builder_type: &str) -> bool {
    matches!(
        builder_type,
        "tmdb_popular"
            | "tmdb_top_rated"
            | "tmdb_trending"
            | "tmdb_now_playing"
            | "tmdb_upcoming"
            | "tmdb_collection"
            | "trakt_trending"
            | "trakt_popular"
            | "trakt_recommended"
            | "trakt_user_lists"
            | "imdb_top_250"
            | "custom_url"
    )
}

fn external_request_delay(max_per_minute: u64) -> Duration {
    Duration::from_secs((60 / max_per_minute.max(1)).max(1))
}

#[derive(Debug, Default)]
struct SyncAggregate {
    collections_seen: u64,
    synced: u64,
    skipped_external: u64,
    failed: u64,
    external_synced: u64,
    rate_limited_abort: bool,
    added: u64,
    removed: u64,
    total_matched: u64,
    missing: u64,
}

impl SyncAggregate {
    fn add_success(&mut self, result: &CollectionSyncResult) {
        self.synced += 1;
        self.added += result.added as u64;
        self.removed += result.removed as u64;
        self.total_matched += result.total_matched as u64;
        self.missing += result.missing as u64;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn external_builder_classification_covers_known_external_sources() {
        assert!(is_external_builder("tmdb_popular"));
        assert!(is_external_builder("tmdb_top_rated"));
        assert!(is_external_builder("tmdb_trending"));
        assert!(is_external_builder("tmdb_now_playing"));
        assert!(is_external_builder("tmdb_upcoming"));
        assert!(is_external_builder("trakt_popular"));
        assert!(is_external_builder("custom_url"));
        assert!(!is_external_builder("genre"));
        assert!(!is_external_builder("audio_codec"));
    }

    #[test]
    fn external_request_delay_is_never_zero() {
        assert_eq!(external_request_delay(30), Duration::from_secs(2));
        assert_eq!(external_request_delay(120), Duration::from_secs(1));
        assert_eq!(external_request_delay(0), Duration::from_secs(60));
    }

    #[test]
    fn parse_uuid_field_ignores_missing_or_invalid_values() {
        let id = Uuid::now_v7();
        let value = json!({
            "library_id": id.to_string(),
            "collection_id": "not-a-uuid"
        });

        assert_eq!(parse_uuid_field(&value, "library_id"), Some(id));
        assert_eq!(parse_uuid_field(&value, "collection_id"), None);
        assert_eq!(parse_uuid_field(&value, "other"), None);
    }

    #[test]
    fn aggregate_add_success_accumulates_sync_totals() {
        let mut agg = SyncAggregate::default();
        let first = CollectionSyncResult {
            collection_id: Uuid::nil(),
            builder_type: "genre".into(),
            added: 2,
            removed: 1,
            total_matched: 10,
            missing: 0,
        };
        let second = CollectionSyncResult {
            collection_id: Uuid::nil(),
            builder_type: "tmdb_popular".into(),
            added: 5,
            removed: 3,
            total_matched: 20,
            missing: 4,
        };

        agg.add_success(&first);
        agg.add_success(&second);

        assert_eq!(agg.synced, 2);
        assert_eq!(agg.added, 7);
        assert_eq!(agg.removed, 4);
        assert_eq!(agg.total_matched, 30);
        assert_eq!(agg.missing, 4);
    }
}

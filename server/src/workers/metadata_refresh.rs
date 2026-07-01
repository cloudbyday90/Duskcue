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

use std::io::Read;

use chrono::Utc;
use flate2::read::GzDecoder;
use sqlx::{PgPool, Row};
use tokio::fs;
use uuid::Uuid;

use crate::domains::tv::service as tv_service;
use crate::domains::tv::types::TvSurfaceSectionType;
use crate::services::event_bus::EventBus;
use crate::services::metadata::EnrichmentOrchestrator;

const EXPORTS_SUBDIR: &str = "metadata/exports";
const MAX_EXPORT_AGE_DAYS: u64 = 7;
const TMDB_FILES_BASE: &str = "https://files.tmdb.org/p/exports";

pub async fn run_metadata_refresh(
    pool: &PgPool,
    cache_dir: &std::path::Path,
    orchestrator: &EnrichmentOrchestrator,
    event_bus: &EventBus,
    task_id: Uuid,
    config: serde_json::Value,
) {
    tracing::info!(task_id = %task_id, "Starting metadata refresh task");

    if let Err(e) = download_daily_exports(cache_dir).await {
        tracing::warn!(error = %e, "Daily export download failed — continuing with /changes refresh");
    }

    match refresh_changed_items(pool, orchestrator, &config).await {
        Ok(enriched) => {
            if enriched > 0 {
                if let Err(e) = tv_service::publish_tv_surface_changed_for_all_users(
                    pool,
                    event_bus,
                    "metadata_changed",
                    all_tv_sections(),
                    None,
                    None,
                )
                .await
                {
                    tracing::warn!(error = %e, "Failed to publish TV metadata change event");
                }
            }
        }
        Err(e) => {
            tracing::error!(error = %e, "Metadata refresh failed");
        }
    }

    tracing::info!(task_id = %task_id, "Metadata refresh task completed");
}

async fn download_daily_exports(
    cache_dir: &std::path::Path,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let exports_dir = cache_dir.join(EXPORTS_SUBDIR);
    fs::create_dir_all(&exports_dir).await?;

    cleanup_old_exports(&exports_dir).await?;

    let today = Utc::now();
    let date_str = today.format("%m_%d_%Y").to_string();

    let files = [
        format!("movie_ids_{date_str}.json.gz"),
        format!("tv_series_ids_{date_str}.json.gz"),
    ];

    let http = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(300))
        .build()?;

    for filename in &files {
        let dest = exports_dir.join(filename);
        if dest.exists() {
            tracing::debug!(file = %filename, "Export file already exists, skipping download");
            continue;
        }

        let url = format!("{TMDB_FILES_BASE}/{filename}");
        tracing::info!(url = %url, "Downloading TMDB daily export");

        let response = http.get(&url).send().await?;
        if !response.status().is_success() {
            tracing::warn!(
                file = %filename,
                status = %response.status(),
                "Failed to download export file"
            );
            continue;
        }

        let bytes = response.bytes().await?;
        fs::write(&dest, &bytes).await?;

        tracing::info!(
            file = %filename,
            size = bytes.len(),
            "Downloaded TMDB daily export"
        );

        let count = count_export_entries(&dest).unwrap_or(0);
        tracing::info!(file = %filename, entries = count, "Parsed export entries");
    }

    Ok(())
}

async fn cleanup_old_exports(
    exports_dir: &std::path::Path,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut entries = fs::read_dir(exports_dir).await?;
    let cutoff = Utc::now() - chrono::Duration::days(MAX_EXPORT_AGE_DAYS as i64);

    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("gz") {
            continue;
        }

        let metadata = fs::metadata(&path).await?;
        let modified: chrono::DateTime<Utc> = metadata.modified()?.into();
        if modified < cutoff {
            tracing::info!(file = ?path, "Removing old export file");
            fs::remove_file(path).await?;
        }
    }

    Ok(())
}

fn count_export_entries(
    path: &std::path::Path,
) -> Result<usize, Box<dyn std::error::Error + Send + Sync>> {
    let bytes = std::fs::read(path)?;
    let mut decoder = GzDecoder::new(&bytes[..]);
    let mut content = String::new();
    decoder.read_to_string(&mut content)?;

    Ok(content.lines().filter(|l| !l.trim().is_empty()).count())
}

async fn refresh_changed_items(
    pool: &PgPool,
    orchestrator: &EnrichmentOrchestrator,
    config: &serde_json::Value,
) -> Result<u64, Box<dyn std::error::Error + Send + Sync>> {
    let tmdb_client = match orchestrator.tmdb_client() {
        Some(c) => c,
        None => {
            tracing::info!("No TMDB client configured — skipping metadata refresh");
            return Ok(0);
        }
    };

    let last_refresh: Option<String> = config
        .get("last_metadata_refresh_at")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let now = Utc::now();
    let end_date = now.format("%Y-%m-%d").to_string();
    let start_date = last_refresh.unwrap_or_else(|| {
        let default = now - chrono::Duration::hours(6);
        default.format("%Y-%m-%d").to_string()
    });

    tracing::info!(
        start_date = %start_date,
        end_date = %end_date,
        "Querying TMDB /changes for modified items"
    );

    let changed_movies = tmdb_client
        .fetch_changed_movie_ids(&start_date, &end_date)
        .await
        .unwrap_or_else(|e| {
            tracing::warn!(error = %e, "Failed to fetch changed movie IDs");
            Vec::new()
        });

    let changed_tv = tmdb_client
        .fetch_changed_tv_ids(&start_date, &end_date)
        .await
        .unwrap_or_else(|e| {
            tracing::warn!(error = %e, "Failed to fetch changed TV IDs");
            Vec::new()
        });

    tracing::info!(
        movies = changed_movies.len(),
        tv = changed_tv.len(),
        "Found changed items from TMDB /changes"
    );

    let mut enriched = 0u64;
    let mut failed = 0u64;

    if !changed_movies.is_empty() {
        let matching_ids = find_matching_items(pool, &changed_movies, "movie").await?;
        tracing::info!(
            changed = changed_movies.len(),
            matching = matching_ids.len(),
            "Processing changed movies"
        );

        for (media_item_id, tmdb_id) in &matching_ids {
            match crate::services::enrichment_persistence::re_enrich_item(
                pool,
                orchestrator,
                *media_item_id,
                "movie",
                *tmdb_id,
            )
            .await
            {
                Ok(()) => enriched += 1,
                Err(e) => {
                    tracing::warn!(
                        media_item_id = %media_item_id,
                        tmdb_id = tmdb_id,
                        error = %e,
                        "Failed to re-enrich movie"
                    );
                    failed += 1;
                }
            }
        }
    }

    if !changed_tv.is_empty() {
        let matching_ids = find_matching_items(pool, &changed_tv, "series").await?;
        tracing::info!(
            changed = changed_tv.len(),
            matching = matching_ids.len(),
            "Processing changed TV series"
        );

        for (media_item_id, tmdb_id) in &matching_ids {
            match crate::services::enrichment_persistence::re_enrich_item(
                pool,
                orchestrator,
                *media_item_id,
                "series",
                *tmdb_id,
            )
            .await
            {
                Ok(()) => enriched += 1,
                Err(e) => {
                    tracing::warn!(
                        media_item_id = %media_item_id,
                        tmdb_id = tmdb_id,
                        error = %e,
                        "Failed to re-enrich TV series"
                    );
                    failed += 1;
                }
            }
        }
    }

    tracing::info!(enriched, failed, "Metadata refresh re-enrichment complete");

    Ok(enriched)
}

fn all_tv_sections() -> Vec<TvSurfaceSectionType> {
    vec![
        TvSurfaceSectionType::Continue,
        TvSurfaceSectionType::NextUp,
        TvSurfaceSectionType::NewEpisodes,
        TvSurfaceSectionType::Recommended,
    ]
}

async fn find_matching_items(
    pool: &PgPool,
    changed_tmdb_ids: &[u64],
    item_type: &str,
) -> Result<Vec<(Uuid, u64)>, sqlx::Error> {
    if changed_tmdb_ids.is_empty() {
        return Ok(Vec::new());
    }

    let ext_table = if item_type == "movie" {
        "movies"
    } else {
        "series"
    };

    let mut builder = sqlx::QueryBuilder::new(
        "SELECT mi.id, (e.metadata->>'tmdb_id')::bigint AS tmdb_id FROM media_items mi JOIN ",
    );
    builder.push(ext_table);
    builder.push(" e ON e.media_item_id = mi.id WHERE mi.deleted_at IS NULL AND mi.match_state = 'confirmed' AND e.metadata->>'tmdb_id' IS NOT NULL AND (e.metadata->>'tmdb_id')::bigint IN (");

    let mut separated = builder.separated(", ");
    for id in changed_tmdb_ids {
        separated.push(*id as i64);
    }
    separated.push_unseparated(")");

    let rows = builder.build().fetch_all(pool).await?;

    Ok(rows
        .iter()
        .map(|r| {
            let id: Uuid = r.get("id");
            let tmdb: i64 = r.get("tmdb_id");
            (id, tmdb as u64)
        })
        .collect())
}

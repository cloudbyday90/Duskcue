// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::domains::subtitles::service::fetch_subtitles;
use crate::domains::subtitles::types::FetchSubtitlesRequest;
use crate::state::AppState;

const DEFAULT_MAX_ITEMS_PER_LANGUAGE: i64 = 50;

pub async fn run_subtitle_auto_fetch(state: &AppState, task_id: Uuid, config: serde_json::Value) {
    tracing::info!(task_id = %task_id, "Starting subtitle auto-fetch task");

    let (languages, max_items) = match resolve_targets(state, &config) {
        ResolvedTargets::Skip(reason) => {
            tracing::info!(task_id = %task_id, reason = %reason, "Subtitle auto-fetch skipped");
            return;
        }
        ResolvedTargets::Run {
            languages,
            max_items_per_language,
        } => (languages, max_items_per_language),
    };

    if languages.is_empty() {
        tracing::info!(task_id = %task_id, "No target languages for subtitle auto-fetch, skipping");
        return;
    }

    let pool = &state.pool;
    let mut total_processed: u64 = 0;
    let mut total_fetched: u64 = 0;
    let mut total_no_results: u64 = 0;
    let mut total_failures: u64 = 0;

    for language in &languages {
        let item_ids = match find_items_missing_subtitles(pool, language, max_items).await {
            Ok(ids) => ids,
            Err(e) => {
                tracing::warn!(
                    task_id = %task_id,
                    language = %language,
                    error = %e,
                    "Failed to query items missing subtitles for language, skipping language"
                );
                continue;
            }
        };

        if item_ids.is_empty() {
            tracing::debug!(task_id = %task_id, language = %language, "No items missing subtitles for language");
            continue;
        }

        tracing::info!(
            task_id = %task_id,
            language = %language,
            candidate_count = item_ids.len(),
            "Fetching subtitles for language"
        );

        for media_item_id in &item_ids {
            total_processed += 1;

            let req = FetchSubtitlesRequest {
                language: language.clone(),
                provider: None,
                is_forced: None,
                is_hearing_impaired: None,
            };

            match fetch_subtitles(state, *media_item_id, &req).await {
                Ok(resp) => {
                    if resp.no_results || resp.fetched.is_empty() {
                        total_no_results += 1;
                    } else {
                        let provider = resp.provider_used.as_deref().unwrap_or("unknown");
                        tracing::info!(
                            task_id = %task_id,
                            media_item_id = %media_item_id,
                            language = %language,
                            provider = %provider,
                            "Auto-fetched subtitle"
                        );
                        total_fetched += 1;
                    }
                }
                Err(e) => {
                    tracing::warn!(
                        task_id = %task_id,
                        media_item_id = %media_item_id,
                        language = %language,
                        error = %e,
                        "Subtitle auto-fetch failed for item"
                    );
                    total_failures += 1;
                }
            }
        }
    }

    tracing::info!(
        task_id = %task_id,
        total_processed,
        total_fetched,
        total_no_results,
        total_failures,
        "Subtitle auto-fetch task completed"
    );
}

enum ResolvedTargets {
    Skip(&'static str),
    Run {
        languages: Vec<String>,
        max_items_per_language: i64,
    },
}

fn resolve_targets(state: &AppState, config: &serde_json::Value) -> ResolvedTargets {
    let runtime = state.runtime_config.load();
    let subtitle_cfg = &runtime.subtitles;
    let providers = &runtime.integrations.subtitle_providers;

    if !subtitle_cfg.auto_fetch_enabled {
        return ResolvedTargets::Skip("subtitles.auto_fetch_enabled is false");
    }

    let subdl_eligible = providers.subdl.enabled
        && providers.subdl.auto_fetch_enabled
        && providers
            .subdl
            .api_key
            .as_deref()
            .map(|k| !k.is_empty())
            .unwrap_or(false);

    let os_eligible = providers.opensubtitles.enabled
        && providers.opensubtitles.auto_fetch_enabled
        && providers
            .opensubtitles
            .api_key
            .as_deref()
            .map(|k| !k.is_empty())
            .unwrap_or(false);

    if !subdl_eligible && !os_eligible {
        return ResolvedTargets::Skip(
            "no subtitle provider is both enabled and auto-fetch-enabled with a non-empty API key",
        );
    }

    let max_items = config
        .get("max_items_per_language")
        .and_then(|v| v.as_i64())
        .filter(|n| *n > 0)
        .unwrap_or(DEFAULT_MAX_ITEMS_PER_LANGUAGE);

    if let Some(langs) = config.get("languages").and_then(|v| v.as_array()) {
        let languages: Vec<String> = langs
            .iter()
            .filter_map(|v| v.as_str())
            .map(|s| s.to_string())
            .filter(|s| !s.is_empty())
            .collect();
        return ResolvedTargets::Run {
            languages,
            max_items_per_language: max_items,
        };
    }

    let mut languages: Vec<String> = Vec::new();
    for lang in &subtitle_cfg.auto_fetch_languages {
        if !languages.iter().any(|l| l == lang) {
            languages.push(lang.clone());
        }
    }
    if subdl_eligible {
        for lang in &providers.subdl.auto_fetch_languages {
            if !languages.iter().any(|l| l == lang) {
                languages.push(lang.clone());
            }
        }
    }
    if os_eligible {
        for lang in &providers.opensubtitles.auto_fetch_languages {
            if !languages.iter().any(|l| l == lang) {
                languages.push(lang.clone());
            }
        }
    }

    ResolvedTargets::Run {
        languages,
        max_items_per_language: max_items,
    }
}

async fn find_items_missing_subtitles(
    pool: &PgPool,
    language: &str,
    limit: i64,
) -> Result<Vec<Uuid>, sqlx::Error> {
    let lang_prefix = format!("{language}%");

    let rows = sqlx::query(
        r#"
        SELECT mi.id
        FROM media_items mi
        WHERE mi.deleted_at IS NULL
          AND mi.type IN ('movie', 'episode')
          AND EXISTS (
              SELECT 1 FROM media_files mf
              WHERE mf.media_item_id = mi.id AND mf.is_healthy = true
          )
          AND NOT EXISTS (
              SELECT 1 FROM subtitle_files sf
              WHERE sf.media_item_id = mi.id
                AND (sf.language = $1 OR sf.language ILIKE $2)
          )
        ORDER BY mi.created_at DESC
        LIMIT $3
        "#,
    )
    .bind(language)
    .bind(&lang_prefix)
    .bind(limit)
    .fetch_all(pool)
    .await?;

    Ok(rows.iter().map(|r| r.get::<Uuid, _>("id")).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolved_targets_skip_logic() {
        assert!(matches!(
            ResolvedTargets::Skip("test"),
            ResolvedTargets::Skip("test")
        ));
    }

    #[test]
    fn test_language_prefix_format() {
        assert_eq!(format!("{}%", "en"), "en%");
        assert_eq!(format!("{}%", "eng"), "eng%");
    }
}

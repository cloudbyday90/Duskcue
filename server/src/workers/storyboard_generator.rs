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

//! Background storyboard generator — the orchestration point for the
//! thumbnail-sprite pipeline defined in
//! [`crate::services::storyboards`](../../services/storyboards/index.html).
//!
//! Implements the pipeline from
//! [STORYBOARDS.md](../../docs/design/STORYBOARDS.md):
//!
//! 1. Resolve candidates (incremental — only files needing storyboards)
//! 2. For each file: resolve effective config (server-wide + per-library overrides)
//! 3. Call [`sb_svc::generate_storyboard`] to extract frames and assemble sprites
//! 4. Persist the `storyboards` DB row (upsert on `media_file_id`)
//!
//! ## Two entry shapes
//!
//! - **Scheduled iteration** ([`run_storyboard_generation`]) — iterates all
//!   non-deleted, scan-enabled libraries. Called by the scheduler daily at
//!   04:00 (after segment analysis at 03:00).
//! - **Synchronous per-library / per-item triggers**
//!   ([`generate_for_library_one`], [`generate_for_item_one`]) — service the
//!   admin `POST` endpoints. These run inline and return a summary, matching
//!   the segment domain's `trigger_library_analysis` pattern.
//!
//! ## Per-library enablement
//!
//! Three gates must pass before a library is processed:
//! 1. Global `TranscodingConfig.storyboards_enabled` must be `true`.
//! 2. The library must be non-deleted with `scan_enabled = true`.
//! 3. Per-library `libraries.metadata->>'storyboards_enabled'` must NOT be
//!    `"false"` (defaults to enabled when the key is absent).
//!
//! The third gate avoids the Jellyfin bug #14558 complaint pattern where
//! users reported CPU usage from a scheduled task that should have been
//! disabled per-library.
//!
//! ## Incremental generation
//!
//! Files with an existing `storyboards` row whose nullable `file_hash` and
//! normalized generation fingerprint match are skipped. This makes subsequent
//! runs fast while still regenerating when output-affecting settings change.

use std::path::Path;

use sqlx::{PgPool, Postgres, Row, Transaction};
use uuid::Uuid;

use crate::domains::storyboards::StoryboardError;
use crate::services::event_bus::ServerEvent;
use crate::services::storyboards as sb_svc;
use crate::services::storyboards::GenerationConfig;
use crate::state::AppState;

/// Storyboards live under `{data_dir}/cache/storyboards/` per the design's
/// "Storage Path" spec. The subdirectory name matches the constant in the
/// storyboards domain service.
const STORYBOARDS_SUBDIR: &str = "storyboards";

// ---------------------------------------------------------------------------
// Public entry points
// ---------------------------------------------------------------------------

/// Scheduled-task entry point — iterates all non-deleted, scan-enabled
/// libraries and generates storyboards for any files needing them.
///
/// `config` may contain:
/// - `library_id` (string UUID) — restrict to a single library
/// - `interval_mode` (`"adaptive"` or `"fixed"`) — override server-wide mode
///
/// Honors the global `storyboards_enabled` gate and per-library
/// `metadata.storyboards_enabled` override. Libraries that fail any gate are
/// silently skipped (logged at debug).
pub async fn run_storyboard_generation(
    state: &AppState,
    task_id: Uuid,
    config: serde_json::Value,
) -> Result<(), String> {
    tracing::info!(task_id = %task_id, "Starting storyboard generation task");

    let cache_dir = state.bootstrap.cache_dir.clone();
    let task_config = parse_storyboard_task_config(config)?;

    let pool = &state.pool;

    let library_ids: Vec<Uuid> = if let Some(library_id) = task_config.library_id {
        if fetch_library_metadata(pool, library_id)
            .await
            .map_err(|error| error.to_string())?
            .is_none()
        {
            return Err(format!(
                "configured storyboard library {library_id} does not exist, is disabled, or is deleted"
            ));
        }
        vec![library_id]
    } else {
        match fetch_enabled_libraries(pool).await {
            Ok(ids) => ids,
            Err(e) => {
                tracing::error!(
                    task_id = %task_id,
                    error = %e,
                    "Failed to fetch libraries for storyboard generation"
                );
                return Err(e.to_string());
            }
        }
    };

    if library_ids.is_empty() {
        tracing::info!(task_id = %task_id, "No libraries to generate storyboards for");
        reconcile_storyboard_cache(pool, &cache_dir).await;
        return Ok(());
    }

    let mut total = AggregateResult::default();
    let mut library_failures = Vec::new();
    for library_id in &library_ids {
        match generate_for_library(
            state,
            *library_id,
            &cache_dir,
            task_config.interval_mode.as_deref(),
            None,
        )
        .await
        {
            Ok(result) => {
                tracing::info!(
                    task_id = %task_id,
                    library_id = %library_id,
                    candidates = result.candidates,
                    generated = result.generated,
                    skipped = result.skipped,
                    errors = result.errors,
                    "Library storyboard generation complete"
                );
                if result.errors > 0 {
                    library_failures.push(format!(
                        "{library_id}: {} file generation failure(s)",
                        result.errors
                    ));
                }
                total.add(&result);
            }
            Err(e) => {
                tracing::warn!(
                    task_id = %task_id,
                    library_id = %library_id,
                    error = %e,
                    "Library storyboard generation failed"
                );
                library_failures.push(format!("{library_id}: {e}"));
            }
        }
    }

    tracing::info!(
        task_id = %task_id,
        libraries = library_ids.len(),
        candidates = total.candidates,
        generated = total.generated,
        skipped = total.skipped,
        errors = total.errors,
        "Storyboard generation task completed"
    );
    reconcile_storyboard_cache(pool, &cache_dir).await;
    if library_failures.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "storyboard generation failed for {} library or libraries: {}",
            library_failures.len(),
            library_failures.join("; ")
        ))
    }
}

/// Synchronous per-library generation entry point — services the admin
/// `POST /api/v1/libraries/{id}/generate-storyboards` endpoint.
///
/// Runs the pipeline inline and returns a summary. `interval_mode_override`
/// is `None` (use server-wide config). Per-library enablement is respected
/// (returns an empty result with `skipped = 0` when disabled, not an error).
///
/// `requesting_user_id` — when `Some`, emits `storyboard_progress` events
/// to the user's SSE channel so the admin UI can show live progress. The
/// scheduled task passes `None` (no progress events for background runs).
pub async fn generate_for_library_one(
    state: &AppState,
    library_id: Uuid,
    requesting_user_id: Option<Uuid>,
) -> Result<LibraryGenerationResult, sqlx::Error> {
    let cache_dir = state.bootstrap.cache_dir.clone();
    let result =
        generate_for_library(state, library_id, &cache_dir, None, requesting_user_id).await?;
    reconcile_storyboard_cache(&state.pool, &cache_dir).await;
    Ok(result)
}

/// Synchronous per-item generation entry point — services the admin
/// `POST /api/v1/items/{id}/generate-storyboards` endpoint.
///
/// Forces regeneration while preserving the current artifacts until the
/// replacement generation has completed and its database pointer commits.
///
/// `requesting_user_id` — when `Some`, emits `storyboard_progress` events
/// to the user's SSE channel.
pub async fn generate_for_item_one(
    state: &AppState,
    media_item_id: Uuid,
    requesting_user_id: Option<Uuid>,
) -> Result<LibraryGenerationResult, StoryboardError> {
    let pool = &state.pool;
    let cache_dir = state.bootstrap.cache_dir.clone();

    let files = load_files_for_item_generation(pool, media_item_id).await?;
    if files.is_empty() {
        return Err(StoryboardError::MediaItemNotFound { media_item_id });
    }

    publish_progress(
        state,
        requesting_user_id,
        Uuid::nil(),
        Some(media_item_id),
        ProgressPhase::Started,
        files.len() as u64,
        0,
        0,
        0,
    );

    let cfg = resolve_generation_config(state, None).await;
    let runtime_config = state.runtime_config.load();
    let mut result = LibraryGenerationResult {
        candidates: files.len() as u64,
        ..Default::default()
    };
    let mut first_locked_media_file_id = None;

    for file in &files {
        let runtime_seconds = file.runtime_seconds.max(0) as u32;
        let mut cfg_with_interval = cfg.clone();
        cfg_with_interval.interval_seconds = resolve_interval(
            &cfg,
            &runtime_config.transcoding.storyboard_interval_mode,
            runtime_seconds,
        );
        let source_path = Path::new(&file.file_path);

        if !source_path.exists() {
            tracing::warn!(
                media_item_id = %media_item_id,
                media_file_id = %file.media_file_id,
                path = %file.file_path,
                "Source file missing, skipping storyboard generation"
            );
            result.errors += 1;
        } else {
            match generate_and_publish_storyboard(
                pool,
                &cache_dir,
                file,
                source_path,
                runtime_seconds,
                &cfg_with_interval,
            )
            .await
            {
                Ok(PublishedStoryboard::Published(gen_result)) => {
                    result.generated += 1;
                    tracing::info!(
                        media_item_id = %media_item_id,
                        media_file_id = %file.media_file_id,
                        sprite_count = gen_result.sprite_count,
                        duration_ms = gen_result.generation_duration_ms,
                        "Storyboard generated"
                    );
                }
                Ok(PublishedStoryboard::AlreadyRunning) => {
                    result.skipped += 1;
                    first_locked_media_file_id.get_or_insert(file.media_file_id);
                }
                Err(GenerationPublishError::Database(error)) => {
                    reconcile_storyboard_cache(pool, &cache_dir).await;
                    return Err(StoryboardError::Database(error));
                }
                Err(GenerationPublishError::Pipeline(error)) => {
                    tracing::warn!(
                        media_item_id = %media_item_id,
                        media_file_id = %file.media_file_id,
                        error = %error,
                        "Storyboard generation failed"
                    );
                    result.errors += 1;
                }
            }
        }

        publish_progress(
            state,
            requesting_user_id,
            file.media_file_id,
            Some(media_item_id),
            ProgressPhase::Progress,
            result.candidates,
            result.generated + result.skipped + result.errors,
            result.generated,
            result.errors,
        );
    }

    publish_progress(
        state,
        requesting_user_id,
        Uuid::nil(),
        Some(media_item_id),
        ProgressPhase::Completed,
        result.candidates,
        result.generated + result.skipped + result.errors,
        result.generated,
        result.errors,
    );
    reconcile_storyboard_cache(pool, &cache_dir).await;
    if result.generated == 0 && result.errors == 0 && result.skipped == result.candidates {
        return Err(StoryboardError::GenerationAlreadyInProgress {
            media_file_id: first_locked_media_file_id.unwrap_or(Uuid::nil()),
        });
    }
    Ok(result)
}

// ---------------------------------------------------------------------------
// Per-library pipeline
// ---------------------------------------------------------------------------

/// Generate storyboards for all files in a library that need them.
///
/// Performs the three enablement gates (global, library exists, per-library
/// metadata override), resolves the effective config, fetches incremental
/// candidates, and processes each file. Per-file errors are logged and
/// counted but do not abort the library run.
///
/// `requesting_user_id` — when `Some`, emits `storyboard_progress` events
/// to the user's SSE channel after each file. The scheduled task passes
/// `None`.
async fn generate_for_library(
    state: &AppState,
    library_id: Uuid,
    cache_dir: &Path,
    interval_mode_override: Option<&str>,
    requesting_user_id: Option<Uuid>,
) -> Result<LibraryGenerationResult, sqlx::Error> {
    let pool = &state.pool;
    let mut result = LibraryGenerationResult::default();

    // Gate 1: global enablement.
    let server_wide_enabled = state.runtime_config.load().transcoding.storyboards_enabled;
    if !server_wide_enabled {
        tracing::debug!(
            library_id = %library_id,
            "Storyboards disabled in server config, skipping library"
        );
        return Ok(result);
    }

    // Gate 2 + 3: library exists + per-library metadata enablement. Fetch
    // the metadata JSONB and check both gates in one round-trip.
    let library_meta = match fetch_library_metadata(pool, library_id).await? {
        Some(m) => m,
        None => {
            tracing::debug!(
                library_id = %library_id,
                "Library not found or soft-deleted, skipping storyboard generation"
            );
            return Ok(result);
        }
    };

    if !is_storyboards_enabled_for_library(&library_meta) {
        tracing::debug!(
            library_id = %library_id,
            "Storyboards disabled in library metadata, skipping"
        );
        return Ok(result);
    }

    // Resolve the effective generation config (server-wide + per-library
    // overrides for width and fixed-interval).
    let cfg = resolve_generation_config(state, Some(&library_meta)).await;
    let server_cfg = state.runtime_config.load();
    let interval_mode = normalized_interval_mode(
        interval_mode_override.unwrap_or(&server_cfg.transcoding.storyboard_interval_mode),
    );

    // Fetch incremental candidates.
    let candidates = fetch_files_needing_storyboards(pool, library_id, &cfg, interval_mode).await?;
    if candidates.is_empty() {
        tracing::debug!(library_id = %library_id, "No files need storyboard generation");
        return Ok(result);
    }
    result.candidates = candidates.len() as u64;

    publish_progress(
        state,
        requesting_user_id,
        Uuid::nil(),
        None,
        ProgressPhase::Started,
        result.candidates,
        0,
        0,
        0,
    );

    for file in &candidates {
        let runtime_seconds = file.runtime_seconds.max(0) as u32;
        let interval = resolve_interval(&cfg, interval_mode, runtime_seconds);

        let mut cfg_for_file = cfg.clone();
        cfg_for_file.interval_seconds = interval;

        let source_path = Path::new(&file.file_path);

        if !source_path.exists() {
            tracing::warn!(
                library_id = %library_id,
                media_file_id = %file.media_file_id,
                path = %file.file_path,
                "Source file missing, skipping"
            );
            result.errors += 1;
            publish_progress(
                state,
                requesting_user_id,
                file.media_file_id,
                None,
                ProgressPhase::Progress,
                result.candidates,
                result.generated + result.skipped + result.errors,
                result.generated,
                result.errors,
            );
            continue;
        }

        match generate_and_publish_storyboard(
            pool,
            cache_dir,
            file,
            source_path,
            runtime_seconds,
            &cfg_for_file,
        )
        .await
        {
            Ok(PublishedStoryboard::Published(gen_result)) => {
                result.generated += 1;
                tracing::debug!(
                    library_id = %library_id,
                    media_file_id = %file.media_file_id,
                    sprite_count = gen_result.sprite_count,
                    duration_ms = gen_result.generation_duration_ms,
                    "Storyboard generated"
                );
            }
            Ok(PublishedStoryboard::AlreadyRunning) => {
                result.skipped += 1;
                tracing::debug!(
                    library_id = %library_id,
                    media_file_id = %file.media_file_id,
                    "Storyboard generation already in progress"
                );
            }
            Err(error) => {
                tracing::warn!(
                    library_id = %library_id,
                    media_file_id = %file.media_file_id,
                    error = ?error,
                    "Storyboard generation or publication failed"
                );
                result.errors += 1;
            }
        }

        publish_progress(
            state,
            requesting_user_id,
            file.media_file_id,
            None,
            ProgressPhase::Progress,
            result.candidates,
            result.generated + result.skipped + result.errors,
            result.generated,
            result.errors,
        );
    }

    publish_progress(
        state,
        requesting_user_id,
        Uuid::nil(),
        None,
        ProgressPhase::Completed,
        result.candidates,
        result.generated + result.skipped + result.errors,
        result.generated,
        result.errors,
    );

    Ok(result)
}

// ---------------------------------------------------------------------------
// Result types
// ---------------------------------------------------------------------------

#[derive(Debug, Default)]
pub struct LibraryGenerationResult {
    pub candidates: u64,
    pub generated: u64,
    pub skipped: u64,
    pub errors: u64,
}

impl LibraryGenerationResult {
    pub fn message(&self) -> String {
        format!(
            "Processed {} candidate(s): {} generated, {} skipped, {} error(s)",
            self.candidates, self.generated, self.skipped, self.errors,
        )
    }
}

#[derive(Debug, Default)]
struct AggregateResult {
    candidates: u64,
    generated: u64,
    skipped: u64,
    errors: u64,
}

impl AggregateResult {
    fn add(&mut self, other: &LibraryGenerationResult) {
        self.candidates += other.candidates;
        self.generated += other.generated;
        self.skipped += other.skipped;
        self.errors += other.errors;
    }
}

// ---------------------------------------------------------------------------
// Progress event publishing
// ---------------------------------------------------------------------------

/// Lifecycle phase reported in `storyboard_progress` event payloads.
#[derive(Debug, Clone, Copy)]
enum ProgressPhase {
    Started,
    Progress,
    Completed,
}

impl ProgressPhase {
    fn as_str(self) -> &'static str {
        match self {
            ProgressPhase::Started => "started",
            ProgressPhase::Progress => "progress",
            ProgressPhase::Completed => "completed",
        }
    }
}

/// Publish a `storyboard_progress` SSE event to the requesting user's
/// channel (if any). No-op when `requesting_user_id` is `None` (scheduled
/// task invocation). Errors are silently swallowed — SSE is best-effort
/// progress feedback, not critical state.
#[allow(clippy::too_many_arguments)]
fn publish_progress(
    state: &AppState,
    requesting_user_id: Option<Uuid>,
    media_file_id: Uuid,
    media_item_id: Option<Uuid>,
    phase: ProgressPhase,
    candidates: u64,
    processed: u64,
    generated: u64,
    errors: u64,
) {
    let Some(user_id) = requesting_user_id else {
        return;
    };

    let payload = serde_json::json!({
        "phase": phase.as_str(),
        "library_id": null,
        "media_file_id": media_file_id,
        "media_item_id": media_item_id,
        "candidates": candidates,
        "processed": processed,
        "generated": generated,
        "errors": errors,
    });

    state
        .event_bus
        .publish(user_id, ServerEvent::new("storyboard_progress", payload));
}

// ---------------------------------------------------------------------------
// DB helpers
// ---------------------------------------------------------------------------

#[derive(Debug)]
struct FileCandidate {
    media_file_id: Uuid,
    file_path: String,
    file_hash: Option<String>,
    runtime_seconds: i32,
    storyboard_file_hash: Option<String>,
    storyboard_config_fingerprint: Option<String>,
}

#[derive(Debug, Default)]
struct StoryboardTaskConfig {
    library_id: Option<Uuid>,
    interval_mode: Option<String>,
}

#[derive(Debug)]
enum PublishedStoryboard {
    Published(sb_svc::GenerationResult),
    AlreadyRunning,
}

#[derive(Debug)]
enum GenerationPublishError {
    Pipeline(sb_svc::StoryboardPipelineError),
    Database(sqlx::Error),
}

/// Fetch all non-deleted, scan-enabled libraries. Used by the scheduled
/// iteration entry point.
async fn fetch_enabled_libraries(pool: &PgPool) -> Result<Vec<Uuid>, sqlx::Error> {
    let rows = sqlx::query(
        r#"
        SELECT id FROM libraries
        WHERE deleted_at IS NULL AND scan_enabled = true
        "#,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows.iter().map(|r| r.get::<Uuid, _>("id")).collect())
}

/// Fetch a library's `metadata` JSONB. Returns `None` if the library does
/// not exist or is soft-deleted (Gate 2).
async fn fetch_library_metadata(
    pool: &PgPool,
    library_id: Uuid,
) -> Result<Option<serde_json::Value>, sqlx::Error> {
    let row = sqlx::query(
        r#"
        SELECT metadata FROM libraries
        WHERE id = $1 AND deleted_at IS NULL AND scan_enabled = true
        "#,
    )
    .bind(library_id)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|r| r.get::<serde_json::Value, _>("metadata")))
}

fn parse_storyboard_task_config(config: serde_json::Value) -> Result<StoryboardTaskConfig, String> {
    let fields = config
        .as_object()
        .ok_or_else(|| "storyboard task config must be an object".to_string())?;

    for key in fields.keys() {
        if !matches!(
            key.as_str(),
            "library_id" | "interval_mode" | "max_concurrent_analyses"
        ) {
            return Err(format!("unsupported storyboard task config field: {key}"));
        }
    }

    let library_id = match fields.get("library_id") {
        Some(value) => Some(
            value
                .as_str()
                .ok_or_else(|| "storyboard task library_id must be a UUID string".to_string())
                .and_then(|value| {
                    Uuid::parse_str(value)
                        .map_err(|_| "storyboard task library_id must be a UUID string".to_string())
                })?,
        ),
        None => None,
    };
    let interval_mode = match fields.get("interval_mode") {
        Some(value) => {
            let value = value.as_str().ok_or_else(|| {
                "storyboard task interval_mode must be adaptive or fixed".to_string()
            })?;
            if !matches!(value, "adaptive" | "fixed") {
                return Err("storyboard task interval_mode must be adaptive or fixed".to_string());
            }
            Some(value.to_string())
        }
        None => None,
    };
    if let Some(value) = fields.get("max_concurrent_analyses")
        && value.as_u64() != Some(1)
    {
        return Err("storyboard task max_concurrent_analyses must be 1".to_string());
    }

    Ok(StoryboardTaskConfig {
        library_id,
        interval_mode,
    })
}

/// Gate 3: per-library enablement check. Returns `false` only when
/// `metadata.storyboards_enabled` is explicitly `false`. Absent key or
/// non-boolean value defaults to enabled (matching the design's "opt-in
/// disable" semantics — admins set `false` to opt a library out).
fn is_storyboards_enabled_for_library(metadata: &serde_json::Value) -> bool {
    metadata
        .get("storyboards_enabled")
        .and_then(|v| v.as_bool())
        .unwrap_or(true)
}

/// Resolve the effective [`GenerationConfig`] for a generation pass.
///
/// Merges server-wide config (`RuntimeConfig.transcoding.storyboard_*`)
/// with optional per-library overrides from `libraries.metadata`:
/// - `storyboard_width` overrides server width
/// - `storyboard_fixed_interval_seconds` overrides server fixed interval
///
async fn resolve_generation_config(
    state: &AppState,
    library_metadata: Option<&serde_json::Value>,
) -> GenerationConfig {
    let cfg = state.runtime_config.load();
    let transcoding = &cfg.transcoding;

    let mut width = transcoding.storyboard_width;
    let mut fixed_interval = transcoding.storyboard_fixed_interval_seconds;

    if let Some(meta) = library_metadata {
        if let Some(w) = meta.get("storyboard_width").and_then(|v| v.as_u64())
            && matches!(w as u32, 160 | 320 | 640)
        {
            width = w as u32;
        }
        if let Some(i) = meta
            .get("storyboard_fixed_interval_seconds")
            .and_then(|v| v.as_u64())
            && (2..=120).contains(&(i as u32))
        {
            fixed_interval = i as u32;
        }
    }

    GenerationConfig {
        width,
        interval_seconds: fixed_interval,
        quality: transcoding.storyboard_quality,
        keyframe_only: transcoding.storyboard_keyframe_only,
        sprite_columns: transcoding.storyboard_sprite_columns,
        sprite_rows: transcoding.storyboard_sprite_rows,
    }
}

/// Resolve the per-file interval. Adaptive mode uses
/// [`sb_svc::adaptive_interval`] keyed on the file's runtime; fixed mode
/// uses the config's interval_seconds directly.
fn resolve_interval(cfg: &GenerationConfig, interval_mode: &str, runtime_seconds: u32) -> u32 {
    if normalized_interval_mode(interval_mode) == "adaptive" {
        sb_svc::adaptive_interval(runtime_seconds)
    } else {
        cfg.interval_seconds
    }
}

fn normalized_interval_mode(interval_mode: &str) -> &str {
    match interval_mode {
        "fixed" => "fixed",
        _ => "adaptive",
    }
}

/// Incremental candidate query: loads each healthy movie/episode file with
/// its optional storyboard row so nullable source hashes and the effective
/// per-file generation fingerprint can be compared in Rust.
async fn fetch_files_needing_storyboards(
    pool: &PgPool,
    library_id: Uuid,
    cfg: &GenerationConfig,
    interval_mode: &str,
) -> Result<Vec<FileCandidate>, sqlx::Error> {
    let rows = sqlx::query(
        r#"
        SELECT
            mf.id           AS media_file_id,
            mf.file_path    AS file_path,
            mf.file_hash    AS file_hash,
            mf.runtime_seconds AS runtime_seconds,
            sb.file_hash AS storyboard_file_hash,
            sb.config_fingerprint AS storyboard_config_fingerprint
        FROM media_files mf
        JOIN media_items mi ON mi.id = mf.media_item_id
        LEFT JOIN storyboards sb ON sb.media_file_id = mf.id
        WHERE mi.library_id = $1
          AND mi.type IN ('movie', 'episode')
          AND mf.is_healthy = true
        ORDER BY mi.created_at ASC
        "#,
    )
    .bind(library_id)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .iter()
        .map(|r| FileCandidate {
            media_file_id: r.get("media_file_id"),
            file_path: r.get("file_path"),
            file_hash: r.get("file_hash"),
            runtime_seconds: r.get("runtime_seconds"),
            storyboard_file_hash: r.get("storyboard_file_hash"),
            storyboard_config_fingerprint: r.get("storyboard_config_fingerprint"),
        })
        .filter(|file| {
            let mut cfg_for_file = cfg.clone();
            cfg_for_file.interval_seconds =
                resolve_interval(cfg, interval_mode, file.runtime_seconds.max(0) as u32);
            storyboard_needs_regeneration(file, &cfg_for_file)
        })
        .collect())
}

async fn load_files_for_item_generation(
    pool: &PgPool,
    media_item_id: Uuid,
) -> Result<Vec<FileCandidate>, sqlx::Error> {
    let rows = sqlx::query(
        r#"
        SELECT
            mf.id           AS media_file_id,
            mf.file_path    AS file_path,
            mf.file_hash    AS file_hash,
            mf.runtime_seconds AS runtime_seconds
        FROM media_files mf
        WHERE mf.media_item_id = $1 AND mf.is_healthy = true
        ORDER BY mf.file_size DESC, mf.id ASC
        "#,
    )
    .bind(media_item_id)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .iter()
        .map(|r| FileCandidate {
            media_file_id: r.get("media_file_id"),
            file_path: r.get("file_path"),
            file_hash: r.get("file_hash"),
            runtime_seconds: r.get("runtime_seconds"),
            storyboard_file_hash: None,
            storyboard_config_fingerprint: None,
        })
        .collect())
}

async fn generate_and_publish_storyboard(
    pool: &PgPool,
    cache_dir: &Path,
    file: &FileCandidate,
    source_path: &Path,
    runtime_seconds: u32,
    cfg: &GenerationConfig,
) -> Result<PublishedStoryboard, GenerationPublishError> {
    let artifact_id = Uuid::now_v7();
    let output_dir = storyboard_dir(cache_dir, file.media_file_id, artifact_id);
    let mut tx = pool
        .begin()
        .await
        .map_err(GenerationPublishError::Database)?;
    let acquired: bool = sqlx::query_scalar("SELECT pg_try_advisory_xact_lock($1)")
        .bind(storyboard_lock_key(file.media_file_id))
        .fetch_one(&mut *tx)
        .await
        .map_err(GenerationPublishError::Database)?;

    if !acquired {
        return Ok(PublishedStoryboard::AlreadyRunning);
    }

    let result =
        match sb_svc::generate_storyboard(source_path, &output_dir, runtime_seconds, cfg).await {
            Ok(result) => result,
            Err(error) => {
                let _ = tokio::fs::remove_dir_all(&output_dir).await;
                return Err(GenerationPublishError::Pipeline(error));
            }
        };

    if let Err(error) = persist_storyboard_row(
        &mut tx,
        file.media_file_id,
        file.file_hash.as_deref(),
        artifact_id,
        cfg,
        &result,
    )
    .await
    {
        let _ = tokio::fs::remove_dir_all(&output_dir).await;
        return Err(GenerationPublishError::Database(error));
    }

    tx.commit()
        .await
        .map_err(GenerationPublishError::Database)?;

    Ok(PublishedStoryboard::Published(result))
}

async fn persist_storyboard_row(
    tx: &mut Transaction<'_, Postgres>,
    media_file_id: Uuid,
    file_hash: Option<&str>,
    artifact_id: Uuid,
    cfg: &GenerationConfig,
    result: &sb_svc::GenerationResult,
) -> Result<(), sqlx::Error> {
    let metadata = serde_json::json!({
        "columns": cfg.sprite_columns,
        "rows": cfg.sprite_rows,
    });

    sqlx::query(
        r#"
        INSERT INTO storyboards
            (media_file_id, file_hash, interval_seconds, width, height,
             sprite_count, total_thumbnails, total_size_bytes, keyframe_only,
             quality, generation_duration_ms, metadata, artifact_id, config_fingerprint)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)
        ON CONFLICT (media_file_id) DO UPDATE
        SET file_hash = EXCLUDED.file_hash,
            interval_seconds = EXCLUDED.interval_seconds,
            width = EXCLUDED.width,
            height = EXCLUDED.height,
            sprite_count = EXCLUDED.sprite_count,
            total_thumbnails = EXCLUDED.total_thumbnails,
            total_size_bytes = EXCLUDED.total_size_bytes,
            keyframe_only = EXCLUDED.keyframe_only,
            quality = EXCLUDED.quality,
            generated_at = now(),
            generation_duration_ms = EXCLUDED.generation_duration_ms,
            metadata = EXCLUDED.metadata,
            artifact_id = EXCLUDED.artifact_id,
            config_fingerprint = EXCLUDED.config_fingerprint,
            updated_at = now()
        "#,
    )
    .bind(media_file_id)
    .bind(file_hash)
    .bind(cfg.interval_seconds as i32)
    .bind(cfg.width as i32)
    .bind(result.height as i32)
    .bind(result.sprite_count as i32)
    .bind(result.total_thumbnails as i32)
    .bind(result.total_size_bytes as i64)
    .bind(cfg.keyframe_only)
    .bind(cfg.quality as i32)
    .bind(result.generation_duration_ms as i32)
    .bind(&metadata)
    .bind(artifact_id)
    .bind(generation_config_fingerprint(cfg))
    .execute(&mut **tx)
    .await?;
    Ok(())
}

fn generation_config_fingerprint(cfg: &GenerationConfig) -> String {
    format!(
        "v1:i={}:w={}:q={}:k={}:c={}:r={}",
        cfg.interval_seconds,
        cfg.width,
        cfg.quality,
        u8::from(cfg.keyframe_only),
        cfg.sprite_columns,
        cfg.sprite_rows,
    )
}

fn storyboard_needs_regeneration(file: &FileCandidate, cfg: &GenerationConfig) -> bool {
    let config_fingerprint = generation_config_fingerprint(cfg);
    file.storyboard_file_hash != file.file_hash
        || file.storyboard_config_fingerprint.as_deref() != Some(config_fingerprint.as_str())
}

fn storyboard_lock_key(media_file_id: Uuid) -> i64 {
    let mut bytes = [0; 8];
    bytes.copy_from_slice(&media_file_id.as_bytes()[..8]);
    i64::from_be_bytes(bytes)
}

fn storyboard_dir(cache_dir: &Path, media_file_id: Uuid, artifact_id: Uuid) -> std::path::PathBuf {
    cache_dir
        .join(STORYBOARDS_SUBDIR)
        .join(media_file_id.to_string())
        .join(artifact_id.to_string())
}

#[derive(Debug, Default)]
struct ArtifactReconcileResult {
    removed_dirs: u64,
    removed_files: u64,
    skipped_locked: u64,
    errors: u64,
}

async fn reconcile_storyboard_cache(pool: &PgPool, cache_dir: &Path) {
    let root = cache_dir.join(STORYBOARDS_SUBDIR);
    let mut media_dirs = match tokio::fs::read_dir(&root).await {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
        Err(error) => {
            tracing::warn!(error = %error, root = %root.display(), "failed to read storyboard cache root");
            return;
        }
    };

    let mut result = ArtifactReconcileResult::default();
    loop {
        let entry = match media_dirs.next_entry().await {
            Ok(Some(entry)) => entry,
            Ok(None) => break,
            Err(error) => {
                tracing::warn!(error = %error, root = %root.display(), "failed while reading storyboard cache root");
                result.errors += 1;
                break;
            }
        };
        let Ok(file_type) = entry.file_type().await else {
            result.errors += 1;
            continue;
        };
        if !file_type.is_dir() {
            continue;
        }

        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        let Ok(media_file_id) = Uuid::parse_str(name) else {
            continue;
        };

        let mut tx = match pool.begin().await {
            Ok(tx) => tx,
            Err(error) => {
                tracing::warn!(error = %error, "failed to begin storyboard cache reconciliation transaction");
                result.errors += 1;
                continue;
            }
        };
        let locked: bool = match sqlx::query_scalar("SELECT pg_try_advisory_xact_lock($1)")
            .bind(storyboard_lock_key(media_file_id))
            .fetch_one(&mut *tx)
            .await
        {
            Ok(locked) => locked,
            Err(error) => {
                tracing::warn!(media_file_id = %media_file_id, error = %error, "failed to lock storyboard cache directory for reconciliation");
                result.errors += 1;
                continue;
            }
        };
        if !locked {
            result.skipped_locked += 1;
            continue;
        }

        let active_artifact = match sqlx::query(
            "SELECT artifact_id FROM storyboards WHERE media_file_id = $1",
        )
        .bind(media_file_id)
        .fetch_optional(&mut *tx)
        .await
        {
            Ok(row) => row.map(|row| row.get::<Option<Uuid>, _>("artifact_id")),
            Err(error) => {
                tracing::warn!(media_file_id = %media_file_id, error = %error, "failed to resolve active storyboard artifact for reconciliation");
                result.errors += 1;
                continue;
            }
        };

        reconcile_storyboard_media_dir(&entry.path(), active_artifact, &mut result).await;
        if let Err(error) = tx.commit().await {
            tracing::warn!(media_file_id = %media_file_id, error = %error, "failed to complete storyboard cache reconciliation transaction");
            result.errors += 1;
        }
    }

    if result.removed_dirs > 0
        || result.removed_files > 0
        || result.skipped_locked > 0
        || result.errors > 0
    {
        tracing::info!(
            removed_dirs = result.removed_dirs,
            removed_files = result.removed_files,
            skipped_locked = result.skipped_locked,
            errors = result.errors,
            "storyboard cache reconciliation complete"
        );
    }
}

async fn reconcile_storyboard_media_dir(
    media_dir: &Path,
    active_artifact: Option<Option<Uuid>>,
    result: &mut ArtifactReconcileResult,
) {
    if active_artifact.is_none() {
        remove_storyboard_cache_dir(media_dir, result).await;
        return;
    }

    let active_artifact = active_artifact.flatten();
    let mut entries = match tokio::fs::read_dir(media_dir).await {
        Ok(entries) => entries,
        Err(error) => {
            tracing::warn!(error = %error, dir = %media_dir.display(), "failed to read storyboard media cache directory");
            result.errors += 1;
            return;
        }
    };

    loop {
        let entry = match entries.next_entry().await {
            Ok(Some(entry)) => entry,
            Ok(None) => break,
            Err(error) => {
                tracing::warn!(error = %error, dir = %media_dir.display(), "failed while reading storyboard media cache directory");
                result.errors += 1;
                break;
            }
        };
        let Ok(file_type) = entry.file_type().await else {
            result.errors += 1;
            continue;
        };
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !should_remove_storyboard_entry(active_artifact, &name, file_type.is_dir()) {
            continue;
        }

        if file_type.is_dir() {
            remove_storyboard_cache_dir(&entry.path(), result).await;
        } else {
            remove_storyboard_cache_file(&entry.path(), result).await;
        }
    }
}

fn should_remove_storyboard_entry(
    active_artifact: Option<Uuid>,
    name: &str,
    is_directory: bool,
) -> bool {
    match active_artifact {
        Some(active_artifact) => !is_directory || name != active_artifact.to_string(),
        None => is_directory,
    }
}

async fn remove_storyboard_cache_dir(path: &Path, result: &mut ArtifactReconcileResult) {
    match tokio::fs::remove_dir_all(path).await {
        Ok(()) => result.removed_dirs += 1,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            tracing::warn!(error = %error, path = %path.display(), "failed to remove orphaned storyboard cache directory");
            result.errors += 1;
        }
    }
}

async fn remove_storyboard_cache_file(path: &Path, result: &mut ArtifactReconcileResult) {
    match tokio::fs::remove_file(path).await {
        Ok(()) => result.removed_files += 1,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            tracing::warn!(error = %error, path = %path.display(), "failed to remove orphaned storyboard cache file");
            result.errors += 1;
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn library_result_message_format() {
        let r = LibraryGenerationResult {
            candidates: 10,
            generated: 7,
            skipped: 2,
            errors: 1,
        };
        let msg = r.message();
        assert!(msg.contains("10 candidate"));
        assert!(msg.contains("7 generated"));
        assert!(msg.contains("2 skipped"));
        assert!(msg.contains("1 error"));
    }

    #[test]
    fn library_result_default_message() {
        let r = LibraryGenerationResult::default();
        let msg = r.message();
        assert!(msg.contains("0 candidate"));
    }

    #[test]
    fn aggregate_add_accumulates() {
        let mut agg = AggregateResult::default();
        let r1 = LibraryGenerationResult {
            candidates: 5,
            generated: 3,
            skipped: 1,
            errors: 1,
        };
        let r2 = LibraryGenerationResult {
            candidates: 8,
            generated: 6,
            skipped: 2,
            errors: 0,
        };
        agg.add(&r1);
        agg.add(&r2);
        assert_eq!(agg.candidates, 13);
        assert_eq!(agg.generated, 9);
        assert_eq!(agg.skipped, 3);
        assert_eq!(agg.errors, 1);
    }

    #[test]
    fn is_storyboards_enabled_defaults_to_true_when_key_absent() {
        let meta = json!({});
        assert!(is_storyboards_enabled_for_library(&meta));
    }

    #[test]
    fn is_storyboards_enabled_respects_explicit_false() {
        let meta = json!({"storyboards_enabled": false});
        assert!(!is_storyboards_enabled_for_library(&meta));
    }

    #[test]
    fn is_storyboards_enabled_respects_explicit_true() {
        let meta = json!({"storyboards_enabled": true});
        assert!(is_storyboards_enabled_for_library(&meta));
    }

    #[test]
    fn is_storyboards_enabled_ignores_non_bool() {
        let meta = json!({"storyboards_enabled": "yes"});
        assert!(is_storyboards_enabled_for_library(&meta));
        let meta = json!({"storyboards_enabled": 1});
        assert!(is_storyboards_enabled_for_library(&meta));
    }

    #[test]
    fn storyboard_dir_layout() {
        let cache = Path::new("/var/cache/duskcue");
        let id = Uuid::nil();
        let dir = storyboard_dir(cache, id, Uuid::from_u128(1));
        assert_eq!(
            dir,
            std::path::Path::new(
                "/var/cache/duskcue/storyboards/00000000-0000-0000-0000-000000000000/00000000-0000-0000-0000-000000000001"
            )
        );
    }

    #[test]
    fn storyboard_lock_key_is_stable() {
        assert_eq!(storyboard_lock_key(Uuid::nil()), 0);
        assert_ne!(
            storyboard_lock_key(Uuid::nil()),
            storyboard_lock_key(Uuid::from_u128(1 << 120))
        );
    }

    #[test]
    fn artifact_reconciliation_preserves_only_the_active_version() {
        let active = Uuid::from_u128(1);
        assert!(!should_remove_storyboard_entry(
            Some(active),
            &active.to_string(),
            true
        ));
        assert!(should_remove_storyboard_entry(
            Some(active),
            "00000000-0000-0000-0000-000000000002",
            true
        ));
        assert!(should_remove_storyboard_entry(
            Some(active),
            "index.vtt",
            false
        ));
    }

    #[test]
    fn artifact_reconciliation_preserves_legacy_root_files() {
        assert!(!should_remove_storyboard_entry(None, "index.vtt", false));
        assert!(should_remove_storyboard_entry(
            None,
            "00000000-0000-0000-0000-000000000001",
            true
        ));
    }

    #[tokio::test]
    async fn artifact_reconciliation_removes_unreferenced_artifacts() {
        let root = std::env::temp_dir().join(format!("duskcue-storyboard-{}", Uuid::now_v7()));
        let active = Uuid::from_u128(1);
        let stale = Uuid::from_u128(2);
        let media_dir = root.join(Uuid::nil().to_string());
        tokio::fs::create_dir_all(media_dir.join(active.to_string()))
            .await
            .unwrap();
        tokio::fs::create_dir_all(media_dir.join(stale.to_string()))
            .await
            .unwrap();
        tokio::fs::write(media_dir.join("index.vtt"), "legacy")
            .await
            .unwrap();

        let mut result = ArtifactReconcileResult::default();
        reconcile_storyboard_media_dir(&media_dir, Some(Some(active)), &mut result).await;

        assert!(media_dir.join(active.to_string()).exists());
        assert!(!media_dir.join(stale.to_string()).exists());
        assert!(!media_dir.join("index.vtt").exists());
        assert_eq!(result.removed_dirs, 1);
        assert_eq!(result.removed_files, 1);
        let _ = tokio::fs::remove_dir_all(root).await;
    }

    #[test]
    fn null_file_hash_is_fresh_when_the_config_matches() {
        let cfg = GenerationConfig::default();
        let file = FileCandidate {
            media_file_id: Uuid::nil(),
            file_path: "/media/example.mkv".to_string(),
            file_hash: None,
            runtime_seconds: 600,
            storyboard_file_hash: None,
            storyboard_config_fingerprint: Some(generation_config_fingerprint(&cfg)),
        };
        assert!(!storyboard_needs_regeneration(&file, &cfg));
    }

    #[test]
    fn changed_generation_config_requires_regeneration() {
        let cfg = GenerationConfig::default();
        let file = FileCandidate {
            media_file_id: Uuid::nil(),
            file_path: "/media/example.mkv".to_string(),
            file_hash: Some("hash".to_string()),
            runtime_seconds: 600,
            storyboard_file_hash: Some("hash".to_string()),
            storyboard_config_fingerprint: Some(generation_config_fingerprint(&cfg)),
        };
        let changed = GenerationConfig { width: 640, ..cfg };
        assert!(storyboard_needs_regeneration(&file, &changed));
    }

    #[test]
    fn storyboard_task_config_accepts_supported_values() {
        let config = parse_storyboard_task_config(json!({
            "library_id": "00000000-0000-0000-0000-000000000001",
            "interval_mode": "fixed",
            "max_concurrent_analyses": 1,
        }))
        .unwrap();
        assert_eq!(config.library_id, Some(Uuid::from_u128(1)));
        assert_eq!(config.interval_mode.as_deref(), Some("fixed"));
    }

    #[test]
    fn storyboard_task_config_rejects_invalid_values() {
        assert!(parse_storyboard_task_config(json!({"library_id": "not-a-uuid"})).is_err());
        assert!(parse_storyboard_task_config(json!({"interval_mode": "hourly"})).is_err());
        assert!(parse_storyboard_task_config(json!({"max_concurrent_analyses": 2})).is_err());
        assert!(parse_storyboard_task_config(json!({"unexpected": true})).is_err());
    }
}

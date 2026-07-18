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

//! Background segment detector — the orchestration point for the 4-method
//! detection pipeline defined in
//! [`crate::services::segments`](../../services/segments/index.html).
//!
//! Implements the 6-phase pipeline from
//! [SEGMENT_DETECTION.md](../../docs/design/SEGMENT_DETECTION.md):
//!
//! 1. Resolve candidates (incremental — only items needing analysis)
//! 2. Chapter marker extraction (Method 1) — instant, from stored JSONB
//! 3. Chromaprint audio fingerprinting (Method 2) — CPU-bound, cached
//! 4. Cross-episode fingerprint comparison (Method 2 cont.) — intros only
//! 5. Black frame + silence detection (Methods 3 & 4) — credits
//! 6. Silence-gap detection after existing credits — outro candidates
//! 7. Report results
//!
//! ## Incremental analysis
//!
//! Files with a matching `media_fingerprints` row (same `file_hash`) skip
//! expensive audio fingerprinting. All healthy files still enter the cheap
//! chapter pass because chapter data lives in `media_files` and may change
//! without a fingerprint change.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Instant;

use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::services::segments as seg;
use crate::services::segments::{
    BlackframeParams, ChromaprintThresholds, SafetyConfig, SilenceParams,
};
use crate::state::{AppState, SegmentAnalysisConfig};

const ALGORITHM: &str = "test2";

pub async fn run_segment_analysis(state: &AppState, task_id: Uuid, config: serde_json::Value) {
    tracing::info!(task_id = %task_id, "Starting segment analysis task");

    let (safety, analysis_cfg, enabled) = resolve_config(state);
    if !enabled {
        tracing::info!(
            task_id = %task_id,
            "Segment detection disabled in server config, skipping"
        );
        return;
    }

    let pool = &state.pool;

    let library_ids: Vec<Uuid> = if let Some(id) = config
        .get("library_id")
        .and_then(|v| v.as_str())
        .and_then(|s| Uuid::parse_str(s).ok())
    {
        vec![id]
    } else {
        match fetch_enabled_libraries(pool).await {
            Ok(ids) => ids,
            Err(e) => {
                tracing::error!(task_id = %task_id, error = %e, "Failed to fetch libraries for segment analysis");
                return;
            }
        }
    };

    if library_ids.is_empty() {
        tracing::info!(task_id = %task_id, "No libraries to analyze");
        return;
    }

    let mut total = AggregateResult::default();
    for library_id in &library_ids {
        tracing::info!(task_id = %task_id, library_id = %library_id, "Analyzing library");
        match analyze_library(pool, *library_id, &safety, &analysis_cfg).await {
            Ok(result) => {
                tracing::info!(
                    task_id = %task_id,
                    library_id = %library_id,
                    candidates = result.candidates,
                    chapters_matched = result.chapters_matched,
                    segments_created = result.segments_created,
                    segments_updated = result.segments_updated,
                    fingerprints_computed = result.fingerprints_computed,
                    fingerprints_reused = result.fingerprints_reused,
                    errors = result.errors,
                    "Library analysis complete"
                );
                total.add(&result);
            }
            Err(e) => {
                tracing::warn!(
                    task_id = %task_id,
                    library_id = %library_id,
                    error = %e,
                    "Library analysis failed"
                );
            }
        }
    }

    tracing::info!(
        task_id = %task_id,
        libraries = library_ids.len(),
        candidates = total.candidates,
        chapters_matched = total.chapters_matched,
        segments_created = total.segments_created,
        segments_updated = total.segments_updated,
        fingerprints_computed = total.fingerprints_computed,
        fingerprints_reused = total.fingerprints_reused,
        errors = total.errors,
        "Segment analysis task completed"
    );
}

pub async fn analyze_library_one(
    state: &AppState,
    library_id: Uuid,
) -> Result<LibraryAnalysisResult, sqlx::Error> {
    let (safety, analysis_cfg, _) = resolve_config(state);
    analyze_library(&state.pool, library_id, &safety, &analysis_cfg).await
}

async fn analyze_library(
    pool: &PgPool,
    library_id: Uuid,
    safety: &SafetyConfig,
    analysis_cfg: &SegmentAnalysisConfig,
) -> Result<LibraryAnalysisResult, sqlx::Error> {
    let started = Instant::now();
    let result = analyze_library_inner(pool, library_id, safety, analysis_cfg).await;
    metrics::histogram!("segment_analysis_duration_seconds")
        .record(started.elapsed().as_secs_f64());
    if let Err(error) = record_active_segments(pool).await {
        record_analysis_error(SegmentAnalysisErrorStage::Database);
        tracing::warn!(library_id = %library_id, error = %error, "Failed to refresh active segment metrics");
    }
    if result.is_err() {
        record_analysis_error(SegmentAnalysisErrorStage::Database);
    }
    result
}

async fn analyze_library_inner(
    pool: &PgPool,
    library_id: Uuid,
    safety: &SafetyConfig,
    analysis_cfg: &SegmentAnalysisConfig,
) -> Result<LibraryAnalysisResult, sqlx::Error> {
    let mut result = LibraryAnalysisResult::default();

    let candidates = match fetch_candidates(pool, library_id).await {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!(library_id = %library_id, error = %e, "Failed to fetch candidates");
            return Err(e);
        }
    };

    if candidates.is_empty() {
        tracing::debug!(library_id = %library_id, "No candidates for segment analysis");
        return Ok(result);
    }

    result.candidates = candidates.len() as u64;

    let chromaprint_thresholds = ChromaprintThresholds::default();
    let blackframe_params = BlackframeParams {
        amount: analysis_cfg.blackframe_amount,
        threshold: analysis_cfg.blackframe_threshold,
    };
    let silence_params = SilenceParams {
        noise_db: analysis_cfg.silence_noise_db,
        min_duration_ms: analysis_cfg.silence_min_duration_ms,
    };

    let mut chapter_resolved: Vec<Uuid> = Vec::new();
    let mut needs_fingerprint: Vec<&Candidate> = Vec::new();

    for candidate in &candidates {
        record_analysis_file(SegmentAnalysisMethod::Chapter);
        let chapters = seg::extract_chapters(&candidate.additional_streams);
        let matched =
            apply_chapter_segments(pool, candidate, &chapters, safety, candidate.is_movie).await;

        result.chapters_matched += matched as u64;
        if matched > 0 {
            chapter_resolved.push(candidate.media_item_id);
        } else {
            needs_fingerprint.push(candidate);
        }
    }

    for candidate in &needs_fingerprint {
        match fingerprint_and_store(pool, candidate, analysis_cfg).await {
            Ok(FingerprintOutcome::Computed) => result.fingerprints_computed += 1,
            Ok(FingerprintOutcome::Reused) => result.fingerprints_reused += 1,
            Err(e) => {
                record_analysis_error(SegmentAnalysisErrorStage::Chromaprint);
                tracing::warn!(
                    media_item_id = %candidate.media_item_id,
                    media_file_id = %candidate.media_file_id,
                    error = %e,
                    "Fingerprinting failed"
                );
                result.errors += 1;
            }
        }
    }

    let intro_segments = match cross_compare_seasons(
        pool,
        library_id,
        &chromaprint_thresholds,
        safety,
    )
    .await
    {
        Ok(n) => n,
        Err(e) => {
            record_analysis_error(SegmentAnalysisErrorStage::Chromaprint);
            tracing::warn!(library_id = %library_id, error = %e, "Cross-episode chromaprint comparison failed");
            0
        }
    };
    result.segments_created += intro_segments as u64;

    let skip_set: Vec<Uuid> = chapter_resolved.clone();
    let credits_segments = match detect_credits(
        pool,
        library_id,
        &skip_set,
        &blackframe_params,
        &silence_params,
        safety,
    )
    .await
    {
        Ok(n) => n,
        Err(e) => {
            record_analysis_error(SegmentAnalysisErrorStage::Database);
            tracing::warn!(library_id = %library_id, error = %e, "Credits detection failed");
            0
        }
    };
    result.segments_created += credits_segments as u64;

    let outro_segments = match detect_outros(pool, library_id, &silence_params, safety).await {
        Ok(n) => n,
        Err(e) => {
            record_analysis_error(SegmentAnalysisErrorStage::Database);
            tracing::warn!(library_id = %library_id, error = %e, "Outro detection failed");
            0
        }
    };
    result.segments_created += outro_segments as u64;

    Ok(result)
}

#[derive(Debug, Default)]
pub struct LibraryAnalysisResult {
    pub candidates: u64,
    pub chapters_matched: u64,
    pub segments_created: u64,
    pub segments_updated: u64,
    pub fingerprints_computed: u64,
    pub fingerprints_reused: u64,
    pub errors: u64,
}

impl LibraryAnalysisResult {
    pub fn message(&self) -> String {
        format!(
            "Analyzed {} candidate(s): {} chapter match(es), {} segment(s) created, {} fingerprint(s) computed, {} error(s)",
            self.candidates,
            self.chapters_matched,
            self.segments_created,
            self.fingerprints_computed,
            self.errors,
        )
    }
}

#[derive(Debug, Default)]
struct AggregateResult {
    candidates: u64,
    chapters_matched: u64,
    segments_created: u64,
    segments_updated: u64,
    fingerprints_computed: u64,
    fingerprints_reused: u64,
    errors: u64,
}

impl AggregateResult {
    fn add(&mut self, other: &LibraryAnalysisResult) {
        self.candidates += other.candidates;
        self.chapters_matched += other.chapters_matched;
        self.segments_created += other.segments_created;
        self.segments_updated += other.segments_updated;
        self.fingerprints_computed += other.fingerprints_computed;
        self.fingerprints_reused += other.fingerprints_reused;
        self.errors += other.errors;
    }
}

#[derive(Debug)]
struct Candidate {
    media_item_id: Uuid,
    media_file_id: Uuid,
    file_path: String,
    file_hash: Option<String>,
    runtime_seconds: i32,
    additional_streams: serde_json::Value,
    is_movie: bool,
    fingerprint_cached: bool,
}

enum FingerprintOutcome {
    Computed,
    Reused,
}

#[derive(Clone, Copy)]
enum SegmentAnalysisMethod {
    Chapter,
    Chromaprint,
    Blackframe,
    Silence,
}

impl SegmentAnalysisMethod {
    fn as_str(self) -> &'static str {
        match self {
            Self::Chapter => "chapter",
            Self::Chromaprint => "chromaprint",
            Self::Blackframe => "blackframe",
            Self::Silence => "silence",
        }
    }
}

#[derive(Clone, Copy)]
enum SegmentAnalysisErrorStage {
    Database,
    Chapter,
    Chromaprint,
    Blackframe,
    Silence,
}

impl SegmentAnalysisErrorStage {
    fn as_str(self) -> &'static str {
        match self {
            Self::Database => "database",
            Self::Chapter => "chapter",
            Self::Chromaprint => "chromaprint",
            Self::Blackframe => "blackframe",
            Self::Silence => "silence",
        }
    }
}

fn record_analysis_file(method: SegmentAnalysisMethod) {
    metrics::counter!("segment_analysis_files_total", "method" => method.as_str()).increment(1);
}

fn record_analysis_error(stage: SegmentAnalysisErrorStage) {
    metrics::counter!("segment_analysis_errors_total", "stage" => stage.as_str()).increment(1);
}

fn record_segment_created(detected: &seg::DetectedSegment) {
    metrics::counter!(
        "segment_segments_created_total",
        "type" => detected.segment_type.as_str(),
        "source" => detected.source.as_str()
    )
    .increment(1);
    if detected
        .metadata
        .get("surfaced")
        .and_then(|value| value.as_bool())
        == Some(false)
    {
        metrics::counter!("segment_low_confidence_total").increment(1);
    }
}

async fn record_active_segments(pool: &PgPool) -> Result<(), sqlx::Error> {
    let rows = sqlx::query(
        r#"
        SELECT segment_type, COUNT(*)::BIGINT AS count
        FROM media_segments
        GROUP BY segment_type
        "#,
    )
    .fetch_all(pool)
    .await?;
    let counts: HashMap<String, i64> = rows
        .iter()
        .map(|row| (row.get("segment_type"), row.get("count")))
        .collect();
    for segment_type in seg::VALID_SEGMENT_TYPES {
        metrics::gauge!("segment_segments_active", "type" => *segment_type)
            .set(counts.get(*segment_type).copied().unwrap_or_default() as f64);
    }
    Ok(())
}

fn resolve_config(state: &AppState) -> (SafetyConfig, SegmentAnalysisConfig, bool) {
    let cfg = state.runtime_config.load();
    let transcoding = &cfg.transcoding;
    (
        SafetyConfig {
            intro_start_padding_ms: 0,
            intro_end_padding_ms: transcoding.segment_safety.intro_end_padding_ms,
            credits_start_padding_ms: 0,
            credits_end_padding_ms: transcoding.segment_safety.credits_end_padding_ms,
            min_confidence: transcoding.segment_safety.min_confidence,
        },
        SegmentAnalysisConfig {
            max_concurrent_analyses: transcoding.segment_analysis.max_concurrent_analyses,
            chromaprint_sample_rate: transcoding.segment_analysis.chromaprint_sample_rate,
            blackframe_amount: transcoding.segment_analysis.blackframe_amount,
            blackframe_threshold: transcoding.segment_analysis.blackframe_threshold,
            silence_noise_db: transcoding.segment_analysis.silence_noise_db,
            silence_min_duration_ms: transcoding.segment_analysis.silence_min_duration_ms,
        },
        transcoding.segment_detection_enabled,
    )
}

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

async fn fetch_candidates(pool: &PgPool, library_id: Uuid) -> Result<Vec<Candidate>, sqlx::Error> {
    let rows = sqlx::query(
        r#"
        SELECT
            mi.id              AS media_item_id,
            mf.id              AS media_file_id,
            mf.file_path       AS file_path,
            mf.file_hash       AS file_hash,
            mf.runtime_seconds AS runtime_seconds,
            mf.additional_streams AS additional_streams,
            (mi.type = 'movie') AS is_movie,
            EXISTS (
                SELECT 1
                FROM media_fingerprints fp
                WHERE fp.media_file_id = mf.id
                  AND fp.file_hash = mf.file_hash
                  AND mf.file_hash IS NOT NULL
                  AND mf.file_hash <> ''
            ) AS fingerprint_cached
        FROM media_files mf
        JOIN media_items mi ON mf.media_item_id = mi.id
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
        .map(|r| Candidate {
            media_item_id: r.get("media_item_id"),
            media_file_id: r.get("media_file_id"),
            file_path: r.get("file_path"),
            file_hash: r.get("file_hash"),
            runtime_seconds: r.get("runtime_seconds"),
            additional_streams: r.get("additional_streams"),
            is_movie: r.get("is_movie"),
            fingerprint_cached: r.get("fingerprint_cached"),
        })
        .collect())
}

async fn apply_chapter_segments(
    pool: &PgPool,
    candidate: &Candidate,
    chapters: &[seg::ChapterInfo],
    safety: &SafetyConfig,
    is_movie: bool,
) -> usize {
    if chapters.is_empty() {
        return 0;
    }

    let runtime_ms = candidate.runtime_seconds.max(0) * 1_000;
    let mut matched = 0;

    for chapter in chapters {
        let Some(title) = chapter.title.as_deref() else {
            continue;
        };
        let Some(seg_type) = seg::classify_chapter_title(title) else {
            continue;
        };

        let thresholds = seg::DurationThresholds::for_type(seg_type);
        let duration = chapter.duration_ms();
        if duration < thresholds.min_ms || duration > thresholds.max_for(is_movie) {
            tracing::debug!(
                media_item_id = %candidate.media_item_id,
                chapter_title = %title,
                duration_ms = duration,
                "Chapter matched type but outside duration thresholds, skipping"
            );
            continue;
        }

        match has_segment_for_type(pool, candidate.media_item_id, seg_type.as_str()).await {
            Ok(true) => continue,
            Ok(false) => {}
            Err(error) => {
                record_analysis_error(SegmentAnalysisErrorStage::Chapter);
                tracing::warn!(
                    media_item_id = %candidate.media_item_id,
                    chapter_title = %title,
                    error = %error,
                    "Failed to check for an existing chapter-derived segment"
                );
                continue;
            }
        }

        let detected = seg::chapter_to_detected_segment(chapter, seg_type, runtime_ms, safety);
        let mut detected = detected;
        seg::mark_surfaced(&mut detected, safety);

        match insert_segment(pool, candidate.media_item_id, &detected).await {
            Ok(true) => matched += 1,
            Ok(false) => {}
            Err(e) => {
                record_analysis_error(SegmentAnalysisErrorStage::Chapter);
                tracing::warn!(
                    media_item_id = %candidate.media_item_id,
                    chapter_title = %title,
                    error = %e,
                    "Failed to insert chapter-derived segment"
                );
            }
        }
    }

    matched
}

async fn has_segment_for_type(
    pool: &PgPool,
    media_item_id: Uuid,
    segment_type: &str,
) -> Result<bool, sqlx::Error> {
    let existing: Option<(Uuid,)> = sqlx::query_as(
        r#"
        SELECT id FROM media_segments
        WHERE media_item_id = $1 AND segment_type = $2
        LIMIT 1
        "#,
    )
    .bind(media_item_id)
    .bind(segment_type)
    .fetch_optional(pool)
    .await?;
    Ok(existing.is_some())
}

async fn fingerprint_and_store(
    pool: &PgPool,
    candidate: &Candidate,
    analysis_cfg: &SegmentAnalysisConfig,
) -> Result<FingerprintOutcome, seg::SegmentPipelineError> {
    if candidate.fingerprint_cached {
        return Ok(FingerprintOutcome::Reused);
    }

    let path = PathBuf::from(&candidate.file_path);
    if !path.exists() {
        return Err(seg::SegmentPipelineError::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("media file not found: {}", candidate.file_path),
        )));
    }

    record_analysis_file(SegmentAnalysisMethod::Chromaprint);
    let output = seg::fingerprint_file(&path, analysis_cfg.chromaprint_sample_rate).await?;

    let raw_bytes: Vec<u8> = output.raw.iter().flat_map(|v| v.to_le_bytes()).collect();

    let file_hash = candidate
        .file_hash
        .clone()
        .unwrap_or_else(|| "unknown".to_string());

    let chapters_json = if candidate.additional_streams.get("chapters").is_some() {
        Some(candidate.additional_streams.get("chapters").cloned())
    } else {
        None
    };

    sqlx::query(
        r#"
        INSERT INTO media_fingerprints
            (media_file_id, file_hash, fingerprint, fingerprint_algorithm,
             fingerprint_duration_ms, chapters_json, metadata)
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        ON CONFLICT (media_file_id) DO UPDATE
        SET file_hash = EXCLUDED.file_hash,
            fingerprint = EXCLUDED.fingerprint,
            fingerprint_algorithm = EXCLUDED.fingerprint_algorithm,
            fingerprint_duration_ms = EXCLUDED.fingerprint_duration_ms,
            chapters_json = EXCLUDED.chapters_json,
            metadata = EXCLUDED.metadata,
            updated_at = now()
        "#,
    )
    .bind(candidate.media_file_id)
    .bind(&file_hash)
    .bind(&raw_bytes)
    .bind(ALGORITHM)
    .bind(output.duration_ms)
    .bind(chapters_json)
    .bind(serde_json::json!({
        "sample_rate": output.sample_rate,
        "subfp_count": output.raw.len(),
    }))
    .execute(pool)
    .await
    .map_err(|e| {
        tracing::warn!(error = %e, "Failed to store fingerprint");
        seg::SegmentPipelineError::Io(std::io::Error::other(e.to_string()))
    })?;

    Ok(FingerprintOutcome::Computed)
}

async fn cross_compare_seasons(
    pool: &PgPool,
    library_id: Uuid,
    thresholds: &ChromaprintThresholds,
    safety: &SafetyConfig,
) -> Result<usize, sqlx::Error> {
    let seasons = fetch_fingerprinted_seasons(pool, library_id).await?;
    if seasons.is_empty() {
        return Ok(0);
    }

    let mut created = 0usize;
    for season in seasons {
        let eps = match load_season_fingerprints(pool, season.season_id).await {
            Ok(e) if e.len() < 2 => continue,
            Ok(e) => e,
            Err(e) => {
                record_analysis_error(SegmentAnalysisErrorStage::Chromaprint);
                tracing::warn!(season_id = %season.season_id, error = %e, "Failed to load season fingerprints");
                continue;
            }
        };

        let matches = seg::find_recurring_segments(&eps, thresholds);
        if matches.is_empty() {
            continue;
        }

        let runtime_by_item: HashMap<Uuid, i32> = eps
            .iter()
            .map(|e| (e.media_item_id, e.runtime_ms))
            .collect();

        for m in &matches {
            match has_segment_for_type(pool, m.media_item_id, seg::SegmentType::Intro.as_str())
                .await
            {
                Ok(true) => continue,
                Ok(false) => {}
                Err(error) => {
                    record_analysis_error(SegmentAnalysisErrorStage::Chromaprint);
                    tracing::warn!(
                        media_item_id = %m.media_item_id,
                        error = %error,
                        "Failed to check for an existing chromaprint intro segment"
                    );
                    continue;
                }
            }

            let runtime = runtime_by_item
                .get(&m.media_item_id)
                .copied()
                .unwrap_or_else(|| season.avg_runtime_seconds.unwrap_or(0).max(0) * 1_000);

            let detected =
                seg::chromaprint_match_to_segment(m, thresholds, false, false, runtime, safety);
            let mut detected = detected;
            seg::mark_surfaced(&mut detected, safety);

            match insert_segment(pool, m.media_item_id, &detected).await {
                Ok(true) => created += 1,
                Ok(false) => {}
                Err(error) => {
                    record_analysis_error(SegmentAnalysisErrorStage::Chromaprint);
                    tracing::warn!(media_item_id = %m.media_item_id, error = %error, "Failed to insert chromaprint intro segment");
                }
            }
        }
    }

    Ok(created)
}

#[derive(Debug)]
struct SeasonSummary {
    season_id: Uuid,
    avg_runtime_seconds: Option<i32>,
}

async fn fetch_fingerprinted_seasons(
    pool: &PgPool,
    library_id: Uuid,
) -> Result<Vec<SeasonSummary>, sqlx::Error> {
    let rows = sqlx::query(
        r#"
        SELECT
            ep.season_id AS season_id,
            AVG(mf.runtime_seconds)::INT AS avg_runtime_seconds
        FROM media_fingerprints fp
        JOIN media_files mf ON mf.id = fp.media_file_id
        JOIN media_items mi ON mi.id = mf.media_item_id
        JOIN episodes ep ON ep.id = mi.id
        WHERE mi.library_id = $1
          AND mi.type = 'episode'
          AND ep.season_id IS NOT NULL
        GROUP BY ep.season_id
        HAVING COUNT(*) >= 2
        "#,
    )
    .bind(library_id)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .iter()
        .map(|r| SeasonSummary {
            season_id: r.get("season_id"),
            avg_runtime_seconds: r.get("avg_runtime_seconds"),
        })
        .collect())
}

async fn load_season_fingerprints(
    pool: &PgPool,
    season_id: Uuid,
) -> Result<Vec<seg::FingerprintWithContext>, sqlx::Error> {
    let rows = sqlx::query(
        r#"
        SELECT
            mi.id AS media_item_id,
            mf.runtime_seconds AS runtime_seconds,
            fp.fingerprint AS fingerprint
        FROM media_fingerprints fp
        JOIN media_files mf ON mf.id = fp.media_file_id
        JOIN media_items mi ON mi.id = mf.media_item_id
        JOIN episodes ep ON ep.id = mi.id
        WHERE ep.season_id = $1
        "#,
    )
    .bind(season_id)
    .fetch_all(pool)
    .await?;

    let mut out = Vec::with_capacity(rows.len());
    for r in &rows {
        let bytes: Vec<u8> = r.get("fingerprint");
        if !bytes.len().is_multiple_of(4) {
            tracing::warn!(
                media_item_id = %r.get::<Uuid, _>("media_item_id"),
                "Stored fingerprint has non-multiple-of-4 length, skipping"
            );
            continue;
        }
        let raw: Vec<u32> = bytes
            .chunks_exact(4)
            .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect();
        if raw.is_empty() {
            continue;
        }
        let runtime_seconds: i32 = r.get("runtime_seconds");
        out.push(seg::FingerprintWithContext {
            media_item_id: r.get("media_item_id"),
            runtime_ms: runtime_seconds.max(0) * 1_000,
            fingerprint: raw,
        });
    }
    Ok(out)
}

async fn detect_credits(
    pool: &PgPool,
    library_id: Uuid,
    skip_item_ids: &[Uuid],
    blackframe_params: &BlackframeParams,
    silence_params: &SilenceParams,
    safety: &SafetyConfig,
) -> Result<usize, sqlx::Error> {
    let items = fetch_items_needing_credits(pool, library_id, skip_item_ids).await?;
    if items.is_empty() {
        return Ok(0);
    }

    let mut created = 0usize;
    for item in &items {
        if has_segment_for_type(pool, item.media_item_id, seg::SegmentType::Credits.as_str())
            .await
            .unwrap_or(false)
        {
            continue;
        }

        let path = PathBuf::from(&item.file_path);
        if !path.exists() {
            continue;
        }

        let runtime_ms = item.runtime_seconds.max(0) * 1_000;
        let search_window = seg::credits_search_window_ms(runtime_ms, item.is_movie);

        record_analysis_file(SegmentAnalysisMethod::Blackframe);
        let blackframes = match seg::detect_blackframes(&path, blackframe_params, search_window)
            .await
        {
            Ok(b) => b,
            Err(e) => {
                record_analysis_error(SegmentAnalysisErrorStage::Blackframe);
                tracing::warn!(media_item_id = %item.media_item_id, error = %e, "Blackframe detection failed");
                continue;
            }
        };
        record_analysis_file(SegmentAnalysisMethod::Silence);
        let silence = match seg::detect_silence(&path, silence_params).await {
            Ok(s) => s,
            Err(e) => {
                record_analysis_error(SegmentAnalysisErrorStage::Silence);
                tracing::warn!(media_item_id = %item.media_item_id, error = %e, "Silence detection failed");
                continue;
            }
        };

        let segments =
            seg::combine_credits_signals(&blackframes, &silence, search_window, runtime_ms, safety);

        for mut seg_ in segments {
            seg::mark_surfaced(&mut seg_, safety);
            match insert_segment(pool, item.media_item_id, &seg_).await {
                Ok(true) => created += 1,
                Ok(false) => {}
                Err(e) => {
                    record_analysis_error(SegmentAnalysisErrorStage::Database);
                    tracing::warn!(
                        media_item_id = %item.media_item_id,
                        error = %e,
                        "Failed to insert credits segment"
                    );
                }
            }
        }
    }

    Ok(created)
}

#[derive(Debug)]
struct CreditsCandidate {
    media_item_id: Uuid,
    file_path: String,
    runtime_seconds: i32,
    is_movie: bool,
}

async fn fetch_items_needing_credits(
    pool: &PgPool,
    library_id: Uuid,
    skip_item_ids: &[Uuid],
) -> Result<Vec<CreditsCandidate>, sqlx::Error> {
    let rows = sqlx::query(
        r#"
        SELECT
            mi.id AS media_item_id,
            mf.file_path AS file_path,
            mf.runtime_seconds AS runtime_seconds,
            (mi.type = 'movie') AS is_movie
        FROM media_files mf
        JOIN media_items mi ON mi.id = mf.media_item_id
        WHERE mi.library_id = $1
          AND mi.type IN ('movie', 'episode')
          AND mf.is_healthy = true
          AND NOT EXISTS (
              SELECT 1 FROM media_segments s
              WHERE s.media_item_id = mi.id AND s.segment_type = 'credits'
          )
        "#,
    )
    .bind(library_id)
    .fetch_all(pool)
    .await?;

    let mut out = Vec::with_capacity(rows.len());
    for r in &rows {
        let media_item_id: Uuid = r.get("media_item_id");
        if skip_item_ids.contains(&media_item_id) {
            continue;
        }
        out.push(CreditsCandidate {
            media_item_id,
            file_path: r.get("file_path"),
            runtime_seconds: r.get("runtime_seconds"),
            is_movie: r.get("is_movie"),
        });
    }
    Ok(out)
}

async fn detect_outros(
    pool: &PgPool,
    library_id: Uuid,
    silence_params: &SilenceParams,
    safety: &SafetyConfig,
) -> Result<usize, sqlx::Error> {
    let items = fetch_items_needing_outros(pool, library_id).await?;
    let mut created = 0usize;

    for item in &items {
        let path = PathBuf::from(&item.file_path);
        if !path.exists() {
            continue;
        }

        record_analysis_file(SegmentAnalysisMethod::Silence);
        let silence = match seg::detect_silence(&path, silence_params).await {
            Ok(events) => events,
            Err(error) => {
                record_analysis_error(SegmentAnalysisErrorStage::Silence);
                tracing::warn!(media_item_id = %item.media_item_id, error = %error, "Outro silence detection failed");
                continue;
            }
        };
        let runtime_ms = item.runtime_seconds.max(0).saturating_mul(1_000);
        if let Some(mut detected) =
            seg::detect_outro_from_silence(item.credits_end_ms, &silence, runtime_ms, item.is_movie)
        {
            seg::mark_surfaced(&mut detected, safety);
            if insert_segment(pool, item.media_item_id, &detected).await? {
                created += 1;
            }
        }
        mark_outro_analyzed(pool, item.credit_segment_id, item.file_hash.as_deref()).await?;
    }

    Ok(created)
}

#[derive(Debug)]
struct OutroCandidate {
    credit_segment_id: Uuid,
    media_item_id: Uuid,
    file_path: String,
    file_hash: Option<String>,
    runtime_seconds: i32,
    credits_end_ms: i32,
    is_movie: bool,
}

async fn fetch_items_needing_outros(
    pool: &PgPool,
    library_id: Uuid,
) -> Result<Vec<OutroCandidate>, sqlx::Error> {
    let rows = sqlx::query(
        r#"
        SELECT
            credits.id AS credit_segment_id,
            mi.id AS media_item_id,
            mf.file_path AS file_path,
            mf.file_hash AS file_hash,
            mf.runtime_seconds AS runtime_seconds,
            credits.end_ms AS credits_end_ms,
            (mi.type = 'movie') AS is_movie
        FROM media_segments credits
        JOIN media_items mi ON mi.id = credits.media_item_id
        JOIN LATERAL (
            SELECT id, file_path, file_hash, runtime_seconds
            FROM media_files
            WHERE media_item_id = mi.id AND is_healthy = true
            ORDER BY file_size DESC, id ASC
            LIMIT 1
        ) mf ON true
        WHERE mi.library_id = $1
          AND mi.type IN ('movie', 'episode')
          AND credits.segment_type = 'credits'
          AND NOT EXISTS (
              SELECT 1 FROM media_segments outro
              WHERE outro.media_item_id = mi.id AND outro.segment_type = 'outro'
          )
          AND (credits.metadata->'outro_analysis'->>'file_hash')
              IS DISTINCT FROM COALESCE(mf.file_hash, '')
        "#,
    )
    .bind(library_id)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .iter()
        .map(|row| OutroCandidate {
            credit_segment_id: row.get("credit_segment_id"),
            media_item_id: row.get("media_item_id"),
            file_path: row.get("file_path"),
            file_hash: row.get("file_hash"),
            runtime_seconds: row.get("runtime_seconds"),
            credits_end_ms: row.get("credits_end_ms"),
            is_movie: row.get("is_movie"),
        })
        .collect())
}

async fn mark_outro_analyzed(
    pool: &PgPool,
    credit_segment_id: Uuid,
    file_hash: Option<&str>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        UPDATE media_segments
        SET metadata = jsonb_set(
                COALESCE(metadata, '{}'::jsonb),
                '{outro_analysis}',
                jsonb_build_object(
                    'algorithm', 'silence_gap_v1',
                    'file_hash', COALESCE($2, ''),
                    'analyzed_at', now()
                ),
                true
            )
        WHERE id = $1
        "#,
    )
    .bind(credit_segment_id)
    .bind(file_hash)
    .execute(pool)
    .await?;
    Ok(())
}

async fn insert_segment(
    pool: &PgPool,
    media_item_id: Uuid,
    detected: &seg::DetectedSegment,
) -> Result<bool, sqlx::Error> {
    let result = sqlx::query(
        r#"
        INSERT INTO media_segments
            (media_item_id, segment_type, start_ms, end_ms, skip_to_ms,
             confidence, source, is_manual, metadata)
        VALUES ($1, $2, $3, $4, $5, $6, $7, false, $8)
        ON CONFLICT DO NOTHING
        "#,
    )
    .bind(media_item_id)
    .bind(detected.segment_type.as_str())
    .bind(detected.start_ms)
    .bind(detected.end_ms)
    .bind(detected.skip_to_ms)
    .bind(detected.confidence)
    .bind(detected.source.as_str())
    .bind(&detected.metadata)
    .execute(pool)
    .await?;
    let inserted = result.rows_affected() > 0;
    if inserted {
        record_segment_created(detected);
    }
    Ok(inserted)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn library_result_message_format() {
        let r = LibraryAnalysisResult {
            candidates: 10,
            chapters_matched: 3,
            segments_created: 5,
            segments_updated: 0,
            fingerprints_computed: 7,
            fingerprints_reused: 0,
            errors: 1,
        };
        let msg = r.message();
        assert!(msg.contains("10 candidate"));
        assert!(msg.contains("3 chapter"));
        assert!(msg.contains("5 segment"));
        assert!(msg.contains("7 fingerprint"));
        assert!(msg.contains("1 error"));
    }

    #[test]
    fn aggregate_add_accumulates() {
        let mut agg = AggregateResult::default();
        let r1 = LibraryAnalysisResult {
            candidates: 5,
            chapters_matched: 2,
            segments_created: 3,
            segments_updated: 0,
            fingerprints_computed: 4,
            fingerprints_reused: 1,
            errors: 0,
        };
        let r2 = LibraryAnalysisResult {
            candidates: 7,
            chapters_matched: 1,
            segments_created: 4,
            segments_updated: 0,
            fingerprints_computed: 6,
            fingerprints_reused: 0,
            errors: 2,
        };
        agg.add(&r1);
        agg.add(&r2);
        assert_eq!(agg.candidates, 12);
        assert_eq!(agg.chapters_matched, 3);
        assert_eq!(agg.segments_created, 7);
        assert_eq!(agg.fingerprints_computed, 10);
        assert_eq!(agg.fingerprints_reused, 1);
        assert_eq!(agg.errors, 2);
    }
}

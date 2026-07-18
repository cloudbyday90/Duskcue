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

use std::collections::HashMap;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Instant, SystemTime};

use chrono::{DateTime, Utc};
use ignore::{WalkBuilder, WalkState};
use regex::Regex;
use serde::{Deserialize, Serialize};
use sqlx::Row;
use thiserror::Error;
use tokio::sync::Semaphore;
use uuid::Uuid;

use crate::services::media_matching;
use crate::services::metadata::EnrichmentOrchestrator;

const MEDIA_VIDEO_EXTENSIONS: &[&str] = &[
    "mkv", "mp4", "avi", "ts", "m2ts", "wmv", "flv", "webm", "mov", "mpg", "mpeg", "m4v", "3gp",
    "ogv", "iso", "img",
];

const MEDIA_SUBTITLE_EXTENSIONS: &[&str] = &["srt", "ass", "ssa", "vtt", "sub", "idx", "sup"];

const MTIME_TOLERANCE_SECS: u64 = 2;

const PARTIAL_HASH_CHUNK: usize = 1024 * 1024;

const DEFAULT_CONCURRENT_PROBES: usize = 2;

#[derive(Debug, Error)]
pub enum ScannerError {
    #[error("Library not found: {0}")]
    LibraryNotFound(Uuid),
    #[error("Library has no scan-enabled paths: {0}")]
    NoScanPaths(Uuid),
    #[error("Scan already in progress for library: {0}")]
    ScanInProgress(Uuid),
    #[error("Path does not exist: {0}")]
    PathNotFound(String),
    #[error("ffprobe failed for {path}: {error}")]
    ProbeFailed { path: String, error: String },
    #[error(transparent)]
    Database(#[from] sqlx::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Clone)]
pub struct DiscoveredFile {
    pub path: PathBuf,
    pub size: u64,
    pub mtime: Option<SystemTime>,
}

#[derive(Debug, Clone)]
pub struct KnownFile {
    pub file_path: String,
    pub file_size: i64,
    pub file_modified_at: Option<DateTime<Utc>>,
    pub file_hash: Option<String>,
    pub media_item_id: Uuid,
}

#[derive(Debug)]
pub struct DiffResult {
    pub new_files: Vec<DiscoveredFile>,
    pub modified_files: Vec<DiscoveredFile>,
    pub unchanged_count: usize,
    pub deleted_paths: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProbeResult {
    pub container_format: String,
    pub video_codec: Option<String>,
    pub video_resolution: Option<String>,
    pub video_bitrate: Option<i32>,
    pub video_dynamic_range: Option<String>,
    pub video_frame_rate: Option<f64>,
    pub audio_codec: Option<String>,
    pub audio_channels: Option<i32>,
    pub audio_language: Option<String>,
    pub audio_bitrate: Option<i32>,
    pub runtime_seconds: i32,
    pub additional_streams: serde_json::Value,
}

#[derive(Debug, Clone)]
pub struct ParsedMediaName {
    pub title: String,
    pub year: Option<u16>,
    pub season: Option<u32>,
    pub episode: Option<u32>,
    pub episode_end: Option<u32>,
    pub episode_title: Option<String>,
    pub resolution: Option<String>,
    pub source: Option<String>,
    pub codec: Option<String>,
    pub group: Option<String>,
    pub edition: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ScanResult {
    pub library_id: Uuid,
    pub scan_type: String,
    pub started_at: DateTime<Utc>,
    pub completed_at: DateTime<Utc>,
    pub duration_ms: u64,
    pub files_discovered: u64,
    pub files_new: u64,
    pub files_modified: u64,
    pub files_unchanged: u64,
    pub files_deleted: u64,
    pub items_created: u64,
    pub items_unmatched: u64,
    pub subtitles_discovered: u64,
    pub errors: Vec<ScanError>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ScanError {
    pub path: String,
    pub phase: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct FfprobeFormat {
    format_name: Option<String>,
    duration: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct FfprobeDisposition {
    forced: Option<i64>,
    hearing_impaired: Option<i64>,
    default: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct FfprobeStream {
    index: Option<i64>,
    codec_type: Option<String>,
    codec_name: Option<String>,
    width: Option<i64>,
    height: Option<i64>,
    bit_rate: Option<String>,
    color_transfer: Option<String>,
    r_frame_rate: Option<String>,
    channels: Option<i32>,
    disposition: Option<FfprobeDisposition>,
    tags: Option<HashMap<String, String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct FfprobeChapter {
    id: Option<i64>,
    start_time: Option<String>,
    end_time: Option<String>,
    tags: Option<HashMap<String, String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct FfprobeOutput {
    format: Option<FfprobeFormat>,
    streams: Option<Vec<FfprobeStream>>,
    chapters: Option<Vec<FfprobeChapter>>,
}

pub async fn scan_library(
    pool: &sqlx::PgPool,
    library_id: Uuid,
    full: bool,
    enrichment: Option<Arc<EnrichmentOrchestrator>>,
) -> Result<ScanResult, ScannerError> {
    let started_at = Utc::now();
    let timer = Instant::now();

    let library = load_library(pool, library_id).await?;
    let paths = load_scan_paths(pool, library_id).await?;

    if paths.is_empty() {
        return Err(ScannerError::NoScanPaths(library_id));
    }

    tracing::info!(
        library_id = %library_id,
        library_name = %library.name,
        media_type = %library.media_type,
        path_count = paths.len(),
        full_scan = full,
        "Starting library scan"
    );

    let mut total_result = ScanResult {
        library_id,
        scan_type: if full { "full" } else { "quick" }.to_string(),
        started_at,
        completed_at: started_at,
        duration_ms: 0,
        files_discovered: 0,
        files_new: 0,
        files_modified: 0,
        files_unchanged: 0,
        files_deleted: 0,
        items_created: 0,
        items_unmatched: 0,
        subtitles_discovered: 0,
        errors: Vec::new(),
    };

    for scan_path in &paths {
        if !scan_path.exists() {
            tracing::warn!(path = %scan_path.display(), "Scan path does not exist, skipping");
            total_result.errors.push(ScanError {
                path: scan_path.to_string_lossy().to_string(),
                phase: "discover".to_string(),
                message: "Path does not exist".to_string(),
            });
            continue;
        }

        match scan_path_pipeline(
            pool,
            library_id,
            &library.media_type,
            scan_path,
            full,
            enrichment.as_deref(),
        )
        .await
        {
            Ok(result) => {
                total_result.files_discovered += result.files_discovered;
                total_result.files_new += result.files_new;
                total_result.files_modified += result.files_modified;
                total_result.files_unchanged += result.files_unchanged;
                total_result.files_deleted += result.files_deleted;
                total_result.items_created += result.items_created;
                total_result.items_unmatched += result.items_unmatched;
                total_result.subtitles_discovered += result.subtitles_discovered;
                total_result.errors.extend(result.errors);
            }
            Err(e) => {
                tracing::error!(path = %scan_path.display(), error = %e, "Scan path failed");
                total_result.errors.push(ScanError {
                    path: scan_path.to_string_lossy().to_string(),
                    phase: "pipeline".to_string(),
                    message: e.to_string(),
                });
            }
        }
    }

    sqlx::query("UPDATE libraries SET last_scan_at = now() WHERE id = $1")
        .bind(library_id)
        .execute(pool)
        .await?;

    let completed_at = Utc::now();
    total_result.completed_at = completed_at;
    total_result.duration_ms = timer.elapsed().as_millis() as u64;

    tracing::info!(
        library_id = %library_id,
        duration_ms = total_result.duration_ms,
        discovered = total_result.files_discovered,
        new = total_result.files_new,
        modified = total_result.files_modified,
        unchanged = total_result.files_unchanged,
        deleted = total_result.files_deleted,
        created = total_result.items_created,
        unmatched = total_result.items_unmatched,
        errors = total_result.errors.len(),
        "Library scan completed"
    );

    Ok(total_result)
}

struct LibraryInfo {
    name: String,
    media_type: String,
}

async fn load_library(pool: &sqlx::PgPool, library_id: Uuid) -> Result<LibraryInfo, ScannerError> {
    let row =
        sqlx::query("SELECT name, media_type FROM libraries WHERE id = $1 AND deleted_at IS NULL")
            .bind(library_id)
            .fetch_optional(pool)
            .await?
            .ok_or(ScannerError::LibraryNotFound(library_id))?;

    Ok(LibraryInfo {
        name: row.get("name"),
        media_type: row.get("media_type"),
    })
}

async fn load_scan_paths(
    pool: &sqlx::PgPool,
    library_id: Uuid,
) -> Result<Vec<PathBuf>, ScannerError> {
    let rows =
        sqlx::query("SELECT path FROM library_paths WHERE library_id = $1 AND scan_enabled = true")
            .bind(library_id)
            .fetch_all(pool)
            .await?;

    Ok(rows
        .iter()
        .map(|r| PathBuf::from(r.get::<String, _>("path")))
        .collect())
}

async fn scan_path_pipeline(
    pool: &sqlx::PgPool,
    library_id: Uuid,
    media_type: &str,
    scan_path: &Path,
    full: bool,
    enrichment: Option<&EnrichmentOrchestrator>,
) -> Result<ScanResult, ScannerError> {
    let started_at = Utc::now();
    let timer = Instant::now();
    let mut errors: Vec<ScanError> = Vec::new();

    let discovered = phase1_discover(scan_path);
    tracing::info!(
        path = %scan_path.display(),
        file_count = discovered.len(),
        "Phase 1 (Discover) complete"
    );

    let diff = phase2_diff(pool, library_id, &discovered, full).await?;
    let new_count = diff.new_files.len();
    let modified_count = diff.modified_files.len();

    tracing::info!(
        path = %scan_path.display(),
        new = new_count,
        modified = modified_count,
        unchanged = diff.unchanged_count,
        deleted = diff.deleted_paths.len(),
        "Phase 2 (Diff) complete"
    );

    let files_to_probe = if full {
        let mut combined = diff.new_files.clone();
        combined.extend(diff.modified_files.iter().cloned());
        combined
    } else {
        diff.new_files.clone()
    };

    let probe_results = phase3_probe(&files_to_probe, &mut errors).await;
    tracing::info!(
        path = %scan_path.display(),
        probed = probe_results.len(),
        "Phase 3 (Probe) complete"
    );

    let items_created = phase4_identify(
        pool,
        library_id,
        media_type,
        scan_path,
        &probe_results,
        &mut errors,
    )
    .await?;
    tracing::info!(
        path = %scan_path.display(),
        items_created,
        "Phase 4 (Identify) complete"
    );

    let subtitles_discovered = match crate::services::subtitle_discovery::discover_subtitles(
        pool,
        library_id,
        &discovered,
    )
    .await
    {
        Ok(n) => n as u64,
        Err(e) => {
            tracing::warn!(error = %e, "Subtitle discovery failed");
            errors.push(ScanError {
                path: scan_path.to_string_lossy().to_string(),
                phase: "subtitle_discovery".to_string(),
                message: e.to_string(),
            });
            0
        }
    };
    tracing::info!(
        path = %scan_path.display(),
        subtitles_discovered,
        "Subtitle discovery complete"
    );

    phase5_enrich(pool, library_id, enrichment, &mut errors).await;

    let deleted_count = phase6_cleanup(pool, library_id, &diff.deleted_paths, &mut errors).await?;
    tracing::info!(
        path = %scan_path.display(),
        deleted = deleted_count,
        "Phase 6 (Cleanup) complete"
    );

    let items_unmatched = count_unmatched(pool, library_id).await?;

    Ok(ScanResult {
        library_id,
        scan_type: if full { "full" } else { "quick" }.to_string(),
        started_at,
        completed_at: Utc::now(),
        duration_ms: timer.elapsed().as_millis() as u64,
        files_discovered: discovered.len() as u64,
        files_new: new_count as u64,
        files_modified: modified_count as u64,
        files_unchanged: diff.unchanged_count as u64,
        files_deleted: deleted_count,
        items_created: items_created as u64,
        items_unmatched: items_unmatched as u64,
        subtitles_discovered,
        errors,
    })
}

fn phase1_discover(root_path: &Path) -> Vec<DiscoveredFile> {
    let mut extensions = MEDIA_VIDEO_EXTENSIONS.to_vec();
    extensions.extend_from_slice(MEDIA_SUBTITLE_EXTENSIONS);

    let mut builder = WalkBuilder::new(root_path);
    builder
        .hidden(false)
        .git_ignore(false)
        .git_exclude(false)
        .git_global(false);

    let extensions_owned = extensions.iter().map(|e| e.to_string()).collect::<Vec<_>>();
    let glob_patterns: Vec<String> = extensions_owned
        .iter()
        .map(|e| format!("*.{}", e))
        .collect();

    let mut override_builder = ignore::overrides::OverrideBuilder::new(root_path);
    for pattern in &glob_patterns {
        override_builder.add(pattern).ok();
    }
    if let Ok(overrides) = override_builder.build() {
        builder.overrides(overrides);
    }

    let discovered: Vec<DiscoveredFile> = Vec::new();
    let discovered = std::sync::Mutex::new(discovered);

    builder.build_parallel().run(|| {
        let discovered = &discovered;
        Box::new(move |entry| {
            let entry = match entry {
                Ok(e) => e,
                Err(_) => return WalkState::Continue,
            };

            if !entry.file_type().is_some_and(|ft| ft.is_file()) {
                return WalkState::Continue;
            }

            let metadata = match entry.metadata() {
                Ok(m) => m,
                Err(_) => return WalkState::Continue,
            };

            let mtime = metadata.modified().ok();

            if let Ok(mut guard) = discovered.lock() {
                guard.push(DiscoveredFile {
                    path: entry.path().to_path_buf(),
                    size: metadata.len(),
                    mtime,
                });
            }

            WalkState::Continue
        })
    });

    discovered.into_inner().unwrap_or_default()
}

async fn phase2_diff(
    pool: &sqlx::PgPool,
    library_id: Uuid,
    discovered: &[DiscoveredFile],
    full: bool,
) -> Result<DiffResult, ScannerError> {
    let known_rows = sqlx::query(
        r#"SELECT mf.file_path, mf.file_size, mf.file_modified_at, mf.file_hash,
                  mf.media_item_id, mf.is_healthy
           FROM media_files mf
           JOIN media_items mi ON mi.id = mf.media_item_id
           WHERE mi.library_id = $1"#,
    )
    .bind(library_id)
    .fetch_all(pool)
    .await?;

    let known: Vec<KnownFile> = known_rows
        .iter()
        .map(|r| KnownFile {
            file_path: r.get("file_path"),
            file_size: r.get("file_size"),
            file_modified_at: r.try_get("file_modified_at").ok(),
            file_hash: r.try_get("file_hash").ok(),
            media_item_id: r.get("media_item_id"),
        })
        .collect();

    let known_map: HashMap<String, &KnownFile> =
        known.iter().map(|k| (k.file_path.clone(), k)).collect();

    let discovered_map: HashMap<String, &DiscoveredFile> = discovered
        .iter()
        .map(|d| (d.path.to_string_lossy().to_string(), d))
        .collect();

    let mut new_files = Vec::new();
    let mut modified_files = Vec::new();
    let mut unchanged_count = 0;

    for disc in discovered {
        let path_str = disc.path.to_string_lossy().to_string();

        if !is_media_video_file(&disc.path) {
            continue;
        }

        match known_map.get(&path_str) {
            None => {
                new_files.push(disc.clone());
            }
            Some(known_file) => {
                if full {
                    modified_files.push(disc.clone());
                    continue;
                }

                let size_changed = disc.size as i64 != known_file.file_size;

                let mtime_changed = match (disc.mtime, known_file.file_modified_at) {
                    (Some(mtime), Some(db_mtime)) => {
                        let mtime_chrono = DateTime::<Utc>::from(mtime);
                        let diff = (mtime_chrono - db_mtime).num_seconds().abs();
                        diff > MTIME_TOLERANCE_SECS as i64
                    }
                    _ => true,
                };

                if size_changed || mtime_changed {
                    modified_files.push(disc.clone());
                } else {
                    unchanged_count += 1;
                }
            }
        }
    }

    let deleted_paths: Vec<String> = known_map
        .keys()
        .filter(|path| !discovered_map.contains_key(*path))
        .cloned()
        .collect();

    Ok(DiffResult {
        new_files,
        modified_files,
        unchanged_count,
        deleted_paths,
    })
}

async fn phase3_probe(
    files: &[DiscoveredFile],
    errors: &mut Vec<ScanError>,
) -> Vec<(DiscoveredFile, ProbeResult)> {
    let semaphore = Arc::new(Semaphore::new(DEFAULT_CONCURRENT_PROBES));
    let mut handles = Vec::new();

    for file in files {
        if !is_media_video_file(&file.path) {
            continue;
        }

        let permit = semaphore.clone().acquire_owned().await.unwrap();
        let file = file.clone();

        handles.push(tokio::spawn(async move {
            let result = probe_file(&file.path).await;
            drop(permit);
            (file, result)
        }));
    }

    let mut results = Vec::new();
    for handle in handles {
        match handle.await {
            Ok((file, Ok(probe))) => {
                results.push((file, probe));
            }
            Ok((file, Err(e))) => {
                tracing::warn!(path = %file.path.display(), error = %e, "Probe failed");
                errors.push(ScanError {
                    path: file.path.to_string_lossy().to_string(),
                    phase: "probe".to_string(),
                    message: e.to_string(),
                });
            }
            Err(e) => {
                tracing::error!(error = %e, "Probe task panicked");
            }
        }
    }

    results
}

async fn probe_file(path: &Path) -> Result<ProbeResult, ScannerError> {
    let output = tokio::process::Command::new("ffprobe")
        .args([
            "-v",
            "quiet",
            "-print_format",
            "json",
            "-show_format",
            "-show_streams",
            "-show_chapters",
        ])
        .arg(path)
        .output()
        .await
        .map_err(|e| ScannerError::ProbeFailed {
            path: path.to_string_lossy().to_string(),
            error: e.to_string(),
        })?;

    if !output.status.success() {
        return Err(ScannerError::ProbeFailed {
            path: path.to_string_lossy().to_string(),
            error: String::from_utf8_lossy(&output.stderr).to_string(),
        });
    }

    let probe: FfprobeOutput =
        serde_json::from_slice(&output.stdout).map_err(|e| ScannerError::ProbeFailed {
            path: path.to_string_lossy().to_string(),
            error: format!("JSON parse error: {}", e),
        })?;

    let streams = probe.streams.unwrap_or_default();
    let format = probe.format;

    let mut video_codec = None;
    let mut video_resolution = None;
    let mut video_bitrate = None;
    let mut video_dynamic_range = None;
    let mut video_frame_rate = None;
    let mut audio_codec = None;
    let mut audio_channels = None;
    let mut audio_language = None;
    let mut audio_bitrate = None;
    let mut additional_streams = serde_json::json!({});
    let mut audio_streams: Vec<serde_json::Value> = Vec::new();
    let mut subtitle_streams: Vec<serde_json::Value> = Vec::new();

    for stream in &streams {
        match stream.codec_type.as_deref() {
            Some("video") => {
                video_codec = stream.codec_name.clone();
                if let (Some(w), Some(h)) = (stream.width, stream.height) {
                    video_resolution = Some(format!("{}x{}", w, h));
                }
                video_bitrate = stream.bit_rate.as_ref().and_then(|b| b.parse::<i32>().ok());
                video_dynamic_range = stream.color_transfer.as_ref().map(|ct| match ct.as_str() {
                    "smpte2084" => "hdr10".to_string(),
                    "arib-std-b67" => "hlg".to_string(),
                    _ => "sdr".to_string(),
                });
                video_frame_rate = parse_frame_rate(stream.r_frame_rate.as_deref());
            }
            Some("audio") => {
                let language = stream
                    .tags
                    .as_ref()
                    .and_then(|t| t.get("language").cloned())
                    .unwrap_or_else(|| "und".to_string());
                let title = stream.tags.as_ref().and_then(|t| t.get("title").cloned());
                let bitrate = stream.bit_rate.as_ref().and_then(|b| b.parse::<i32>().ok());
                if audio_codec.is_none() {
                    audio_codec = stream.codec_name.clone();
                    audio_channels = stream.channels;
                    audio_language = Some(language.clone());
                    audio_bitrate = bitrate;
                }
                audio_streams.push(serde_json::json!({
                    "index": stream.index.unwrap_or(0),
                    "codec": stream.codec_name,
                    "channels": stream.channels,
                    "language": language,
                    "title": title,
                    "bitrate": bitrate,
                }));
            }
            Some("subtitle") => {
                let lang = stream
                    .tags
                    .as_ref()
                    .and_then(|t| t.get("language").cloned())
                    .unwrap_or_else(|| "und".to_string());
                let title = stream.tags.as_ref().and_then(|t| t.get("title").cloned());
                let title_lower = title.as_ref().map(|t| t.to_lowercase()).unwrap_or_default();
                let disp_forced = stream
                    .disposition
                    .as_ref()
                    .and_then(|d| d.forced)
                    .map(|f| f == 1)
                    .unwrap_or(false);
                let disp_hi = stream
                    .disposition
                    .as_ref()
                    .and_then(|d| d.hearing_impaired)
                    .map(|f| f == 1)
                    .unwrap_or(false);
                let is_forced = disp_forced || title_lower.contains("forced");
                let is_hearing_impaired = disp_hi
                    || title_lower.contains("hearing impaired")
                    || title_lower.contains("sdh")
                    || title_lower.contains("cc");

                subtitle_streams.push(serde_json::json!({
                    "index": stream.index.unwrap_or(0),
                    "codec": stream.codec_name,
                    "language": lang,
                    "title": title,
                    "is_forced": is_forced,
                    "is_hearing_impaired": is_hearing_impaired,
                }));
            }
            _ => {}
        }
    }

    let runtime_seconds = format
        .as_ref()
        .and_then(|f| f.duration.as_ref())
        .and_then(|d| d.parse::<f64>().ok())
        .map(|d| d.round() as i32)
        .unwrap_or(0);

    let container_format = format
        .as_ref()
        .and_then(|f| f.format_name.clone())
        .unwrap_or_else(|| "unknown".to_string());

    let chapters: Vec<serde_json::Value> = probe
        .chapters
        .unwrap_or_default()
        .iter()
        .map(|ch| {
            serde_json::json!({
                "id": ch.id,
                "start_time": ch.start_time,
                "end_time": ch.end_time,
                "tags": ch.tags,
            })
        })
        .collect();

    if !chapters.is_empty() {
        additional_streams
            .as_object_mut()
            .unwrap_or(&mut serde_json::Map::new())
            .insert("chapters".to_string(), serde_json::json!(chapters));
    }

    if !audio_streams.is_empty() {
        additional_streams
            .as_object_mut()
            .unwrap_or(&mut serde_json::Map::new())
            .insert("audio".to_string(), serde_json::json!(audio_streams));
    }

    if !subtitle_streams.is_empty() {
        additional_streams
            .as_object_mut()
            .unwrap_or(&mut serde_json::Map::new())
            .insert("subtitles".to_string(), serde_json::json!(subtitle_streams));
    }

    Ok(ProbeResult {
        container_format,
        video_codec,
        video_resolution,
        video_bitrate,
        video_dynamic_range,
        video_frame_rate,
        audio_codec,
        audio_channels,
        audio_language,
        audio_bitrate,
        runtime_seconds: runtime_seconds.max(0),
        additional_streams,
    })
}

fn parse_frame_rate(rate: Option<&str>) -> Option<f64> {
    let rate_str = rate?;
    let parts: Vec<&str> = rate_str.split('/').collect();
    match parts.len() {
        1 => parts[0].parse::<f64>().ok(),
        2 => {
            let num: f64 = parts[0].parse().ok()?;
            let den: f64 = parts[1].parse().ok()?;
            if den == 0.0 { None } else { Some(num / den) }
        }
        _ => None,
    }
}

async fn phase4_identify(
    pool: &sqlx::PgPool,
    library_id: Uuid,
    media_type: &str,
    scan_path: &Path,
    probed: &[(DiscoveredFile, ProbeResult)],
    errors: &mut Vec<ScanError>,
) -> Result<usize, ScannerError> {
    let mut items_created = 0;

    match media_type {
        "movies" => {
            for (file, probe) in probed {
                match identify_and_create_movie(pool, library_id, scan_path, file, probe).await {
                    Ok(()) => items_created += 1,
                    Err(e) => {
                        tracing::warn!(
                            path = %file.path.display(),
                            error = %e,
                            "Failed to identify movie"
                        );
                        errors.push(ScanError {
                            path: file.path.to_string_lossy().to_string(),
                            phase: "identify".to_string(),
                            message: e.to_string(),
                        });
                    }
                }
            }
        }
        "tvshows" => {
            let grouped = group_episodes_by_series(scan_path, probed);

            for (series_key, episodes) in grouped.values() {
                match identify_and_create_series(
                    pool, library_id, scan_path, series_key, episodes, errors,
                )
                .await
                {
                    Ok(count) => items_created += count,
                    Err(e) => {
                        tracing::warn!(
                            series = %series_key.title,
                            error = %e,
                            "Failed to identify series"
                        );
                        errors.push(ScanError {
                            path: series_key.title.clone(),
                            phase: "identify".to_string(),
                            message: e.to_string(),
                        });
                    }
                }
            }
        }
        _ => {}
    }

    Ok(items_created)
}

async fn identify_and_create_movie(
    pool: &sqlx::PgPool,
    library_id: Uuid,
    scan_path: &Path,
    file: &DiscoveredFile,
    probe: &ProbeResult,
) -> Result<(), ScannerError> {
    let existing = sqlx::query("SELECT id FROM media_files WHERE file_path = $1")
        .bind(file.path.to_string_lossy().to_string())
        .fetch_optional(pool)
        .await?;

    if existing.is_some() {
        return update_media_file(pool, file, probe).await;
    }

    let parent = file.path.parent().unwrap_or(scan_path);
    let file_stem = file.path.file_stem().and_then(|s| s.to_str());
    let ident = media_matching::resolve_identification(parent, None, file_stem);

    let parsed = parse_media_name(&file.path, parent, "movies");
    let title = parsed
        .as_ref()
        .map(|p| p.title.clone())
        .or(ident.title.clone())
        .unwrap_or_else(|| {
            file.path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("Unknown")
                .to_string()
        });

    let year = parsed.as_ref().and_then(|p| p.year).or(ident.year);
    let sort_title = generate_sort_title(&title);
    let file_hash = compute_partial_hash_sync(&file.path);

    let mut tx = pool.begin().await?;

    let media_item_row = sqlx::query(
        r#"INSERT INTO media_items (library_id, type, title, sort_title, premiere_date,
                      match_state, identification_source, tmdb_id, imdb_id, tvdb_id)
           VALUES ($1, 'movie', $2, $3,
                   CASE WHEN $4::int IS NOT NULL THEN
                       to_timestamp($4::int)::date ELSE NULL END,
                   $5, $6, $7, $8, $9)
           RETURNING id"#,
    )
    .bind(library_id)
    .bind(&title)
    .bind(&sort_title)
    .bind(year.map(|y| y as i32))
    .bind(&ident.match_state)
    .bind(&ident.identification_source)
    .bind(ident.ids.tmdb_id)
    .bind(ident.ids.imdb_id.clone())
    .bind(ident.ids.tvdb_id)
    .fetch_one(&mut *tx)
    .await?;

    let media_item_id: Uuid = media_item_row.get("id");

    sqlx::query("INSERT INTO movies (id) VALUES ($1)")
        .bind(media_item_id)
        .execute(&mut *tx)
        .await?;

    insert_media_file(&mut tx, &media_item_id, file, probe, file_hash.as_deref()).await?;

    tx.commit().await?;

    tracing::debug!(title = %title, path = %file.path.display(), "Created movie item");
    Ok(())
}

#[derive(Debug)]
struct SeriesKey {
    title: String,
    year: Option<u16>,
    folder: PathBuf,
}

#[allow(dead_code)]
struct EpisodeInfo {
    file: DiscoveredFile,
    probe: ProbeResult,
    season: u32,
    episode: u32,
    episode_end: Option<u32>,
}

fn group_episodes_by_series(
    scan_path: &Path,
    probed: &[(DiscoveredFile, ProbeResult)],
) -> HashMap<String, (SeriesKey, Vec<EpisodeInfo>)> {
    let mut groups: HashMap<String, (SeriesKey, Vec<EpisodeInfo>)> = HashMap::new();
    let mut media_match_cache: HashMap<PathBuf, Option<media_matching::MediaMatchData>> =
        HashMap::new();

    for (file, probe) in probed {
        let parent = file.path.parent().unwrap_or(scan_path);

        let series_folder = find_series_folder(parent, scan_path);

        let cached = media_match_cache
            .entry(series_folder.clone())
            .or_insert_with(|| {
                media_matching::parse_media_match_file(&series_folder.join(".media-match"))
            });

        let series_name = series_folder
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("Unknown");

        let filename = file.path.file_name().and_then(|n| n.to_str()).unwrap_or("");

        let (season, episode, episode_end) = if let Some(mm) = cached {
            if let Some(ov_result) = media_matching::resolve_episode_override(
                filename,
                &mm.episode_overrides,
                mm.pattern.as_deref(),
                mm.season.unwrap_or(1),
            ) {
                (Some(ov_result.0), Some(ov_result.1), ov_result.2)
            } else {
                let parsed = parse_media_name(&file.path, &series_folder, "tvshows");
                match parsed {
                    Some(ref p) => (p.season, p.episode, p.episode_end),
                    None => {
                        let parsed_ep = parse_sxxexx_filename(&file.path);
                        match parsed_ep {
                            Some((s, e, ee)) => (Some(s), Some(e), ee),
                            None => continue,
                        }
                    }
                }
            }
        } else {
            let parsed = parse_media_name(&file.path, &series_folder, "tvshows");
            match parsed {
                Some(ref p) => (p.season, p.episode, p.episode_end),
                None => {
                    let parsed_ep = parse_sxxexx_filename(&file.path);
                    match parsed_ep {
                        Some((s, e, ee)) => (Some(s), Some(e), ee),
                        None => continue,
                    }
                }
            }
        };

        let (season_num, episode_num) = match (season, episode) {
            (Some(s), Some(e)) => (s, e),
            _ => continue,
        };

        let title = {
            let parsed = parse_media_name(&file.path, &series_folder, "tvshows");
            parsed
                .as_ref()
                .map(|p| p.title.clone())
                .unwrap_or_else(|| clean_series_name(series_name))
        };

        let key = format!("{}-{}", title, series_folder.to_string_lossy());

        let year = {
            let parsed = parse_media_name(&file.path, &series_folder, "tvshows");
            parsed
                .as_ref()
                .and_then(|p| p.year)
                .or_else(|| parse_year_from_name(series_name))
        };

        groups
            .entry(key)
            .or_insert_with(|| {
                (
                    SeriesKey {
                        title: title.clone(),
                        year,
                        folder: series_folder.to_path_buf(),
                    },
                    Vec::new(),
                )
            })
            .1
            .push(EpisodeInfo {
                file: file.clone(),
                probe: probe.clone(),
                season: season_num,
                episode: episode_num,
                episode_end,
            });
    }

    groups
}

async fn identify_and_create_series(
    pool: &sqlx::PgPool,
    library_id: Uuid,
    _scan_path: &Path,
    series_key: &SeriesKey,
    episodes: &[EpisodeInfo],
    _errors: &mut Vec<ScanError>,
) -> Result<usize, ScannerError> {
    let mut items_created = 0;

    let ident = media_matching::resolve_identification(&series_key.folder, None, None);

    let title = series_key.title.clone();
    let sort_title = generate_sort_title(&title);
    let year = series_key.year;

    let series_item_id = match find_existing_series(pool, library_id, &title).await? {
        Some(id) => id,
        None => {
            let mut tx = pool.begin().await?;

            let row = sqlx::query(
                r#"INSERT INTO media_items (library_id, type, title, sort_title, premiere_date,
                              match_state, identification_source, tmdb_id, imdb_id, tvdb_id)
                   VALUES ($1, 'series', $2, $3,
                           CASE WHEN $4::int IS NOT NULL THEN
                               to_timestamp($4::int)::date ELSE NULL END,
                           $5, $6, $7, $8, $9)
                   RETURNING id"#,
            )
            .bind(library_id)
            .bind(&title)
            .bind(&sort_title)
            .bind(year.map(|y| y as i32))
            .bind(&ident.match_state)
            .bind(&ident.identification_source)
            .bind(ident.ids.tmdb_id)
            .bind(ident.ids.imdb_id.clone())
            .bind(ident.ids.tvdb_id)
            .fetch_one(&mut *tx)
            .await?;

            let series_id: Uuid = row.get("id");

            sqlx::query("INSERT INTO series (id, status) VALUES ($1, 'continuing')")
                .bind(series_id)
                .execute(&mut *tx)
                .await?;

            tx.commit().await?;
            items_created += 1;

            tracing::debug!(title = %title, "Created series item");
            series_id
        }
    };

    let mut season_cache: HashMap<u32, Uuid> = HashMap::new();

    for ep_info in episodes {
        let season_id = match season_cache.get(&ep_info.season) {
            Some(id) => *id,
            None => {
                let id = ensure_season(pool, series_item_id, ep_info.season, library_id).await?;
                items_created += 1;
                season_cache.insert(ep_info.season, id);
                id
            }
        };

        let existing = sqlx::query("SELECT id FROM media_files WHERE file_path = $1")
            .bind(ep_info.file.path.to_string_lossy().to_string())
            .fetch_optional(pool)
            .await?;

        if existing.is_some() {
            update_media_file(pool, &ep_info.file, &ep_info.probe).await?;
            continue;
        }

        let file_hash = compute_partial_hash_sync(&ep_info.file.path);

        let episode_title = parse_episode_title(&ep_info.file.path);

        let mut tx = pool.begin().await?;

        let episode_row = sqlx::query(
            r#"INSERT INTO media_items (library_id, type, title, sort_title,
                          runtime_seconds, match_state, identification_source)
               VALUES ($1, 'episode', $2, $3, $4, 'auto_matched', 'filename_parse')
               RETURNING id"#,
        )
        .bind(library_id)
        .bind(
            episode_title
                .as_deref()
                .unwrap_or(&format!("S{:02}E{:02}", ep_info.season, ep_info.episode)),
        )
        .bind(
            episode_title
                .as_deref()
                .unwrap_or(&format!("S{:02}E{:02}", ep_info.season, ep_info.episode)),
        )
        .bind(ep_info.probe.runtime_seconds)
        .fetch_one(&mut *tx)
        .await?;

        let episode_id: Uuid = episode_row.get("id");

        sqlx::query(
            r#"INSERT INTO episodes (id, series_id, season_id, episode_number)
               VALUES ($1, $2, $3, $4)"#,
        )
        .bind(episode_id)
        .bind(series_item_id)
        .bind(season_id)
        .bind(ep_info.episode as i32)
        .execute(&mut *tx)
        .await?;

        insert_media_file(
            &mut tx,
            &episode_id,
            &ep_info.file,
            &ep_info.probe,
            file_hash.as_deref(),
        )
        .await?;

        tx.commit().await?;
        items_created += 1;
    }

    Ok(items_created)
}

async fn find_existing_series(
    pool: &sqlx::PgPool,
    library_id: Uuid,
    title: &str,
) -> Result<Option<Uuid>, ScannerError> {
    let row = sqlx::query(
        r#"SELECT mi.id FROM media_items mi
           JOIN series s ON s.id = mi.id
           WHERE mi.library_id = $1 AND mi.title = $2 AND mi.type = 'series'"#,
    )
    .bind(library_id)
    .bind(title)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|r| r.get("id")))
}

async fn ensure_season(
    pool: &sqlx::PgPool,
    series_id: Uuid,
    season_number: u32,
    library_id: Uuid,
) -> Result<Uuid, ScannerError> {
    let existing =
        sqlx::query("SELECT id FROM seasons WHERE series_id = $1 AND season_number = $2")
            .bind(series_id)
            .bind(season_number as i32)
            .fetch_optional(pool)
            .await?;

    if let Some(row) = existing {
        return Ok(row.get("id"));
    }

    let mut tx = pool.begin().await?;

    let season_title = format!("Season {:02}", season_number);

    let row = sqlx::query(
        r#"INSERT INTO media_items (library_id, type, title, sort_title, match_state)
           VALUES ($1, 'season', $2, $3, 'confirmed')
           RETURNING id"#,
    )
    .bind(library_id)
    .bind(&season_title)
    .bind(&season_title)
    .fetch_one(&mut *tx)
    .await?;

    let season_id: Uuid = row.get("id");

    sqlx::query("INSERT INTO seasons (id, series_id, season_number) VALUES ($1, $2, $3)")
        .bind(season_id)
        .bind(series_id)
        .bind(season_number as i32)
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;

    tracing::debug!(
        series_id = %series_id,
        season = season_number,
        "Created season"
    );

    Ok(season_id)
}

async fn insert_media_file(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    media_item_id: &Uuid,
    file: &DiscoveredFile,
    probe: &ProbeResult,
    file_hash: Option<&str>,
) -> Result<(), ScannerError> {
    let mtime = file.mtime.map(DateTime::<Utc>::from);

    sqlx::query(
        r#"INSERT INTO media_files (media_item_id, file_path, file_size, file_hash,
                      file_modified_at, container_format, video_codec, video_resolution,
                      video_bitrate, video_dynamic_range, video_frame_rate,
                      audio_codec, audio_channels, audio_language, audio_bitrate,
                      runtime_seconds, additional_streams)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17)"#,
    )
    .bind(media_item_id)
    .bind(file.path.to_string_lossy().to_string())
    .bind(file.size as i64)
    .bind(file_hash)
    .bind(mtime)
    .bind(&probe.container_format)
    .bind(&probe.video_codec)
    .bind(&probe.video_resolution)
    .bind(probe.video_bitrate)
    .bind(&probe.video_dynamic_range)
    .bind(probe.video_frame_rate.map(|f| format!("{:.3}", f)))
    .bind(&probe.audio_codec)
    .bind(probe.audio_channels)
    .bind(&probe.audio_language)
    .bind(probe.audio_bitrate)
    .bind(probe.runtime_seconds)
    .bind(&probe.additional_streams)
    .execute(&mut **tx)
    .await?;

    Ok(())
}

async fn update_media_file(
    pool: &sqlx::PgPool,
    file: &DiscoveredFile,
    probe: &ProbeResult,
) -> Result<(), ScannerError> {
    let file_hash = compute_partial_hash_sync(&file.path);
    let mtime = file.mtime.map(DateTime::<Utc>::from);

    sqlx::query(
        r#"UPDATE media_files SET
            file_size = $2, file_hash = COALESCE($3, file_hash),
            file_modified_at = COALESCE($4, file_modified_at),
            container_format = $5, video_codec = $6, video_resolution = $7,
            video_bitrate = $8, video_dynamic_range = $9, video_frame_rate = $10,
            audio_codec = $11, audio_channels = $12, audio_language = $13,
            audio_bitrate = $14, runtime_seconds = $15,
            additional_streams = $16, is_healthy = true,
            last_scanned_at = now(), updated_at = now()
           WHERE file_path = $1"#,
    )
    .bind(file.path.to_string_lossy().to_string())
    .bind(file.size as i64)
    .bind(file_hash)
    .bind(mtime)
    .bind(&probe.container_format)
    .bind(&probe.video_codec)
    .bind(&probe.video_resolution)
    .bind(probe.video_bitrate)
    .bind(&probe.video_dynamic_range)
    .bind(probe.video_frame_rate.map(|f| format!("{:.3}", f)))
    .bind(&probe.audio_codec)
    .bind(probe.audio_channels)
    .bind(&probe.audio_language)
    .bind(probe.audio_bitrate)
    .bind(probe.runtime_seconds)
    .bind(&probe.additional_streams)
    .execute(pool)
    .await?;

    Ok(())
}

fn parse_media_name(file_path: &Path, parent: &Path, media_type: &str) -> Option<ParsedMediaName> {
    let filename = file_path.file_stem()?.to_str()?;

    let folder_name = parent.file_name().and_then(|n| n.to_str()).unwrap_or("");

    let name_source = folder_name;

    let year = parse_year_from_name(name_source);
    let title = if year.is_some() {
        let re = Regex::new(r"\s*\(\d{4}\)\s*").ok()?;
        let clean = re.replace(name_source, "");
        clean.trim().to_string()
    } else {
        clean_series_name(name_source)
    };

    let (season, episode, episode_end) = parse_sxxexx_filename(file_path)
        .map(|(s, e, ee)| (Some(s), Some(e), ee))
        .unwrap_or((None, None, None));

    let resolution = parse_resolution_tag(filename);
    let source = parse_source_tag(filename);
    let codec = parse_codec_tag(filename);
    let group = parse_group_tag(filename);
    let edition = parse_edition_tag(filename);
    let episode_title = if media_type == "tvshows" {
        parse_episode_title(file_path)
    } else {
        None
    };

    Some(ParsedMediaName {
        title,
        year,
        season,
        episode,
        episode_end,
        episode_title,
        resolution,
        source,
        codec,
        group,
        edition,
    })
}

fn parse_sxxexx_filename(path: &Path) -> Option<(u32, u32, Option<u32>)> {
    let filename = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

    let re = Regex::new(r"(?i)[_.\s\-]s?(\d{1,2})[ex](\d{1,3})(?:\s*[-e]\s*(\d{1,3}))?").unwrap();

    if let Some(caps) = re.captures(filename) {
        let season: u32 = caps.get(1)?.as_str().parse().ok()?;
        let episode: u32 = caps.get(2)?.as_str().parse().ok()?;
        let episode_end: Option<u32> = caps.get(3).and_then(|m| m.as_str().parse().ok());
        return Some((season, episode, episode_end));
    }

    let alt_re = Regex::new(r"(?i)[_.\s\-](\d{1,2})x(\d{1,3})").unwrap();
    if let Some(caps) = alt_re.captures(filename) {
        let season: u32 = caps.get(1)?.as_str().parse().ok()?;
        let episode: u32 = caps.get(2)?.as_str().parse().ok()?;
        return Some((season, episode, None));
    }

    None
}

fn parse_year_from_name(name: &str) -> Option<u16> {
    let re = Regex::new(r"\((\d{4})\)").ok()?;
    if let Some(caps) = re.captures(name) {
        return caps.get(1)?.as_str().parse().ok();
    }

    let year_standalone = Regex::new(r"(?:^|[.\-_ ])(\d{4})(?:[.\-_ ]|$)").ok()?;
    if let Some(caps) = year_standalone.captures(name) {
        let year: u16 = caps.get(1)?.as_str().parse().ok()?;
        if (1900..=2100).contains(&year) {
            return Some(year);
        }
    }

    None
}

fn parse_resolution_tag(filename: &str) -> Option<String> {
    let lower = filename.to_lowercase();
    if lower.contains("2160p") || lower.contains("4k") || lower.contains("uhd") {
        return Some("2160p".to_string());
    }
    if lower.contains("1080p") || lower.contains("fhd") {
        return Some("1080p".to_string());
    }
    if lower.contains("720p") || lower.contains("hd") {
        return Some("720p".to_string());
    }
    if lower.contains("480p") || lower.contains("sd") {
        return Some("480p".to_string());
    }
    None
}

fn parse_source_tag(filename: &str) -> Option<String> {
    let lower = filename.to_lowercase();
    if lower.contains("bluray") || lower.contains("blu-ray") || lower.contains("bdrip") {
        return Some("bluray".to_string());
    }
    if lower.contains("web-dl") || lower.contains("webdl") {
        return Some("web-dl".to_string());
    }
    if lower.contains("webrip") {
        return Some("webrip".to_string());
    }
    if lower.contains("hdtv") {
        return Some("hdtv".to_string());
    }
    if lower.contains("dvd") {
        return Some("dvd".to_string());
    }
    if lower.contains("remux") {
        return Some("remux".to_string());
    }
    None
}

fn parse_codec_tag(filename: &str) -> Option<String> {
    let lower = filename.to_lowercase();
    if lower.contains("x265") || lower.contains("h265") || lower.contains("hevc") {
        return Some("hevc".to_string());
    }
    if lower.contains("x264") || lower.contains("h264") || lower.contains("avc") {
        return Some("avc".to_string());
    }
    if lower.contains("av1") {
        return Some("av1".to_string());
    }
    if lower.contains("xvid") {
        return Some("xvid".to_string());
    }
    None
}

fn parse_group_tag(filename: &str) -> Option<String> {
    let re = Regex::new(r"-(\w+)$").ok()?;
    let caps = re.captures(filename)?;
    Some(caps[1].to_string())
}

fn parse_edition_tag(filename: &str) -> Option<String> {
    let lower = filename.to_lowercase();
    let editions = [
        "directors cut",
        "director's cut",
        "extended",
        "theatrical",
        "unrated",
        "remastered",
        "final cut",
        "ultimate",
        "special edition",
    ];

    for edition in editions {
        if lower.contains(edition) {
            return Some(edition.to_string());
        }
    }

    let re = Regex::new(r"[-.]([-\s\w]+?)(?:\.\w+)?$").ok()?;
    if let Some(caps) = re.captures(filename) {
        let candidate = caps[1].trim().to_lowercase();
        if editions.iter().any(|e| candidate.contains(e)) {
            return Some(caps[1].trim().to_string());
        }
    }

    None
}

fn parse_episode_title(path: &Path) -> Option<String> {
    let filename = path.file_stem()?.to_str()?;
    let re = Regex::new(r"(?i)S\d{1,2}E\d{1,3}\s*[-. ]\s*(.+)$").ok()?;
    let caps = re.captures(filename)?;
    let title = caps[1].trim().to_string();

    if title.is_empty() {
        return None;
    }

    let clean_re = Regex::new(r"\s*[-. ]\s*(?:1080p|720p|2160p|480p|4k|bluray|web-?dl|webrip|hdtv|x264|x265|hevc|avc|av1|dvd|remux).*").ok()?;
    Some(clean_re.replace(&title, "").trim().to_string())
}

fn find_series_folder(current: &Path, scan_root: &Path) -> PathBuf {
    if current == scan_root {
        return current.to_path_buf();
    }

    let season_re = Regex::new(r"(?i)^season\s*\d+$").unwrap();
    let specials_re = Regex::new(r"(?i)^specials$").unwrap();

    let folder_name = current.file_name().and_then(|n| n.to_str()).unwrap_or("");

    if (season_re.is_match(folder_name) || specials_re.is_match(folder_name))
        && let Some(parent) = current.parent()
    {
        return parent.to_path_buf();
    }

    current.to_path_buf()
}

fn clean_series_name(name: &str) -> String {
    let re = Regex::new(r"\s*\{[^}]*\}\s*").unwrap();
    let re2 = Regex::new(r"\s*\[[^\]]*\]\s*").unwrap();
    let cleaned = re.replace(name, "");
    let cleaned = re2.replace(&cleaned, "");

    let year_re = Regex::new(r"\s*\(\d{4}\)\s*").unwrap();
    let cleaned = year_re.replace(&cleaned, "");

    cleaned
        .trim()
        .replace(['.', '_'], " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn generate_sort_title(title: &str) -> String {
    let lower = title.to_lowercase();

    let articles = ["the ", "a ", "an "];
    for article in articles {
        if lower.starts_with(article) {
            let remainder = &title[article.len()..];
            return format!("{}, {}", remainder, &title[..article.len() - 1]);
        }
    }

    title.to_string()
}

async fn phase5_enrich(
    pool: &sqlx::PgPool,
    library_id: Uuid,
    enrichment: Option<&EnrichmentOrchestrator>,
    errors: &mut Vec<ScanError>,
) {
    let Some(orchestrator) = enrichment else {
        tracing::info!(
            library_id = %library_id,
            "Phase 5 (Enrich) — skipped, no enrichment orchestrator configured"
        );
        return;
    };

    let mut persist_errors: Vec<String> = Vec::new();
    crate::services::enrichment_persistence::enrich_items_for_library(
        pool,
        orchestrator,
        library_id,
        &mut persist_errors,
    )
    .await;

    for err in persist_errors {
        errors.push(ScanError {
            path: format!("library:{library_id}"),
            phase: "enrich".to_string(),
            message: err,
        });
    }
}

async fn phase6_cleanup(
    pool: &sqlx::PgPool,
    library_id: Uuid,
    deleted_paths: &[String],
    _errors: &mut Vec<ScanError>,
) -> Result<u64, ScannerError> {
    let mut deleted_count = 0u64;

    for path in deleted_paths {
        sqlx::query(
            "UPDATE media_files SET is_healthy = false, updated_at = now() WHERE file_path = $1",
        )
        .bind(path)
        .execute(pool)
        .await?;

        deleted_count += 1;
    }

    let orphans: Vec<Uuid> = sqlx::query(
        r#"SELECT mi.id FROM media_items mi
           LEFT JOIN media_files mf ON mf.media_item_id = mi.id AND mf.is_healthy = true
           WHERE mi.library_id = $1
           AND mi.type IN ('movie', 'episode')
           AND mf.id IS NULL"#,
    )
    .bind(library_id)
    .fetch_all(pool)
    .await?
    .iter()
    .map(|r| r.get("id"))
    .collect();

    if !orphans.is_empty() {
        tracing::info!(
            library_id = %library_id,
            orphan_count = orphans.len(),
            "Detected orphaned media items (no healthy files)"
        );
    }

    Ok(deleted_count)
}

async fn count_unmatched(pool: &sqlx::PgPool, library_id: Uuid) -> Result<usize, ScannerError> {
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM media_items WHERE library_id = $1 AND match_state = 'unmatched'",
    )
    .bind(library_id)
    .fetch_one(pool)
    .await?;

    Ok(count as usize)
}

fn compute_partial_hash_sync(path: &Path) -> Option<String> {
    let mut file = std::fs::File::open(path).ok()?;
    let file_size = file.metadata().ok()?.len();

    let mut hasher = blake3::Hasher::new();
    let mut buf = vec![0u8; PARTIAL_HASH_CHUNK];

    let n = file.read(&mut buf).ok()?;
    hasher.update(&buf[..n]);

    if file_size > (PARTIAL_HASH_CHUNK as u64) * 2 {
        file.seek(SeekFrom::End(-(PARTIAL_HASH_CHUNK as i64)))
            .ok()?;
        let n = file.read(&mut buf).ok()?;
        hasher.update(&buf[..n]);
    }

    Some(hasher.finalize().to_hex().to_string())
}

fn is_media_video_file(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| MEDIA_VIDEO_EXTENSIONS.contains(&e.to_lowercase().as_str()))
        .unwrap_or(false)
}

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

#![allow(unused_variables)]

use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};

use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::domains::subtitles::error::SubtitleError;
use crate::domains::subtitles::types::*;
use crate::services::subdl_client::{SubdlClient, SubtitleSearchResult};
use crate::services::subtitles as sub_svc;
use crate::services::opensubtitles_client::OpensubtitlesClient;
use crate::state::AppState;

const LIST_SUBTITLES_SQL: &str = r#"SELECT id, media_item_id, file_path, language, subtitle_type,
         is_forced, is_hearing_impaired, source_provider
  FROM subtitle_files
  WHERE media_item_id = $1
  ORDER BY
    CASE subtitle_type
      WHEN 'external' THEN 0
      WHEN 'fetched' THEN 1
      WHEN 'embedded' THEN 2
    END,
    is_forced DESC,
    language ASC"#;

const GET_SUBTITLE_SQL: &str = r#"SELECT id, media_item_id, file_path, language, subtitle_type,
         is_forced, is_hearing_impaired, source_provider
  FROM subtitle_files
  WHERE id = $1 AND media_item_id = $2"#;

pub async fn list_subtitles(
    pool: &PgPool,
    media_item_id: Uuid,
) -> Result<SubtitleListResponse, SubtitleError> {
    let rows = sqlx::query(LIST_SUBTITLES_SQL)
        .bind(media_item_id)
        .fetch_all(pool)
        .await?;

    let items: Vec<SubtitleFileResponse> = rows.iter().map(row_to_response).collect();
    let total = items.len() as i64;

    Ok(SubtitleListResponse { items, total })
}

pub async fn get_subtitle(
    pool: &PgPool,
    media_item_id: Uuid,
    subtitle_id: Uuid,
) -> Result<SubtitleFileResponse, SubtitleError> {
    let row = sqlx::query(GET_SUBTITLE_SQL)
        .bind(subtitle_id)
        .bind(media_item_id)
        .fetch_optional(pool)
        .await?
        .ok_or(SubtitleError::FileNotFound { subtitle_id })?;

    Ok(row_to_response(&row))
}

pub async fn get_subtitle_content(
    pool: &PgPool,
    media_item_id: Uuid,
    subtitle_id: Uuid,
    delivery_format: Option<&str>,
    user_offset_ms: Option<i32>,
) -> Result<(String, &'static str), SubtitleError> {
    let row = sqlx::query(GET_SUBTITLE_SQL)
        .bind(subtitle_id)
        .bind(media_item_id)
        .fetch_optional(pool)
        .await?
        .ok_or(SubtitleError::FileNotFound { subtitle_id })?;

    let file_path: String = row.try_get("file_path").unwrap_or_default();

    if file_path.contains("::embedded::") {
        return Err(SubtitleError::InvalidSubtitleFormat(
            "embedded subtitle extraction requires FFmpeg (not yet implemented)".into(),
        ));
    }

    let source_format = detect_subtitle_format(&file_path).ok_or_else(|| {
        SubtitleError::InvalidSubtitleFormat(format!(
            "unsupported subtitle file extension: {}",
            file_path
        ))
    })?;

    if matches!(source_format, "sup" | "sub" | "idx") {
        return Err(SubtitleError::InvalidSubtitleFormat(format!(
            "image subtitle format '{source_format}' requires OCR before delivery (see Task 5)"
        )));
    }

    let content = tokio::fs::read_to_string(&file_path)
        .await
        .map_err(|e| SubtitleError::FileNotFound { subtitle_id })?;

    let target_format = delivery_format.unwrap_or(source_format);
    if !VALID_DELIVERY_FORMATS.contains(&target_format) && target_format != "ass" && target_format != "ssa" {
        return Err(SubtitleError::InvalidSubtitleFormat(format!(
            "unsupported delivery format: {target_format}"
        )));
    }

    let srt_content = sub_svc::to_srt(&content, source_format);

    let mut final_content = if target_format == "vtt" {
        sub_svc::srt_to_webvtt(&srt_content)
    } else {
        srt_content
    };

    if let Some(offset) = user_offset_ms
        && offset != 0
    {
        final_content = sub_svc::apply_offset(&final_content, target_format, offset);
    }

    let content_type = subtitle_content_type(target_format);

    Ok((final_content, content_type))
}

pub async fn fetch_subtitles(
    state: &AppState,
    media_item_id: Uuid,
    req: &FetchSubtitlesRequest,
) -> Result<FetchSubtitlesResponse, SubtitleError> {
    let config = state.runtime_config.load();
    let subtitle_providers = &config.integrations.subtitle_providers;

    let item = get_media_item_for_fetch(&state.pool, media_item_id).await?;
    let media_file = resolve_media_file_path(&state.pool, media_item_id).await?;
    let media_file_str = media_file.to_string_lossy().to_string();

    let language = &req.language;
    let want_hi = req.is_hearing_impaired.unwrap_or(false);
    let want_forced = req.is_forced.unwrap_or(false);

    let provider_pref = match req.provider.as_deref() {
        Some("subdl") => vec!["subdl"],
        Some("opensubtitles") => vec!["opensubtitles"],
        _ => vec!["subdl", "opensubtitles"],
    };

    for &provider in &provider_pref {
        match provider {
            "subdl" if subtitle_providers.subdl.enabled => {
                let api_key = subtitle_providers.subdl.api_key.as_deref().unwrap_or("");
                if api_key.is_empty() {
                    continue;
                }
                let client = SubdlClient::new(api_key.to_string());
                let item_type = if item.media_type == "tv" || item.media_type == "episode" {
                    Some("tv")
                } else {
                    Some("movie")
                };
                let results = match item.tmdb_id {
                    Some(tmdb_id) => {
                        client
                            .search_by_tmdb(tmdb_id, language, item_type)
                            .await
                    }
                    None => match &item.imdb_id {
                        Some(imdb) => {
                            client
                                .search_by_imdb(imdb, language, item_type)
                                .await
                        }
                        None => {
                            client
                                .search_by_name(&item.title, language, item_type)
                                .await
                        }
                    },
                };

                match results {
                    Ok(search_results) => {
                        if let Some(fetched) = try_download_and_save_subdl(
                            &client,
                            search_results,
                            language,
                            want_hi,
                            want_forced,
                            &media_file,
                            &state.pool,
                            media_item_id,
                            subtitle_providers.subdl.prefer_hearing_impaired,
                        )
                        .await?
                        {
                            return Ok(FetchSubtitlesResponse {
                                fetched: vec![fetched],
                                provider_used: Some("subdl".to_string()),
                                no_results: false,
                            });
                        }
                    }
                    Err(SubtitleError::ProviderUnavailable { .. })
                    | Err(SubtitleError::ProviderRateLimited { .. }) => {
                        tracing::warn!("SubDL provider error, falling through to next provider");
                        continue;
                    }
                    Err(e) => return Err(e),
                }
            }
            "opensubtitles" if subtitle_providers.opensubtitles.enabled => {
                let api_key = subtitle_providers.opensubtitles.api_key.as_deref().unwrap_or("");
                if api_key.is_empty() {
                    continue;
                }
                let client = OpensubtitlesClient::new(api_key.to_string());
                let item_type = if item.media_type == "tv" || item.media_type == "episode" {
                    Some("tv")
                } else {
                    Some("movie")
                };

                let hash_result = match sub_svc::compute_oshash(&media_file).await {
                    Ok(hash) => {
                        let file_size = tokio::fs::metadata(&media_file)
                            .await
                            .map(|m| m.len())
                            .unwrap_or(0);
                        Some((hash, file_size))
                    }
                    Err(e) => {
                        tracing::debug!("OSHash computation failed: {e}");
                        None
                    }
                };

                let results = if let Some((ref hash, file_size)) = hash_result {
                    client
                        .search_by_hash(hash, file_size, language)
                        .await
                } else {
                    match item.tmdb_id {
                        Some(tmdb_id) => {
                            client
                                .search_by_tmdb(tmdb_id, language, item_type)
                                .await
                        }
                        None => match &item.imdb_id {
                            Some(imdb) => {
                                client
                                    .search_by_imdb(imdb, language, item_type)
                                    .await
                            }
                            None => {
                                client
                                    .search_by_query(&item.title, language)
                                    .await
                            }
                        },
                    }
                };

                match results {
                    Ok(search_results) => {
                        if let Some(fetched) = try_download_and_save_os(
                            &client,
                            search_results,
                            language,
                            want_hi,
                            want_forced,
                            &media_file,
                            &state.pool,
                            media_item_id,
                            subtitle_providers.opensubtitles.prefer_hearing_impaired,
                        )
                        .await?
                        {
                            return Ok(FetchSubtitlesResponse {
                                fetched: vec![fetched],
                                provider_used: Some("opensubtitles".to_string()),
                                no_results: false,
                            });
                        }
                    }
                    Err(SubtitleError::ProviderUnavailable { .. })
                    | Err(SubtitleError::ProviderRateLimited { .. }) => {
                        tracing::warn!("OpenSubtitles provider error, no more providers");
                        continue;
                    }
                    Err(e) => return Err(e),
                }
            }
            _ => {}
        }
    }

    Ok(FetchSubtitlesResponse {
        fetched: vec![],
        provider_used: None,
        no_results: true,
    })
}

struct MediaItemForFetch {
    title: String,
    tmdb_id: Option<u64>,
    imdb_id: Option<String>,
    media_type: String,
}

async fn get_media_item_for_fetch(
    pool: &PgPool,
    media_item_id: Uuid,
) -> Result<MediaItemForFetch, SubtitleError> {
    let row = sqlx::query(
        r#"SELECT title, tmdb_id, imdb_id, media_type
           FROM media_items
           WHERE id = $1"#,
    )
    .bind(media_item_id)
    .fetch_optional(pool)
    .await?
    .ok_or(SubtitleError::MediaItemNotFound { media_item_id })?;

    let tmdb_raw: Option<i64> = row.try_get("tmdb_id").unwrap_or(None);
    let tmdb_id = tmdb_raw.map(|v| v as u64);

    Ok(MediaItemForFetch {
        title: row.try_get("title").unwrap_or_default(),
        tmdb_id,
        imdb_id: row.try_get("imdb_id").unwrap_or(None),
        media_type: row.try_get("media_type").unwrap_or_default(),
    })
}

#[allow(clippy::too_many_arguments)]
async fn try_download_and_save_subdl(
    client: &SubdlClient,
    results: Vec<SubtitleSearchResult>,
    language: &str,
    want_hi: bool,
    want_forced: bool,
    media_file: &Path,
    pool: &PgPool,
    media_item_id: Uuid,
    prefer_hi: bool,
) -> Result<Option<SubtitleFileResponse>, SubtitleError> {
    let best = pick_best_result(results, language, want_hi, want_forced, prefer_hi);
    let Some(best) = best else {
        return Ok(None);
    };

    let raw_bytes = client.download(&best.download_url).await?;
    let subtitle_bytes = extract_subtitle_from_zip(&raw_bytes)?;
    let file_ext = &best.format;
    let saved_path = save_subtitle_file(media_file, language, file_ext, &subtitle_bytes).await?;
    let saved_path_str = saved_path.to_string_lossy().to_string();

    let row = insert_fetched_subtitle(
        pool,
        media_item_id,
        &saved_path_str,
        &best.language,
        best.is_forced,
        best.is_hearing_impaired,
        "subdl",
    )
    .await?;

    Ok(Some(row))
}

#[allow(clippy::too_many_arguments)]
async fn try_download_and_save_os(
    client: &OpensubtitlesClient,
    results: Vec<SubtitleSearchResult>,
    language: &str,
    want_hi: bool,
    want_forced: bool,
    media_file: &Path,
    pool: &PgPool,
    media_item_id: Uuid,
    prefer_hi: bool,
) -> Result<Option<SubtitleFileResponse>, SubtitleError> {
    let best = pick_best_result(results, language, want_hi, want_forced, prefer_hi);
    let Some(best) = best else {
        return Ok(None);
    };

    let parts: Vec<&str> = best.download_url.split(':').collect();
    if parts.len() != 2 {
        return Err(SubtitleError::FetchFailed {
            reason: format!("invalid OpenSubtitles download URL: {}", best.download_url),
        });
    }
    let file_id: u64 = parts[1].parse().map_err(|_| SubtitleError::FetchFailed {
        reason: format!("invalid file_id in OpenSubtitles result: {}", parts[1]),
    })?;

    let (raw_bytes, _server_filename) = client.download(file_id).await?;
    let subtitle_bytes = if is_zip(&raw_bytes) {
        extract_subtitle_from_zip(&raw_bytes)?
    } else {
        raw_bytes
    };
    let file_ext = if best.format.is_empty() { "srt" } else { best.format.as_str() };
    let saved_path = save_subtitle_file(media_file, language, file_ext, &subtitle_bytes).await?;
    let saved_path_str = saved_path.to_string_lossy().to_string();

    let row = insert_fetched_subtitle(
        pool,
        media_item_id,
        &saved_path_str,
        &best.language,
        best.is_forced,
        best.is_hearing_impaired,
        "opensubtitles",
    )
    .await?;

    Ok(Some(row))
}

fn pick_best_result(
    results: Vec<SubtitleSearchResult>,
    language: &str,
    want_hi: bool,
    want_forced: bool,
    prefer_hi: bool,
) -> Option<SubtitleSearchResult> {
    let lang_lower = language.to_lowercase();

    let mut lang_matches: Vec<SubtitleSearchResult> = results
        .into_iter()
        .filter(|r| {
            let r_lang = r.language.to_lowercase();
            r_lang == lang_lower || r_lang.starts_with(&lang_lower)
        })
        .collect();

    if lang_matches.is_empty() {
        return None;
    }

    let forced_matches: Vec<SubtitleSearchResult> = lang_matches
        .iter()
        .filter(|r| r.is_forced == want_forced)
        .cloned()
        .collect();
    let hi_matches: Vec<SubtitleSearchResult> = lang_matches
        .iter()
        .filter(|r| r.is_hearing_impaired == want_hi)
        .cloned()
        .collect();

    if !forced_matches.is_empty() {
        lang_matches = forced_matches;
    } else if !hi_matches.is_empty() {
        lang_matches = hi_matches;
    }

    lang_matches.sort_by(|a, b| {
        let a_score = score_result(a, prefer_hi);
        let b_score = score_result(b, prefer_hi);
        b_score.cmp(&a_score)
    });

    lang_matches.into_iter().next()
}

fn score_result(r: &SubtitleSearchResult, prefer_hi: bool) -> u32 {
    let mut score = r.vote_count;
    if r.is_hearing_impaired == prefer_hi {
        score += 10;
    }
    if r.format == "srt" {
        score += 5;
    }
    score
}

fn extract_subtitle_from_zip(data: &[u8]) -> Result<Vec<u8>, SubtitleError> {
    let reader = Cursor::new(data);
    let mut archive = zip::ZipArchive::new(reader).map_err(|e| {
        SubtitleError::FetchFailed {
            reason: format!("ZIP parse error: {e}"),
        }
    })?;

    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(|e| SubtitleError::FetchFailed {
                reason: format!("ZIP entry read error: {e}"),
            })?;

        let name = entry.name().to_lowercase();
        if name.ends_with(".srt")
            || name.ends_with(".vtt")
            || name.ends_with(".ass")
            || name.ends_with(".ssa")
            || name.ends_with(".ttml")
        {
            let mut buf = Vec::new();
            entry
                .read_to_end(&mut buf)
                .map_err(|e| SubtitleError::FetchFailed {
                    reason: format!("ZIP content read error: {e}"),
                })?;
            return Ok(buf);
        }
    }

    Err(SubtitleError::FetchFailed {
        reason: "ZIP archive contains no subtitle files".to_string(),
    })
}

fn is_zip(data: &[u8]) -> bool {
    data.len() >= 4 && data[0..2] == [0x50, 0x4b]
}

async fn save_subtitle_file(
    media_file: &Path,
    language: &str,
    ext: &str,
    content: &[u8],
) -> Result<PathBuf, SubtitleError> {
    let stem = media_file
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "subtitle".to_string());
    let parent = media_file.parent().unwrap_or_else(|| std::path::Path::new("."));

    let filename = format!("{stem}.{language}.{ext}");
    let target = parent.join(&filename);

    tokio::fs::write(&target, content)
        .await
        .map_err(|e| SubtitleError::FetchFailed {
            reason: format!("failed to write subtitle file {target:?}: {e}"),
        })?;

    tracing::info!(path = %target.display(), "saved fetched subtitle");
    Ok(target)
}

async fn insert_fetched_subtitle(
    pool: &PgPool,
    media_item_id: Uuid,
    file_path: &str,
    language: &str,
    is_forced: bool,
    is_hearing_impaired: bool,
    provider: &str,
) -> Result<SubtitleFileResponse, SubtitleError> {
    let row = sqlx::query(
        r#"INSERT INTO subtitle_files (id, media_item_id, file_path, language, subtitle_type, is_forced, is_hearing_impaired, source_provider)
           VALUES (uuidv7(), $1, $2, $3, 'fetched', $4, $5, $6)
           RETURNING id, media_item_id, file_path, language, subtitle_type, is_forced, is_hearing_impaired, source_provider"#,
    )
    .bind(media_item_id)
    .bind(file_path)
    .bind(language)
    .bind(is_forced)
    .bind(is_hearing_impaired)
    .bind(provider)
    .fetch_one(pool)
    .await?;

    Ok(SubtitleFileResponse {
        id: row.try_get("id").unwrap_or_default(),
        media_item_id: row.try_get("media_item_id").unwrap_or_default(),
        file_path: row.try_get("file_path").unwrap_or_default(),
        language: row.try_get("language").unwrap_or_default(),
        subtitle_type: row.try_get("subtitle_type").unwrap_or_default(),
        is_forced: row.try_get("is_forced").unwrap_or(false),
        is_hearing_impaired: row.try_get("is_hearing_impaired").unwrap_or(false),
        source_provider: row.try_get("source_provider").ok().flatten(),
    })
}

pub async fn set_subtitle_offset(
    pool: &PgPool,
    user_id: Uuid,
    media_item_id: Uuid,
    subtitle_id: Uuid,
    offset_ms: i32,
) -> Result<SubtitleOffsetResponse, SubtitleError> {
    let row = sqlx::query(GET_SUBTITLE_SQL)
        .bind(subtitle_id)
        .bind(media_item_id)
        .fetch_optional(pool)
        .await?
        .ok_or(SubtitleError::FileNotFound { subtitle_id })?;

    let patch = serde_json::json!({ "subtitle_offset_ms": offset_ms });

    sqlx::query(
        r#"INSERT INTO user_item_data (id, user_id, media_item_id, metadata)
           VALUES (uuidv7(), $1, $2, $3::jsonb)
           ON CONFLICT (user_id, media_item_id)
           DO UPDATE SET metadata = COALESCE(user_item_data.metadata, '{}'::jsonb) || $3::jsonb,
                         updated_at = now()"#,
    )
    .bind(user_id)
    .bind(media_item_id)
    .bind(&patch)
    .execute(pool)
    .await?;

    Ok(SubtitleOffsetResponse {
        subtitle_id,
        offset_ms,
    })
}

pub async fn trigger_ocr(
    pool: &PgPool,
    media_item_id: Uuid,
    subtitle_id: Uuid,
    engine_override: Option<&str>,
) -> Result<SubtitleOcrResult, SubtitleError> {
    let row = sqlx::query(GET_SUBTITLE_SQL)
        .bind(subtitle_id)
        .bind(media_item_id)
        .fetch_optional(pool)
        .await?
        .ok_or(SubtitleError::FileNotFound { subtitle_id })?;

    let file_path: String = row.try_get("file_path").unwrap_or_default();

    let (source_path, stream_index) = parse_embedded_path(&file_path)
        .ok_or_else(|| SubtitleError::InvalidSubtitleFormat(
            "OCR is only applicable to embedded bitmap subtitles (PGS/VobSub)".into(),
        ))?;

    if !is_image_subtitle(&file_path) {
        return Err(SubtitleError::InvalidSubtitleFormat(
            "OCR is only applicable to image subtitles (.sup/.sub); text subtitles do not need OCR".into(),
        ));
    }

    let engine = engine_override.map(parse_engine_override);

    let media_file_path = resolve_media_file_path(pool, media_item_id).await?;

    let result = sub_svc::run_ocr(
        &media_file_path,
        stream_index,
        engine,
        media_item_id,
    )
    .await
    .map_err(|_| SubtitleError::OcrUnavailable)?;

    let threshold = 0.80f64;
    let below_threshold = result
        .confidence_score
        .is_some_and(|c| c < threshold);

    Ok(SubtitleOcrResult {
        subtitle_file_id: subtitle_id,
        ocr_engine: result.engine.as_str().to_string(),
        confidence_score: result.confidence_score,
        srt_content_length: result.srt_content.len(),
        below_threshold,
    })
}

fn parse_embedded_path(file_path: &str) -> Option<(PathBuf, i32)> {
    let marker = "::embedded::";
    let pos = file_path.find(marker)?;
    let media_path = PathBuf::from(&file_path[..pos]);
    let stream_str = &file_path[pos + marker.len()..];
    let stream_index: i32 = stream_str.parse().ok()?;
    Some((media_path, stream_index))
}

fn is_image_subtitle(file_path: &str) -> bool {
    let ext = file_path.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    matches!(ext.as_str(), "sup" | "sub" | "idx" | "pgs")
        || file_path.contains("hdmv_pgs")
        || file_path.contains("dvd_subtitle")
        || file_path.contains("::embedded::")
}

fn parse_engine_override(engine: &str) -> crate::services::subtitles::OcrEngine {
    match engine.to_ascii_lowercase().as_str() {
        "tesseract" => crate::services::subtitles::OcrEngine::Tesseract,
        _ => crate::services::subtitles::OcrEngine::PaddleOcr,
    }
}

async fn resolve_media_file_path(
    pool: &PgPool,
    media_item_id: Uuid,
) -> Result<PathBuf, SubtitleError> {
    let row = sqlx::query(
        r#"SELECT file_path FROM media_files
           WHERE media_item_id = $1 AND is_healthy = true
           ORDER BY created_at ASC LIMIT 1"#,
    )
    .bind(media_item_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| SubtitleError::MediaItemNotFound { media_item_id })?;

    let path: String = row.try_get("file_path").unwrap_or_default();
    Ok(PathBuf::from(path))
}

pub async fn get_subtitle_sync_data(
    pool: &PgPool,
    media_item_id: Uuid,
    subtitle_id: Uuid,
) -> Result<SubtitleSyncDataResponse, SubtitleError> {
    let row = sqlx::query(
        r#"SELECT subtitle_file_id, sync_method, offset_ms, confidence,
                  fps_source, fps_target
           FROM subtitle_sync_data
           WHERE media_item_id = $1 AND subtitle_file_id = $2
           ORDER BY created_at DESC
           LIMIT 1"#,
    )
    .bind(media_item_id)
    .bind(subtitle_id)
    .fetch_optional(pool)
    .await?
    .ok_or(SubtitleError::SyncDataNotFound { subtitle_id })?;

    Ok(SubtitleSyncDataResponse {
        subtitle_file_id: row.try_get("subtitle_file_id").unwrap_or_default(),
        sync_method: row.try_get("sync_method").unwrap_or_default(),
        offset_ms: row.try_get("offset_ms").unwrap_or(0),
        confidence: row.try_get("confidence").ok().flatten(),
        fps_source: row.try_get("fps_source").ok().flatten(),
        fps_target: row.try_get("fps_target").ok().flatten(),
    })
}

pub async fn delete_subtitle(
    pool: &PgPool,
    media_item_id: Uuid,
    subtitle_id: Uuid,
) -> Result<(), SubtitleError> {
    let row = sqlx::query(GET_SUBTITLE_SQL)
        .bind(subtitle_id)
        .bind(media_item_id)
        .fetch_optional(pool)
        .await?
        .ok_or(SubtitleError::FileNotFound { subtitle_id })?;

    let subtitle_type: String = row.try_get("subtitle_type").unwrap_or_default();

    if subtitle_type == "embedded" || subtitle_type == "external" {
        return Err(SubtitleError::InvalidSubtitleFormat(
            "only fetched subtitles can be deleted via API".into(),
        ));
    }

    sqlx::query("DELETE FROM subtitle_files WHERE id = $1 AND media_item_id = $2")
        .bind(subtitle_id)
        .bind(media_item_id)
        .execute(pool)
        .await?;

    Ok(())
}

pub fn validate_language_code(code: &str) -> bool {
    code.len() >= 2 && code.len() <= 10 && code.chars().all(|c| c.is_ascii_alphabetic())
}

fn row_to_response(row: &sqlx::postgres::PgRow) -> SubtitleFileResponse {
    SubtitleFileResponse {
        id: row.try_get("id").unwrap_or_default(),
        media_item_id: row.try_get("media_item_id").unwrap_or_default(),
        file_path: row.try_get("file_path").unwrap_or_default(),
        language: row.try_get("language").unwrap_or_default(),
        subtitle_type: row.try_get("subtitle_type").unwrap_or_default(),
        is_forced: row.try_get("is_forced").unwrap_or(false),
        is_hearing_impaired: row.try_get("is_hearing_impaired").unwrap_or(false),
        source_provider: row.try_get("source_provider").ok().flatten(),
    }
}

fn detect_subtitle_format(file_path: &str) -> Option<&'static str> {
    let ext = file_path.rsplit('.').next()?.to_ascii_lowercase();
    match ext.as_str() {
        "srt" => Some("srt"),
        "ass" => Some("ass"),
        "ssa" => Some("ssa"),
        "vtt" => Some("vtt"),
        "sup" => Some("sup"),
        "sub" => Some("sub"),
        "idx" => Some("idx"),
        "ttml" => Some("ttml"),
        _ => None,
    }
}

fn subtitle_content_type(format: &str) -> &'static str {
    match format {
        "vtt" => "text/vtt; charset=utf-8",
        "srt" => "application/x-subrip; charset=utf-8",
        _ => "text/plain; charset=utf-8",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_subtitle_format() {
        assert_eq!(detect_subtitle_format("movie.en.srt"), Some("srt"));
        assert_eq!(detect_subtitle_format("movie.ass"), Some("ass"));
        assert_eq!(detect_subtitle_format("movie.ssa"), Some("ssa"));
        assert_eq!(detect_subtitle_format("movie.vtt"), Some("vtt"));
        assert_eq!(detect_subtitle_format("movie.sup"), Some("sup"));
        assert_eq!(detect_subtitle_format("movie.txt"), None);
    }

    #[test]
    fn test_subtitle_content_type() {
        assert_eq!(subtitle_content_type("vtt"), "text/vtt; charset=utf-8");
        assert_eq!(
            subtitle_content_type("srt"),
            "application/x-subrip; charset=utf-8"
        );
        assert_eq!(
            subtitle_content_type("ass"),
            "text/plain; charset=utf-8"
        );
    }

    #[test]
    fn test_validate_language_code() {
        assert!(validate_language_code("en"));
        assert!(validate_language_code("eng"));
        assert!(!validate_language_code("en-US"));
        assert!(!validate_language_code("x"));
        assert!(!validate_language_code("english_language"));
        assert!(!validate_language_code("en1"));
    }

    #[test]
    fn test_parse_embedded_path() {
        let path = "/media/movie.mkv::embedded::2";
        let result = parse_embedded_path(path);
        assert!(result.is_some());
        let (p, idx) = result.unwrap();
        assert_eq!(p, std::path::PathBuf::from("/media/movie.mkv"));
        assert_eq!(idx, 2);
    }

    #[test]
    fn test_parse_embedded_path_invalid() {
        assert!(parse_embedded_path("/media/movie.srt").is_none());
        assert!(parse_embedded_path("/media/movie.mkv::embedded::abc").is_none());
    }

    #[test]
    fn test_is_image_subtitle() {
        assert!(is_image_subtitle("movie.sup"));
        assert!(is_image_subtitle("movie.sub"));
        assert!(is_image_subtitle("movie.idx"));
        assert!(is_image_subtitle("/path/hdmv_pgs_subtitle.mkv"));
        assert!(is_image_subtitle("/path/dvd_subtitle.mkv"));
        assert!(is_image_subtitle("/path/movie.mkv::embedded::0"));
        assert!(!is_image_subtitle("movie.srt"));
        assert!(!is_image_subtitle("movie.ass"));
    }

    #[test]
    fn test_parse_engine_override() {
        assert_eq!(
            parse_engine_override("paddleocr"),
            crate::services::subtitles::OcrEngine::PaddleOcr
        );
        assert_eq!(
            parse_engine_override("tesseract"),
            crate::services::subtitles::OcrEngine::Tesseract
        );
        assert_eq!(
            parse_engine_override("PADDLEOCR"),
            crate::services::subtitles::OcrEngine::PaddleOcr
        );
    }
}

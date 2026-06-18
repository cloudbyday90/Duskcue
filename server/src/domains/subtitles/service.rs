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

use std::path::PathBuf;

use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::domains::subtitles::error::SubtitleError;
use crate::domains::subtitles::types::*;
use crate::services::subtitles as sub_svc;

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
    pool: &PgPool,
    media_item_id: Uuid,
    req: &FetchSubtitlesRequest,
) -> Result<FetchSubtitlesResponse, SubtitleError> {
    Err(SubtitleError::FetchFailed {
        reason: "subtitle provider fetching not yet implemented (Task 6)".into(),
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

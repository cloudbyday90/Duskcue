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

use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::domains::subtitles::error::SubtitleError;
use crate::domains::subtitles::types::*;

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

    let srt_content = to_srt(&content, source_format);

    let mut final_content = if target_format == "vtt" {
        srt_to_webvtt(&srt_content)
    } else {
        srt_content
    };

    if let Some(offset) = user_offset_ms
        && offset != 0
    {
        final_content = apply_offset(&final_content, target_format, offset);
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
    Err(SubtitleError::OcrUnavailable)
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

fn to_srt(content: &str, source_format: &str) -> String {
    match source_format {
        "srt" => content.to_string(),
        "ass" | "ssa" => ass_to_srt(content),
        "vtt" => vtt_to_srt(content),
        _ => content.to_string(),
    }
}

fn srt_to_webvtt(srt: &str) -> String {
    let mut output = String::with_capacity(srt.len() + 16);
    output.push_str("WEBVTT\n\n");

    let mut cue_num = 1u32;
    for block in srt.split("\n\n") {
        let block = block.trim();
        if block.is_empty() {
            continue;
        }

        let lines: Vec<&str> = block.lines().collect();
        let timecode_idx = lines
            .iter()
            .position(|l| l.contains("-->"));

        let timecode_idx = match timecode_idx {
            Some(idx) => idx,
            None => continue,
        };

        let timecode = lines[timecode_idx].replace(',', ".");
        let text_lines = &lines[timecode_idx + 1..];

        output.push_str(&format!("{cue_num}\n"));
        output.push_str(&timecode);
        output.push('\n');
        for line in text_lines {
            output.push_str(line);
            output.push('\n');
        }
        output.push('\n');
        cue_num += 1;
    }

    output
}

fn vtt_to_srt(vtt: &str) -> String {
    let mut output = String::with_capacity(vtt.len());
    let mut cue_num = 1u32;

    for block in vtt.split("\n\n") {
        let block = block.trim();
        if block.is_empty() || block.starts_with("WEBVTT") || block.starts_with("NOTE") {
            continue;
        }

        let lines: Vec<&str> = block.lines().collect();
        let timecode_idx = lines
            .iter()
            .position(|l| l.contains("-->"));

        let timecode_idx = match timecode_idx {
            Some(idx) => idx,
            None => continue,
        };

        let timecode = lines[timecode_idx].replace('.', ",");
        let text_lines = &lines[timecode_idx + 1..];

        output.push_str(&format!("{cue_num}\n"));
        output.push_str(&timecode);
        output.push('\n');
        for line in text_lines {
            output.push_str(line);
            output.push('\n');
        }
        output.push('\n');
        cue_num += 1;
    }

    output
}

fn ass_to_srt(ass: &str) -> String {
    let mut output = String::with_capacity(ass.len() / 2);
    let mut in_events = false;
    let mut start_idx = 1;
    let mut end_idx = 2;
    let mut text_idx = 9;
    let mut cue_num = 1u32;

    for line in ass.lines() {
        let line = line.trim();

        if line.starts_with('[') && line.ends_with(']') {
            in_events = line.eq_ignore_ascii_case("[Events]");
            continue;
        }

        if !in_events {
            continue;
        }

        if let Some(format_spec) = line.strip_prefix("Format:") {
            let fields: Vec<&str> = format_spec.split(',').map(|f| f.trim()).collect();
            for (i, field) in fields.iter().enumerate() {
                match field.to_ascii_lowercase().as_str() {
                    "start" => start_idx = i,
                    "end" => end_idx = i,
                    "text" => text_idx = i,
                    _ => {}
                }
            }
            continue;
        }

        if let Some(dialogue) = line.strip_prefix("Dialogue:") {
            let fields: Vec<&str> = dialogue.split(',').collect();
            if fields.len() <= text_idx.max(start_idx).max(end_idx) {
                continue;
            }

            let start = fields[start_idx].trim();
            let end = fields[end_idx].trim();
            let text = fields[text_idx].trim();

            let start_srt = ass_timestamp_to_srt(start);
            let end_srt = ass_timestamp_to_srt(end);

            let clean_text = text
                .replace(r"\N", "\n")
                .replace(r"\n", "\n")
                .replace(r"\h", " ");

            let clean_text = strip_ass_override_tags(&clean_text);

            output.push_str(&format!("{cue_num}\n"));
            output.push_str(&start_srt);
            output.push_str(" --> ");
            output.push_str(&end_srt);
            output.push('\n');
            output.push_str(&clean_text);
            output.push_str("\n\n");

            cue_num += 1;
        }
    }

    output
}

fn ass_timestamp_to_srt(ts: &str) -> String {
    let ts = ts.trim();
    let parts: Vec<&str> = ts.split(':').collect();
    if parts.len() != 3 {
        return "00:00:00,000".to_string();
    }

    let h: u32 = parts[0].parse().unwrap_or(0);
    let m: u32 = parts[1].parse().unwrap_or(0);
    let sec_and_cs = parts[2];
    let (sec_str, cs_str) = if let Some(dot_pos) = sec_and_cs.find('.') {
        (&sec_and_cs[..dot_pos], &sec_and_cs[dot_pos + 1..])
    } else {
        (sec_and_cs, "0")
    };

    let s: u32 = sec_str.parse().unwrap_or(0);
    let cs: u32 = cs_str.parse().unwrap_or(0);
    let ms = cs * 10;

    format!("{h:02}:{m:02}:{s:02},{ms:03}")
}

fn strip_ass_override_tags(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut in_tag = false;

    for ch in text.chars() {
        match ch {
            '{' => in_tag = true,
            '}' => in_tag = false,
            _ if !in_tag => result.push(ch),
            _ => {}
        }
    }

    result
}

fn apply_offset(content: &str, format: &str, offset_ms: i32) -> String {
    let is_vtt = format == "vtt";
    let separator = if is_vtt { '.' } else { ',' };

    content
        .lines()
        .map(|line| {
            if line.contains("-->") {
                apply_offset_to_timecode_line(line, separator, offset_ms)
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn apply_offset_to_timecode_line(line: &str, separator: char, offset_ms: i32) -> String {
    let parts: Vec<&str> = line.split("-->").collect();
    if parts.len() != 2 {
        return line.to_string();
    }

    let start = parts[0].trim();
    let end = parts[1].trim();

    let start_ms = parse_timecode_to_ms(start, separator);
    let end_ms = parse_timecode_to_ms(end, separator);

    let new_start = (start_ms as i64 + offset_ms as i64).max(0) as u64;
    let new_end = (end_ms as i64 + offset_ms as i64).max(0) as u64;

    let start_str = ms_to_timecode(new_start, separator);
    let end_str = ms_to_timecode(new_end, separator);

    format!("{start_str} --> {end_str}")
}

fn parse_timecode_to_ms(tc: &str, separator: char) -> u64 {
    let tc = tc.trim();
    let parts: Vec<&str> = tc.split(':').collect();

    let (h, m, s_ms) = if parts.len() == 3 {
        (
            parts[0].parse::<u64>().unwrap_or(0),
            parts[1].parse::<u64>().unwrap_or(0),
            parts[2],
        )
    } else if parts.len() == 2 {
        (0u64, parts[0].parse::<u64>().unwrap_or(0), parts[1])
    } else {
        return 0;
    };

    let (s_str, ms_str) = if let Some(pos) = s_ms.find(separator) {
        (&s_ms[..pos], &s_ms[pos + 1..])
    } else {
        (s_ms, "0")
    };

    let s: u64 = s_str.parse().unwrap_or(0);
    let ms: u64 = ms_str.parse().unwrap_or(0);

    h * 3_600_000 + m * 60_000 + s * 1000 + ms
}

fn ms_to_timecode(ms: u64, separator: char) -> String {
    let h = ms / 3_600_000;
    let m = (ms % 3_600_000) / 60_000;
    let s = (ms % 60_000) / 1000;
    let millis = ms % 1000;

    format!("{h:02}:{m:02}:{s:02}{separator}{millis:03}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_srt_to_webvtt() {
        let srt = "1\n00:00:01,000 --> 00:00:04,000\nHello world\n\n2\n00:00:05,000 --> 00:00:09,000\nGoodbye\n";
        let vtt = srt_to_webvtt(srt);
        assert!(vtt.starts_with("WEBVTT\n\n"));
        assert!(vtt.contains("00:00:01.000 --> 00:00:04.000"));
        assert!(vtt.contains("00:00:05.000 --> 00:00:09.000"));
        assert!(!vtt.contains(','));
    }

    #[test]
    fn test_vtt_to_srt() {
        let vtt = "WEBVTT\n\n00:00:01.000 --> 00:00:04.000\nHello world\n\n00:00:05.000 --> 00:00:09.000\nGoodbye\n";
        let srt = vtt_to_srt(vtt);
        assert!(srt.contains("00:00:01,000 --> 00:00:04,000"));
        assert!(srt.contains("00:00:05,000 --> 00:00:09,000"));
        assert!(!srt.contains("WEBVTT"));
    }

    #[test]
    fn test_ass_to_srt() {
        let ass = "[Script Info]\nTitle: Test\n\n\
[Events]\n\
Format: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\n\
Dialogue: 0,0:00:01.00,0:00:04.00,Default,,0,0,0,,Hello world\n\
Dialogue: 0,0:00:05.50,0:00:09.00,Default,,0,0,0,,{\\b1}Bold{\\b0} text\n";
        let srt = ass_to_srt(ass);
        assert!(srt.contains("00:00:01,000 --> 00:00:04,000"));
        assert!(srt.contains("Hello world"));
        assert!(srt.contains("00:00:05,500 --> 00:00:09,000"));
        assert!(srt.contains("Bold text"));
        assert!(!srt.contains("{\\b1}"));
        assert!(!srt.contains("{\\b0}"));
    }

    #[test]
    fn test_ass_timestamp_to_srt() {
        assert_eq!(ass_timestamp_to_srt("0:00:01.00"), "00:00:01,000");
        assert_eq!(ass_timestamp_to_srt("1:23:45.67"), "01:23:45,670");
        assert_eq!(ass_timestamp_to_srt("0:05:30.50"), "00:05:30,500");
    }

    #[test]
    fn test_strip_ass_override_tags() {
        assert_eq!(strip_ass_override_tags("{\\b1}Bold{\\b0}"), "Bold");
        assert_eq!(strip_ass_override_tags("Plain text"), "Plain text");
        assert_eq!(
            strip_ass_override_tags("{\\an8}Top{\\an7}Left"),
            "TopLeft"
        );
    }

    #[test]
    fn test_apply_offset_positive() {
        let srt = "1\n00:00:01,000 --> 00:00:04,000\nText\n";
        let shifted = apply_offset(srt, "srt", 2000);
        assert!(shifted.contains("00:00:03,000 --> 00:00:06,000"));
    }

    #[test]
    fn test_apply_offset_negative_clamped() {
        let srt = "1\n00:00:01,000 --> 00:00:04,000\nText\n";
        let shifted = apply_offset(srt, "srt", -5000);
        assert!(shifted.contains("00:00:00,000 --> 00:00:00,000"));
    }

    #[test]
    fn test_apply_offset_vtt_separator() {
        let vtt = "WEBVTT\n\n00:00:01.000 --> 00:00:04.000\nText\n";
        let shifted = apply_offset(vtt, "vtt", 1500);
        assert!(shifted.contains("00:00:02.500 --> 00:00:05.500"));
    }

    #[test]
    fn test_parse_timecode_to_ms() {
        assert_eq!(parse_timecode_to_ms("00:00:01,000", ','), 1000);
        assert_eq!(parse_timecode_to_ms("00:01:30,500", ','), 90500);
        assert_eq!(parse_timecode_to_ms("01:00:00.000", '.'), 3600000);
        assert_eq!(parse_timecode_to_ms("01:30.000", '.'), 90000);
    }

    #[test]
    fn test_ms_to_timecode() {
        assert_eq!(ms_to_timecode(1000, ','), "00:00:01,000");
        assert_eq!(ms_to_timecode(90500, ','), "00:01:30,500");
        assert_eq!(ms_to_timecode(3600000, '.'), "01:00:00.000");
    }

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
}

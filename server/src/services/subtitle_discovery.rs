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
use std::path::{Path, PathBuf};

use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::workers::library_scanner::DiscoveredFile;

const SUBTITLE_PROCESS_EXTENSIONS: &[&str] = &["srt", "ass", "ssa", "vtt", "sub", "sup"];
const SUBTITLE_DIR_NAMES: &[&str] = &["subs", "subtitles"];

struct VideoFileEntry {
    media_item_id: Uuid,
    file_path: String,
    file_stem: String,
    parent_dir: PathBuf,
    additional_streams: serde_json::Value,
}

struct ParsedSubtitleName {
    base_name: String,
    language: String,
    is_forced: bool,
    is_hearing_impaired: bool,
}

pub async fn discover_subtitles(
    pool: &PgPool,
    library_id: Uuid,
    discovered: &[DiscoveredFile],
) -> Result<usize, sqlx::Error> {
    let video_files = load_video_files(pool, library_id).await?;

    if video_files.is_empty() {
        return Ok(0);
    }

    let mut inserted = 0usize;

    inserted += discover_external_subtitles(pool, discovered, &video_files).await?;
    inserted += discover_embedded_subtitles(pool, &video_files).await?;

    tracing::info!(
        library_id = %library_id,
        inserted,
        "Subtitle discovery completed"
    );

    Ok(inserted)
}

async fn load_video_files(
    pool: &PgPool,
    library_id: Uuid,
) -> Result<Vec<VideoFileEntry>, sqlx::Error> {
    let rows = sqlx::query(
        r#"SELECT mf.media_item_id, mf.file_path, mf.additional_streams
           FROM media_files mf
           JOIN media_items mi ON mi.id = mf.media_item_id
           WHERE mi.library_id = $1"#,
    )
    .bind(library_id)
    .fetch_all(pool)
    .await?;

    let mut entries = Vec::with_capacity(rows.len());
    for row in &rows {
        let file_path: String = row.get("file_path");
        let path = PathBuf::from(&file_path);
        let file_stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();
        let parent_dir = path.parent().unwrap_or(Path::new("")).to_path_buf();
        let additional_streams: serde_json::Value = row
            .try_get("additional_streams")
            .unwrap_or(serde_json::json!({}));

        entries.push(VideoFileEntry {
            media_item_id: row.get("media_item_id"),
            file_path,
            file_stem,
            parent_dir,
            additional_streams,
        });
    }

    Ok(entries)
}

async fn discover_external_subtitles(
    pool: &PgPool,
    discovered: &[DiscoveredFile],
    video_files: &[VideoFileEntry],
) -> Result<usize, sqlx::Error> {
    let dir_map = build_directory_map(video_files);
    let mut inserted = 0usize;

    for file in discovered {
        if !is_subtitle_file(&file.path) {
            continue;
        }

        let Some((vf, parsed)) = match_external_subtitle(&file.path, video_files, &dir_map) else {
            tracing::debug!(
                path = %file.path.display(),
                "External subtitle could not be matched to a video file"
            );
            continue;
        };

        let path_str = file.path.to_string_lossy().to_string();
        let rows = insert_subtitle_file(
            pool,
            vf.media_item_id,
            &path_str,
            &parsed.language,
            "external",
            parsed.is_forced,
            parsed.is_hearing_impaired,
            None,
        )
        .await?;

        inserted += rows;
    }

    Ok(inserted)
}

fn build_directory_map(video_files: &[VideoFileEntry]) -> HashMap<PathBuf, Vec<usize>> {
    let mut map: HashMap<PathBuf, Vec<usize>> = HashMap::new();
    for (i, vf) in video_files.iter().enumerate() {
        map.entry(vf.parent_dir.clone()).or_default().push(i);
    }
    map
}

fn match_external_subtitle<'a>(
    subtitle_path: &Path,
    video_files: &'a [VideoFileEntry],
    dir_map: &HashMap<PathBuf, Vec<usize>>,
) -> Option<(&'a VideoFileEntry, ParsedSubtitleName)> {
    let parent = subtitle_path.parent()?;
    let sub_parent_name = parent.file_name().and_then(|n| n.to_str()).unwrap_or("");
    let sub_parent_lower = sub_parent_name.to_lowercase();

    let search_dir = if SUBTITLE_DIR_NAMES.contains(&sub_parent_lower.as_str()) {
        parent.parent()?
    } else {
        parent
    };

    let candidate_indices = dir_map.get(search_dir)?;
    let candidates: Vec<&VideoFileEntry> = candidate_indices
        .iter()
        .filter_map(|&i| video_files.get(i))
        .collect();

    if candidates.is_empty() {
        return None;
    }

    let stem = subtitle_path.file_stem()?.to_str()?;
    let parsed = parse_subtitle_filename(stem);

    if candidates.len() == 1 {
        return Some((candidates[0], parsed));
    }

    for vf in &candidates {
        if vf.file_stem.eq_ignore_ascii_case(&parsed.base_name) {
            return Some((vf, parsed));
        }
    }

    for vf in &candidates {
        if vf
            .file_stem
            .to_lowercase()
            .starts_with(&parsed.base_name.to_lowercase())
        {
            return Some((vf, parsed));
        }
    }

    for vf in &candidates {
        if parsed
            .base_name
            .to_lowercase()
            .starts_with(&vf.file_stem.to_lowercase())
        {
            return Some((vf, parsed));
        }
    }

    None
}

async fn discover_embedded_subtitles(
    pool: &PgPool,
    video_files: &[VideoFileEntry],
) -> Result<usize, sqlx::Error> {
    let mut inserted = 0usize;

    for vf in video_files {
        if let Some(subs_array) = vf
            .additional_streams
            .get("subtitles")
            .and_then(|s| s.as_array())
        {
            for sub in subs_array {
                let stream_index = sub.get("index").and_then(|i| i.as_i64()).unwrap_or(0);
                let language = sub
                    .get("language")
                    .and_then(|l| l.as_str())
                    .unwrap_or("und")
                    .to_string();
                let is_forced = sub
                    .get("is_forced")
                    .and_then(|f| f.as_bool())
                    .unwrap_or(false);
                let is_hearing_impaired = sub
                    .get("is_hearing_impaired")
                    .and_then(|h| h.as_bool())
                    .unwrap_or(false);

                let synthetic_path = format!("{}::embedded::{}", vf.file_path, stream_index);

                let rows = insert_subtitle_file(
                    pool,
                    vf.media_item_id,
                    &synthetic_path,
                    &language,
                    "embedded",
                    is_forced,
                    is_hearing_impaired,
                    None,
                )
                .await?;

                inserted += rows;
            }
        }
    }

    Ok(inserted)
}

#[allow(clippy::too_many_arguments)]
async fn insert_subtitle_file(
    pool: &PgPool,
    media_item_id: Uuid,
    file_path: &str,
    language: &str,
    subtitle_type: &str,
    is_forced: bool,
    is_hearing_impaired: bool,
    source_provider: Option<&str>,
) -> Result<usize, sqlx::Error> {
    let result = sqlx::query(
        r#"INSERT INTO subtitle_files
               (media_item_id, file_path, language, subtitle_type,
                is_forced, is_hearing_impaired, source_provider)
           VALUES ($1, $2, $3, $4, $5, $6, $7)
           ON CONFLICT (media_item_id, file_path) DO NOTHING"#,
    )
    .bind(media_item_id)
    .bind(file_path)
    .bind(language)
    .bind(subtitle_type)
    .bind(is_forced)
    .bind(is_hearing_impaired)
    .bind(source_provider)
    .execute(pool)
    .await?;

    Ok(result.rows_affected() as usize)
}

fn is_subtitle_file(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| SUBTITLE_PROCESS_EXTENSIONS.contains(&e.to_lowercase().as_str()))
        .unwrap_or(false)
}

fn parse_subtitle_filename(stem: &str) -> ParsedSubtitleName {
    let parts: Vec<&str> = stem.split('.').collect();

    if parts.is_empty() {
        return ParsedSubtitleName {
            base_name: stem.to_string(),
            language: "und".to_string(),
            is_forced: false,
            is_hearing_impaired: false,
        };
    }

    let mut language = "und".to_string();
    let mut is_forced = false;
    let mut is_hearing_impaired = false;
    let mut base_end = parts.len();

    for (i, part) in parts.iter().enumerate().rev() {
        if i == 0 {
            break;
        }

        let lower = part.to_lowercase();

        if lower == "forced" {
            is_forced = true;
            base_end = i;
        } else if lower == "hi"
            || lower == "hearingimpaired"
            || lower == "hearing_impaired"
            || lower == "sdh"
            || lower == "cc"
        {
            is_hearing_impaired = true;
            base_end = i;
        } else if lower == "default" {
            base_end = i;
        } else if language == "und" && looks_like_language_code(part) {
            language = lower;
            base_end = i;
        } else {
            break;
        }
    }

    let base_name = parts[..base_end].join(".");

    ParsedSubtitleName {
        base_name,
        language,
        is_forced,
        is_hearing_impaired,
    }
}

fn looks_like_language_code(s: &str) -> bool {
    let len = s.len();
    if !(2..=5).contains(&len) {
        return false;
    }
    if !s.chars().all(|c| c.is_ascii_alphabetic() || c == '-') {
        return false;
    }
    s.starts_with(|c: char| c.is_ascii_alphabetic())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_filename() {
        let parsed = parse_subtitle_filename("Movie");
        assert_eq!(parsed.base_name, "Movie");
        assert_eq!(parsed.language, "und");
        assert!(!parsed.is_forced);
        assert!(!parsed.is_hearing_impaired);
    }

    #[test]
    fn test_parse_with_language() {
        let parsed = parse_subtitle_filename("The.Matrix.1999.en");
        assert_eq!(parsed.base_name, "The.Matrix.1999");
        assert_eq!(parsed.language, "en");
        assert!(!parsed.is_forced);
    }

    #[test]
    fn test_parse_with_language_3letter() {
        let parsed = parse_subtitle_filename("The.Matrix.1999.eng");
        assert_eq!(parsed.base_name, "The.Matrix.1999");
        assert_eq!(parsed.language, "eng");
    }

    #[test]
    fn test_parse_with_language_region() {
        let parsed = parse_subtitle_filename("Movie.pt-BR");
        assert_eq!(parsed.base_name, "Movie");
        assert_eq!(parsed.language, "pt-br");
    }

    #[test]
    fn test_parse_forced() {
        let parsed = parse_subtitle_filename("Movie.en.forced");
        assert_eq!(parsed.base_name, "Movie");
        assert_eq!(parsed.language, "en");
        assert!(parsed.is_forced);
    }

    #[test]
    fn test_parse_hearing_impaired() {
        let parsed = parse_subtitle_filename("Movie.en.hi");
        assert_eq!(parsed.base_name, "Movie");
        assert_eq!(parsed.language, "en");
        assert!(parsed.is_hearing_impaired);
    }

    #[test]
    fn test_parse_sdh() {
        let parsed = parse_subtitle_filename("Movie.en.sdh");
        assert_eq!(parsed.base_name, "Movie");
        assert_eq!(parsed.language, "en");
        assert!(parsed.is_hearing_impaired);
    }

    #[test]
    fn test_parse_cc() {
        let parsed = parse_subtitle_filename("Movie.en.cc");
        assert_eq!(parsed.base_name, "Movie");
        assert_eq!(parsed.language, "en");
        assert!(parsed.is_hearing_impaired);
    }

    #[test]
    fn test_parse_forced_and_hi() {
        let parsed = parse_subtitle_filename("Movie.es.forced.hi");
        assert_eq!(parsed.base_name, "Movie");
        assert_eq!(parsed.language, "es");
        assert!(parsed.is_forced);
        assert!(parsed.is_hearing_impaired);
    }

    #[test]
    fn test_parse_no_language_with_flag() {
        let parsed = parse_subtitle_filename("Movie.forced");
        assert_eq!(parsed.base_name, "Movie");
        assert_eq!(parsed.language, "und");
        assert!(parsed.is_forced);
    }

    #[test]
    fn test_language_code_detection() {
        assert!(looks_like_language_code("en"));
        assert!(looks_like_language_code("eng"));
        assert!(looks_like_language_code("en-US"));
        assert!(looks_like_language_code("pt-BR"));
        assert!(looks_like_language_code("zh"));
        assert!(looks_like_language_code("chi"));

        assert!(!looks_like_language_code("1"));
        assert!(!looks_like_language_code("1080p"));
        assert!(!looks_like_language_code("BluRay"));
        assert!(!looks_like_language_code("forced"));
        assert!(!looks_like_language_code(""));
        assert!(!looks_like_language_code("a"));
    }

    #[test]
    fn test_subtitle_file_detection() {
        assert!(is_subtitle_file(Path::new("/movie/movie.en.srt")));
        assert!(is_subtitle_file(Path::new("/movie/movie.ass")));
        assert!(is_subtitle_file(Path::new("/movie/movie.SUP")));
        assert!(is_subtitle_file(Path::new("/movie/movie.vtt")));
        assert!(is_subtitle_file(Path::new("/movie/movie.sub")));

        assert!(!is_subtitle_file(Path::new("/movie/movie.mkv")));
        assert!(!is_subtitle_file(Path::new("/movie/movie.idx")));
        assert!(!is_subtitle_file(Path::new("/movie/movie.mp4")));
    }
}

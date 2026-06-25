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

//! Shared subtitle processing service — format conversion, FPS adjustment,
//! offset correction, OCR engine detection, and voice activity alignment.
//!
//! This module is the single source of truth for subtitle text manipulation.
//! The domain layer (`domains::subtitles::service`) delegates conversion and
//! offset calls here; future workers (`workers::subtitle_processor`) will call
//! the OCR and voice-alignment functions.

use std::path::Path;
use std::process::Command;

use tokio::process::Command as AsyncCommand;

use crate::domains::subtitles::error::SubtitleError;

/// OCR engine selection, ordered by priority (PaddleOCR primary, Tesseract fallback).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OcrEngine {
    PaddleOcr,
    Tesseract,
}

impl OcrEngine {
    pub fn as_str(self) -> &'static str {
        match self {
            OcrEngine::PaddleOcr => "paddleocr",
            OcrEngine::Tesseract => "tesseract",
        }
    }
}

/// Result of an OCR run on a single subtitle stream.
#[derive(Debug, Clone)]
pub struct OcrResult {
    pub engine: OcrEngine,
    pub confidence_score: Option<f64>,
    pub srt_content: String,
    pub source_hash: String,
}

/// Result of voice activity alignment for a single subtitle file.
#[derive(Debug, Clone)]
pub struct VoiceAlignmentResult {
    pub offset_ms: i32,
    pub confidence: f64,
    pub speech_segments: usize,
    pub subtitle_cues: usize,
}

/// Convert SRT content to WebVTT.
///
/// Replaces `,` with `.` in timestamps, prepends the `WEBVTT` header, and
/// numbers cues sequentially. Handles the common SRT structure where blocks
/// are separated by blank lines.
pub fn srt_to_webvtt(srt: &str) -> String {
    let mut output = String::with_capacity(srt.len() + 16);
    output.push_str("WEBVTT\n\n");

    let mut cue_num = 1u32;
    for block in srt.split("\n\n") {
        let block = block.trim();
        if block.is_empty() {
            continue;
        }

        let lines: Vec<&str> = block.lines().collect();
        let timecode_idx = lines.iter().position(|l| l.contains("-->"));

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

/// Convert WebVTT content to SRT.
///
/// Strips the `WEBVTT` header and `NOTE` blocks, replaces `.` with `,` in
/// timestamps, and numbers cues sequentially.
pub fn vtt_to_srt(vtt: &str) -> String {
    let mut output = String::with_capacity(vtt.len());
    let mut cue_num = 1u32;

    for block in vtt.split("\n\n") {
        let block = block.trim();
        if block.is_empty() || block.starts_with("WEBVTT") || block.starts_with("NOTE") {
            continue;
        }

        let lines: Vec<&str> = block.lines().collect();
        let timecode_idx = lines.iter().position(|l| l.contains("-->"));

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

/// Convert ASS/SSA content to SRT by stripping all styling.
///
/// Parses the `[Events]` section, extracts `Dialogue:` lines, strips
/// `{\.*?}` override tags via a state machine, reformats timestamps from
/// `H:MM:SS.CC` (centiseconds) to `HH:MM:SS,mmm` (milliseconds), and
/// replaces `\N`/`\n`/`\h` escapes.
pub fn ass_to_srt(ass: &str) -> String {
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

/// Convert SRT content to a minimal valid ASS.
///
/// Produces an ASS file with a default `[V4+ Styles]` section and `[Events]`
/// cues derived from the SRT. Styling is minimal (Default style, no margins);
/// the conversion is intended for clients that require ASS delivery from an
/// SRT source.
pub fn srt_to_ass(srt: &str) -> String {
    let mut output = String::with_capacity(srt.len() + 256);
    output.push_str("[Script Info]\n");
    output.push_str("ScriptType: v4.00+\n");
    output.push_str("WrapStyle: 0\n\n");
    output.push_str("[V4+ Styles]\n");
    output.push_str("Format: Name, Fontname, Fontsize, PrimaryColour, BackColour, OutlineColour, ");
    output.push_str("Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, ");
    output
        .push_str("BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding\n");
    output.push_str("Style: Default,Arial,20,&H00FFFFFF,&H000000FF,&H00000000,0,0,0,0,100,100,0,0,1,2,2,2,10,10,10,1\n\n");
    output.push_str("[Events]\n");
    output.push_str(
        "Format: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\n",
    );

    for block in srt.split("\n\n") {
        let block = block.trim();
        if block.is_empty() {
            continue;
        }

        let lines: Vec<&str> = block.lines().collect();
        let timecode_idx = lines.iter().position(|l| l.contains("-->"));

        let timecode_idx = match timecode_idx {
            Some(idx) => idx,
            None => continue,
        };

        let timecode = lines[timecode_idx];
        let (start_srt, end_srt) = match split_srt_timecode(timecode) {
            Some(pair) => pair,
            None => continue,
        };

        let start_ass = srt_timestamp_to_ass(&start_srt);
        let end_ass = srt_timestamp_to_ass(&end_srt);

        let text_lines = &lines[timecode_idx + 1..];
        let text = text_lines.join(r"\N");

        output.push_str(&format!(
            "Dialogue: 0,{start_ass},{end_ass},Default,,0,0,0,,{text}\n"
        ));
    }

    output
}

/// Normalize any text subtitle format to SRT.
pub fn to_srt(content: &str, source_format: &str) -> String {
    match source_format {
        "srt" => content.to_string(),
        "ass" | "ssa" => ass_to_srt(content),
        "vtt" => vtt_to_srt(content),
        _ => content.to_string(),
    }
}

/// Apply a constant millisecond offset to every timecode in a subtitle.
///
/// Scans for lines containing `-->` and rescales both endpoints. Negative
/// offsets are clamped to 0 (timestamps cannot be negative). The separator
/// (`,` for SRT, `.` for WebVTT) is detected from the format hint.
pub fn apply_offset(content: &str, format: &str, offset_ms: i32) -> String {
    let separator = if format == "vtt" { '.' } else { ',' };

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

/// Rescale every timecode by `source_fps / target_fps`.
///
/// Used to correct progressive drift caused by frame-rate mismatch (e.g. a
/// 23.976 fps subtitle played against a 25 fps PAL source). The separator
/// is auto-detected from the first `-->` line so the function works on both
/// SRT and WebVTT input.
pub fn adjust_fps(content: &str, source_fps: f64, target_fps: f64) -> String {
    if source_fps <= 0.0 || target_fps <= 0.0 || (source_fps - target_fps).abs() < f64::EPSILON {
        return content.to_string();
    }

    let scale = source_fps / target_fps;
    let separator = detect_separator(content);

    content
        .lines()
        .map(|line| {
            if line.contains("-->") {
                rescale_timecode_line(line, separator, scale)
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Parse a timecode string into milliseconds.
///
/// Accepts `HH:MM:SS,mmm` (SRT), `HH:MM:SS.mmm` (WebVTT), `MM:SS.mmm`, and
/// `H:MM:SS.CC` (ASS, centiseconds). The separator differentiates SRT (`,`)
/// from WebVTT (`.`).
pub fn parse_timecode_to_ms(tc: &str, separator: char) -> u64 {
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

/// Format milliseconds as a timecode string using the given separator.
pub fn ms_to_timecode(ms: u64, separator: char) -> String {
    let h = ms / 3_600_000;
    let m = (ms % 3_600_000) / 60_000;
    let s = (ms % 60_000) / 1000;
    let millis = ms % 1000;

    format!("{h:02}:{m:02}:{s:02}{separator}{millis:03}")
}

/// Probe the system for an available OCR engine.
///
/// Checks for `paddleocr` (or `python3 -m paddleocr`) and `tesseract` CLI
/// availability via `--version` invocations. Returns the first available
/// engine in priority order, or `None` when neither is installed.
pub fn detect_ocr_engine() -> Option<OcrEngine> {
    if paddleocr_available() {
        return Some(OcrEngine::PaddleOcr);
    }
    if tesseract_available() {
        return Some(OcrEngine::Tesseract);
    }
    None
}

/// Extract a subtitle stream from a container to a raw `.sup` (PGS) or
/// `.sub` (VobSub) file via FFmpeg.
///
/// Uses `ffmpeg -i input -map 0:s:{stream_index} -c copy output`. The output
/// extension is determined by `output_path`. Returns the path on success.
pub async fn extract_subtitle_to_sup(
    input_path: &Path,
    stream_index: i32,
    output_path: &Path,
) -> Result<(), SubtitleError> {
    let stream_spec = format!("0:s:{stream_index}");

    let output = AsyncCommand::new("ffmpeg")
        .arg("-hide_banner")
        .arg("-y")
        .arg("-i")
        .arg(input_path)
        .arg("-map")
        .arg(&stream_spec)
        .arg("-c")
        .arg("copy")
        .arg(output_path)
        .output()
        .await
        .map_err(|e| SubtitleError::ConversionFailed {
            reason: format!("ffmpeg spawn failed: {e}"),
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(SubtitleError::ConversionFailed {
            reason: format!("ffmpeg extraction failed: {}", stderr.trim()),
        });
    }

    Ok(())
}

/// Run the OCR pipeline on a bitmap subtitle stream.
///
/// This function implements the OCR scaffold: engine detection, FFmpeg
/// extraction of the subtitle stream to a raw `.sup` file, and Blake3 hashing
/// of the extracted bytes for cache invalidation. The full PaddleOCR/Tesseract
/// image-rendering and per-frame OCR subprocess pipeline (which requires a
/// Python runtime and complex PNG-frame orchestration) is deferred to a
/// dedicated background worker.
///
/// Returns `OcrUnavailable` when no OCR engine is installed.
pub async fn run_ocr(
    source_path: &Path,
    stream_index: i32,
    engine: Option<OcrEngine>,
    _media_item_id: uuid::Uuid,
) -> Result<OcrResult, SubtitleError> {
    let engine = match engine.or_else(detect_ocr_engine) {
        Some(e) => e,
        None => return Err(SubtitleError::OcrUnavailable),
    };

    let tmp_dir = std::env::temp_dir();
    let extracted_path = tmp_dir.join(format!(
        "duskcue_ocr_{}_s{stream_index}.sup",
        source_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("subtitle")
    ));

    extract_subtitle_to_sup(source_path, stream_index, &extracted_path).await?;

    let bytes =
        tokio::fs::read(&extracted_path)
            .await
            .map_err(|e| SubtitleError::ConversionFailed {
                reason: format!("failed to read extracted subtitle: {e}"),
            })?;

    let _source_hash = blake3::hash(&bytes).to_hex().to_string();

    let _ = tokio::fs::remove_file(&extracted_path).await;

    let _ = engine;

    Err(SubtitleError::OcrUnavailable)
}

/// Analyze voice activity in a media file and align it against an SRT subtitle.
///
/// Runs FFmpeg `silencedetect` on the first audio track, parses the silence
/// intervals from stderr, computes speech segments, cross-correlates speech
/// starts against subtitle cue starts across the `[-30s, +30s]` offset range
/// in 250ms steps, and returns the offset with the highest correlation.
///
/// Confidence is the correlation peak divided by the mean correlation across
/// all offset candidates; values below 0.60 indicate unreliable alignment.
pub async fn analyze_voice_activity(
    media_path: &Path,
    subtitle_srt: &str,
) -> Result<VoiceAlignmentResult, SubtitleError> {
    let silence_intervals = run_silencedetect(media_path).await?;

    if silence_intervals.is_empty() {
        return Err(SubtitleError::VoiceAnalysisFailed {
            reason: "no silence detected; cannot derive speech segments".into(),
        });
    }

    let speech_starts = compute_speech_starts(&silence_intervals);
    if speech_starts.is_empty() {
        return Err(SubtitleError::VoiceAnalysisFailed {
            reason: "no speech segments derived from silence intervals".into(),
        });
    }

    let cue_starts = parse_srt_cue_starts(subtitle_srt);
    if cue_starts.is_empty() {
        return Err(SubtitleError::VoiceAnalysisFailed {
            reason: "no subtitle cues found in SRT content".into(),
        });
    }

    let result = cross_correlate(&speech_starts, &cue_starts);

    Ok(VoiceAlignmentResult {
        offset_ms: result.offset_ms,
        confidence: result.confidence,
        speech_segments: speech_starts.len(),
        subtitle_cues: cue_starts.len(),
    })
}

fn paddleocr_available() -> bool {
    if Command::new("paddleocr")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success())
    {
        return true;
    }
    Command::new("python3")
        .args(["-m", "paddleocr", "--version"])
        .output()
        .is_ok_and(|o| o.status.success())
}

fn tesseract_available() -> bool {
    Command::new("tesseract")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success())
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

fn srt_timestamp_to_ass(ts: &str) -> String {
    let ms = parse_timecode_to_ms(ts.trim(), ',');
    let h = ms / 3_600_000;
    let m = (ms % 3_600_000) / 60_000;
    let s = (ms % 60_000) / 1000;
    let cs = (ms % 1000) / 10;
    format!("{h}:{m:02}:{s:02}.{cs:02}")
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

fn rescale_timecode_line(line: &str, separator: char, scale: f64) -> String {
    let parts: Vec<&str> = line.split("-->").collect();
    if parts.len() != 2 {
        return line.to_string();
    }

    let start = parts[0].trim();
    let end = parts[1].trim();

    let start_ms = parse_timecode_to_ms(start, separator);
    let end_ms = parse_timecode_to_ms(end, separator);

    let new_start = ((start_ms as f64) * scale).round() as u64;
    let new_end = ((end_ms as f64) * scale).round() as u64;

    let start_str = ms_to_timecode(new_start, separator);
    let end_str = ms_to_timecode(new_end, separator);

    format!("{start_str} --> {end_str}")
}

fn detect_separator(content: &str) -> char {
    for line in content.lines() {
        if line.contains("-->") {
            if line.contains(',') {
                return ',';
            }
            if line.contains('.') {
                return '.';
            }
        }
    }
    ','
}

fn split_srt_timecode(line: &str) -> Option<(String, String)> {
    let parts: Vec<&str> = line.split("-->").collect();
    if parts.len() != 2 {
        return None;
    }
    Some((parts[0].trim().to_string(), parts[1].trim().to_string()))
}

async fn run_silencedetect(media_path: &Path) -> Result<Vec<(u64, u64)>, SubtitleError> {
    let output = AsyncCommand::new("ffmpeg")
        .arg("-hide_banner")
        .arg("-i")
        .arg(media_path)
        .arg("-af")
        .arg("silencedetect=noise=-30dB:d=0.5")
        .arg("-f")
        .arg("null")
        .arg("-")
        .output()
        .await
        .map_err(|e| SubtitleError::VoiceAnalysisFailed {
            reason: format!("ffmpeg spawn failed: {e}"),
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(SubtitleError::VoiceAnalysisFailed {
            reason: format!("ffmpeg silencedetect failed: {}", stderr.trim()),
        });
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    Ok(parse_silence_intervals(&stderr))
}

fn parse_silence_intervals(stderr: &str) -> Vec<(u64, u64)> {
    let mut intervals: Vec<(u64, u64)> = Vec::new();
    let mut current_start: Option<u64> = None;

    for line in stderr.lines() {
        let trimmed = line.trim();
        if let Some(pos) = trimmed.find("silence_start:") {
            let value_str = trimmed[pos + "silence_start:".len()..]
                .split_whitespace()
                .next()
                .unwrap_or("0");
            let secs: f64 = value_str.parse().unwrap_or(0.0);
            current_start = Some((secs * 1000.0).round() as u64);
        } else if let Some(pos) = trimmed.find("silence_end:") {
            let value_str = trimmed[pos + "silence_end:".len()..]
                .split_whitespace()
                .next()
                .unwrap_or("0");
            let secs: f64 = value_str.parse().unwrap_or(0.0);
            let end_ms = (secs * 1000.0).round() as u64;
            if let Some(start) = current_start.take() {
                intervals.push((start, end_ms));
            }
        }
    }

    intervals
}

fn compute_speech_starts(silence_intervals: &[(u64, u64)]) -> Vec<u64> {
    let mut starts: Vec<u64> = Vec::new();

    if silence_intervals.is_empty() {
        return starts;
    }

    if silence_intervals[0].0 > 0 {
        starts.push(0);
    }

    for window in silence_intervals.windows(2) {
        let prev_end = window[0].1;
        starts.push(prev_end);
    }

    starts.push(silence_intervals.last().expect("non-empty").1);

    starts
}

fn parse_srt_cue_starts(srt: &str) -> Vec<u64> {
    let mut starts: Vec<u64> = Vec::new();

    for block in srt.split("\n\n") {
        let block = block.trim();
        if block.is_empty() {
            continue;
        }

        for line in block.lines() {
            if let Some(pos) = line.find("-->") {
                let start_tc = line[..pos].trim();
                starts.push(parse_timecode_to_ms(start_tc, ','));
                break;
            }
        }
    }

    starts
}

struct CrossCorrelationResult {
    offset_ms: i32,
    confidence: f64,
}

fn cross_correlate(speech_starts: &[u64], cue_starts: &[u64]) -> CrossCorrelationResult {
    const MIN_OFFSET_MS: i32 = -30_000;
    const MAX_OFFSET_MS: i32 = 30_000;
    const STEP_MS: i32 = 250;
    const TOLERANCE_MS: i64 = 1_000;

    let mut best_offset = 0i32;
    let mut best_count: u64 = 0;
    let mut counts: Vec<u64> = Vec::new();

    let mut offset = MIN_OFFSET_MS;
    while offset <= MAX_OFFSET_MS {
        let mut count: u64 = 0;
        for cue in cue_starts {
            let adjusted_cue = (*cue as i64) + (offset as i64);
            for speech in speech_starts {
                if ((adjusted_cue - (*speech as i64)).abs()) <= TOLERANCE_MS {
                    count += 1;
                    break;
                }
            }
        }
        counts.push(count);
        if count > best_count || (count == best_count && offset.abs() < best_offset.abs()) {
            best_count = count;
            best_offset = offset;
        }
        offset += STEP_MS;
    }

    let mean: f64 = if counts.is_empty() {
        0.0
    } else {
        counts.iter().map(|&c| c as f64).sum::<f64>() / counts.len() as f64
    };

    let confidence = if mean > 0.0 {
        (best_count as f64) / mean
    } else {
        0.0
    };

    CrossCorrelationResult {
        offset_ms: best_offset,
        confidence,
    }
}

const CHUNK_SIZE: u64 = 65536;
const MIN_OSHASH_FILE_SIZE: u64 = 131072;

pub async fn compute_oshash(path: &Path) -> Result<String, std::io::Error> {
    use tokio::io::{AsyncReadExt, AsyncSeekExt};

    let mut file = tokio::fs::File::open(path).await?;
    let metadata = file.metadata().await?;
    let file_size = metadata.len();

    if file_size < MIN_OSHASH_FILE_SIZE {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "file too small for OSHash (minimum 128 KB)",
        ));
    }

    let mut hash: u64 = file_size;

    let mut first_chunk = vec![0u8; CHUNK_SIZE as usize];
    file.seek(std::io::SeekFrom::Start(0)).await?;
    file.read_exact(&mut first_chunk).await?;

    for chunk in first_chunk.chunks_exact(8) {
        let val = u64::from_le_bytes([
            chunk[0], chunk[1], chunk[2], chunk[3], chunk[4], chunk[5], chunk[6], chunk[7],
        ]);
        hash = hash.wrapping_add(val);
    }

    let mut last_chunk = vec![0u8; CHUNK_SIZE as usize];
    file.seek(std::io::SeekFrom::Start(file_size - CHUNK_SIZE))
        .await?;
    file.read_exact(&mut last_chunk).await?;

    for chunk in last_chunk.chunks_exact(8) {
        let val = u64::from_le_bytes([
            chunk[0], chunk[1], chunk[2], chunk[3], chunk[4], chunk[5], chunk[6], chunk[7],
        ]);
        hash = hash.wrapping_add(val);
    }

    Ok(format!("{hash:016x}"))
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
    }

    #[test]
    fn test_srt_to_ass_roundtrip() {
        let srt = "1\n00:00:01,000 --> 00:00:04,000\nHello world\n";
        let ass = srt_to_ass(srt);
        assert!(ass.contains("[Script Info]"));
        assert!(ass.contains("[V4+ Styles]"));
        assert!(ass.contains("[Events]"));
        assert!(ass.contains("Dialogue: 0,0:00:01.00,0:00:04.00"));
        assert!(ass.contains("Hello world"));
    }

    #[test]
    fn test_ass_timestamp_to_srt() {
        assert_eq!(ass_timestamp_to_srt("0:00:01.00"), "00:00:01,000");
        assert_eq!(ass_timestamp_to_srt("1:23:45.67"), "01:23:45,670");
        assert_eq!(ass_timestamp_to_srt("0:05:30.50"), "00:05:30,500");
    }

    #[test]
    fn test_srt_timestamp_to_ass() {
        assert_eq!(srt_timestamp_to_ass("00:00:01,000"), "0:00:01.00");
        assert_eq!(srt_timestamp_to_ass("01:23:45,670"), "1:23:45.67");
    }

    #[test]
    fn test_strip_ass_override_tags() {
        assert_eq!(strip_ass_override_tags("{\\b1}Bold{\\b0}"), "Bold");
        assert_eq!(strip_ass_override_tags("Plain text"), "Plain text");
        assert_eq!(strip_ass_override_tags("{\\an8}Top{\\an7}Left"), "TopLeft");
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
    fn test_adjust_fps_pal_to_ntsc() {
        let srt = "1\n00:00:00,000 --> 00:01:40,000\nText\n";
        let adjusted = adjust_fps(srt, 25.0, 23.976);
        assert!(adjusted.contains(" --> 00:01:44,"));
    }

    #[test]
    fn test_adjust_fps_ntsc_to_pal() {
        let srt = "1\n00:00:00,000 --> 00:01:40,000\nText\n";
        let adjusted = adjust_fps(srt, 23.976, 25.0);
        assert!(adjusted.contains(" --> 00:01:35,"));
    }

    #[test]
    fn test_adjust_fps_equal_fps_noop() {
        let srt = "1\n00:00:01,000 --> 00:00:04,000\nText\n";
        let adjusted = adjust_fps(srt, 24.0, 24.0);
        assert_eq!(adjusted, srt);
    }

    #[test]
    fn test_adjust_fps_zero_fps_noop() {
        let srt = "1\n00:00:01,000 --> 00:00:04,000\nText\n";
        let adjusted = adjust_fps(srt, 0.0, 24.0);
        assert_eq!(adjusted, srt);
    }

    #[test]
    fn test_adjust_fps_vtt_format() {
        let vtt = "WEBVTT\n\n00:00:00.000 --> 00:01:40.000\nText\n";
        let adjusted = adjust_fps(vtt, 25.0, 23.976);
        assert!(adjusted.contains(" --> 00:01:44."));
    }

    #[test]
    fn test_detect_separator_srt() {
        let srt = "1\n00:00:01,000 --> 00:00:04,000\nText\n";
        assert_eq!(detect_separator(srt), ',');
    }

    #[test]
    fn test_detect_separator_vtt() {
        let vtt = "WEBVTT\n\n00:00:01.000 --> 00:00:04.000\nText\n";
        assert_eq!(detect_separator(vtt), '.');
    }

    #[test]
    fn test_parse_silence_intervals() {
        let stderr = "[silencedetect @ 0x...] silence_start: 1.5\n\
[silencedetect @ 0x...] silence_end: 2.5 | silence_duration: 1\n\
[silencedetect @ 0x...] silence_start: 10.0\n\
[silencedetect @ 0x...] silence_end: 11.0 | silence_duration: 1\n";
        let intervals = parse_silence_intervals(stderr);
        assert_eq!(intervals, vec![(1500, 2500), (10000, 11000)]);
    }

    #[test]
    fn test_compute_speech_starts() {
        let silence = vec![(1500, 2500), (10000, 11000)];
        let starts = compute_speech_starts(&silence);
        assert_eq!(starts, vec![0, 2500, 11000]);
    }

    #[test]
    fn test_compute_speech_starts_no_initial_silence() {
        let silence = vec![(0, 1000), (5000, 6000)];
        let starts = compute_speech_starts(&silence);
        assert_eq!(starts, vec![1000, 6000]);
    }

    #[test]
    fn test_parse_srt_cue_starts() {
        let srt =
            "1\n00:00:01,000 --> 00:00:04,000\nHello\n\n2\n00:00:05,000 --> 00:00:09,000\nWorld\n";
        let starts = parse_srt_cue_starts(srt);
        assert_eq!(starts, vec![1000, 5000]);
    }

    #[test]
    fn test_cross_correlate_perfect_match() {
        let speech = vec![1000u64, 5000, 10000];
        let cues = vec![1000u64, 5000, 10000];
        let result = cross_correlate(&speech, &cues);
        assert_eq!(result.offset_ms, 0);
        assert!(result.confidence > 1.0);
    }

    #[test]
    fn test_cross_correlate_with_offset() {
        let speech = vec![3000u64, 7000, 12000];
        let cues = vec![1000u64, 5000, 10000];
        let result = cross_correlate(&speech, &cues);
        assert!(
            (1000..=2000).contains(&result.offset_ms),
            "expected offset within tolerance band of true +2000ms, got {}",
            result.offset_ms
        );
        assert!(result.confidence > 1.0);
    }

    #[test]
    fn test_to_srt_dispatch() {
        let srt = "1\n00:00:01,000 --> 00:00:04,000\nText\n";
        assert_eq!(to_srt(srt, "srt"), srt);

        let vtt = "00:00:01.000 --> 00:00:04.000\nText\n";
        let result = to_srt(vtt, "vtt");
        assert!(result.contains("00:00:01,000 --> 00:00:04,000"));
    }

    #[test]
    fn test_ocr_engine_as_str() {
        assert_eq!(OcrEngine::PaddleOcr.as_str(), "paddleocr");
        assert_eq!(OcrEngine::Tesseract.as_str(), "tesseract");
    }
}

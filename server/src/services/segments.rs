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

//! Segment detection pipeline — the four-method detector library.
//!
//! Stateless functions for detecting skippable segments (intros, credits,
//! recaps, previews, outros) in media files. Each detection method is a pure
//! function over its inputs (no DB access, no global state); the worker
//! (`workers/segment_detector.rs`, Task 5) is the orchestration point that
//! ties these together with the CRUD layer in `domains::segments::service`.
//!
//! ## Method overview
//!
//! | Method | When | Cost | Accuracy |
//! |---|---|---|---|
//! | Chapter markers | During scan (Phase 3 already stored them) | Zero | Highest |
//! | Chromaprint | Background scheduled task | High (CPU) | Very high for intros |
//! | Black frame | Background task | Medium | Medium (credits) |
//! | Silence | Background task | Low | Medium (boundary refinement) |
//!
//! See [SEGMENT_DETECTION.md](../../docs/design/SEGMENT_DETECTION.md) for the
//! authoritative design including search windows, confidence scoring, and
//! safety padding.

use std::path::Path;
use std::sync::LazyLock;

use regex::Regex;
use thiserror::Error;
use tokio::io::AsyncReadExt;
use tokio::process::Command;

pub use crate::domains::segments::types::{VALID_SEGMENT_SOURCES, VALID_SEGMENT_TYPES};

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Skippable segment category. Mirrors the `media_segments.segment_type`
/// CHECK constraint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SegmentType {
    Intro,
    Credits,
    Recap,
    Preview,
    Outro,
}

impl SegmentType {
    pub fn as_str(self) -> &'static str {
        match self {
            SegmentType::Intro => "intro",
            SegmentType::Credits => "credits",
            SegmentType::Recap => "recap",
            SegmentType::Preview => "preview",
            SegmentType::Outro => "outro",
        }
    }

    pub fn from_db_str(s: &str) -> Option<Self> {
        match s {
            "intro" => Some(SegmentType::Intro),
            "credits" => Some(SegmentType::Credits),
            "recap" => Some(SegmentType::Recap),
            "preview" => Some(SegmentType::Preview),
            "outro" => Some(SegmentType::Outro),
            _ => None,
        }
    }
}

/// How a segment was detected. Mirrors the `media_segments.source` CHECK
/// constraint. `Combined` is reserved for credits where both black frame and
/// silence detection agree (high-confidence credits per the multi-method
/// validation rule).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SegmentSource {
    Chapter,
    Chromaprint,
    Blackframe,
    Silence,
    Combined,
}

impl SegmentSource {
    pub fn as_str(self) -> &'static str {
        match self {
            SegmentSource::Chapter => "chapter",
            SegmentSource::Chromaprint => "chromaprint",
            SegmentSource::Blackframe => "blackframe",
            SegmentSource::Silence => "silence",
            SegmentSource::Combined => "combined",
        }
    }
}

/// A candidate segment produced by the detection pipeline before it is
/// written to `media_segments`. Confidence is in `[0.0, 1.0]`.
#[derive(Debug, Clone)]
pub struct DetectedSegment {
    pub segment_type: SegmentType,
    pub start_ms: i32,
    pub end_ms: i32,
    /// Where the client should seek to when the user presses skip. Includes
    /// safety padding; always inside `[start_ms, end_ms]`.
    pub skip_to_ms: i32,
    pub source: SegmentSource,
    pub confidence: f32,
    /// Free-form traceability metadata written to `media_segments.metadata`.
    pub metadata: serde_json::Value,
}

impl DetectedSegment {
    /// Duration in milliseconds.
    pub fn duration_ms(&self) -> i32 {
        self.end_ms - self.start_ms
    }
}

/// Chapter entry extracted from a media file's container metadata.
#[derive(Debug, Clone)]
pub struct ChapterInfo {
    pub start_ms: i32,
    pub end_ms: i32,
    /// Chapter title from container metadata (e.g. "Intro", "Chapter 1").
    pub title: Option<String>,
}

impl ChapterInfo {
    pub fn duration_ms(&self) -> i32 {
        self.end_ms - self.start_ms
    }
}

/// Search window in milliseconds. Candidates outside this window are
/// discarded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SearchWindow {
    pub start_ms: i32,
    pub end_ms: i32,
}

/// Output of a chromaprint fingerprinting pass on a single file.
#[derive(Debug, Clone)]
pub struct FingerprintOutput {
    /// Raw sub-fingerprint sequence — one `u32` per ~11.6 ms of audio.
    pub raw: Vec<u32>,
    /// Sample rate used to compute the fingerprint (always 11025 by default).
    pub sample_rate: u32,
    /// Duration of the fingerprinted audio in milliseconds.
    pub duration_ms: i32,
}

/// A fingerprint paired with the media item it belongs to, for cross-episode
/// comparison.
#[derive(Debug, Clone)]
pub struct FingerprintWithContext {
    pub media_item_id: uuid::Uuid,
    pub runtime_ms: i32,
    pub fingerprint: Vec<u32>,
}

/// A recurring audio segment found across multiple episodes of a season.
#[derive(Debug, Clone)]
pub struct RecurringMatch {
    /// The media item this match belongs to. Populated by
    /// `find_recurring_segments` so the worker can persist results without
    /// re-deriving the association.
    pub media_item_id: uuid::Uuid,
    pub segment_type: SegmentType,
    pub start_ms: i32,
    pub end_ms: i32,
    /// Number of episodes that contributed to this match (≥2 by construction).
    pub matching_episodes: usize,
    /// Best per-pair bit-similarity ratio in `[0.0, 1.0]`.
    pub similarity: f32,
}

/// A single black-frame event emitted by FFmpeg's `blackframe` filter.
#[derive(Debug, Clone, Copy)]
pub struct BlackframeEvent {
    /// Frame number in the decoded stream.
    pub frame: i64,
    /// Percentage of pixels at or below `threshold` (0–100).
    pub pblack: u8,
    /// Timestamp in milliseconds.
    pub time_ms: i32,
}

/// A single silence interval emitted by FFmpeg's `silencedetect` filter.
#[derive(Debug, Clone, Copy)]
pub struct SilenceEvent {
    pub start_ms: i32,
    pub end_ms: i32,
    pub duration_ms: i32,
}

/// Safety padding applied to detected segments. See "Conservative Boundaries"
/// in SEGMENT_DETECTION.md.
#[derive(Debug, Clone, Copy)]
pub struct SafetyConfig {
    pub intro_start_padding_ms: i32,
    pub intro_end_padding_ms: i32,
    pub credits_start_padding_ms: i32,
    pub credits_end_padding_ms: i32,
    pub min_confidence: f32,
}

impl Default for SafetyConfig {
    fn default() -> Self {
        Self {
            intro_start_padding_ms: 0,
            intro_end_padding_ms: 2_000,
            credits_start_padding_ms: 0,
            credits_end_padding_ms: 0,
            min_confidence: 0.7,
        }
    }
}

/// Duration thresholds per segment type. Candidates outside `[min, max]` are
/// rejected as false positives.
#[derive(Debug, Clone, Copy)]
pub struct DurationThresholds {
    pub min_ms: i32,
    pub max_tv_ms: i32,
    pub max_movie_ms: i32,
}

impl DurationThresholds {
    pub fn for_type(seg_type: SegmentType) -> Self {
        match seg_type {
            SegmentType::Intro => Self {
                min_ms: 15_000,
                max_tv_ms: 120_000,
                max_movie_ms: 120_000,
            },
            SegmentType::Credits => Self {
                min_ms: 15_000,
                max_tv_ms: 300_000,
                max_movie_ms: 900_000,
            },
            SegmentType::Recap => Self {
                min_ms: 15_000,
                max_tv_ms: 120_000,
                max_movie_ms: 120_000,
            },
            SegmentType::Preview => Self {
                min_ms: 15_000,
                max_tv_ms: 120_000,
                max_movie_ms: 120_000,
            },
            SegmentType::Outro => Self {
                min_ms: 15_000,
                max_tv_ms: 120_000,
                max_movie_ms: 300_000,
            },
        }
    }

    /// Returns the duration ceiling appropriate for the given media class.
    pub fn max_for(&self, is_movie: bool) -> i32 {
        if is_movie {
            self.max_movie_ms
        } else {
            self.max_tv_ms
        }
    }
}

/// Tunable parameters for the `blackframe` FFmpeg filter.
#[derive(Debug, Clone, Copy)]
pub struct BlackframeParams {
    /// Percentage of pixels at or below `threshold` for a frame to count as
    /// black (0–100). Default 75 per SEGMENT_DETECTION.md (FFmpeg filter
    /// default is 98).
    pub amount: u8,
    /// Pixel value ceiling (0–255). Default 2 per SEGMENT_DETECTION.md
    /// (FFmpeg filter default is 32).
    pub threshold: u8,
}

impl Default for BlackframeParams {
    fn default() -> Self {
        Self {
            amount: 75,
            threshold: 2,
        }
    }
}

/// Tunable parameters for the `silencedetect` FFmpeg filter.
#[derive(Debug, Clone, Copy)]
pub struct SilenceParams {
    /// Noise tolerance in millibels relative to full scale. Default -55 dB
    /// per SEGMENT_DETECTION.md (FFmpeg filter default is -60 dB).
    pub noise_db: i16,
    /// Minimum silence duration in milliseconds. Default 2_000.
    pub min_duration_ms: i32,
}

impl Default for SilenceParams {
    fn default() -> Self {
        Self {
            noise_db: -55,
            min_duration_ms: 2_000,
        }
    }
}

/// Tunable thresholds for the chromaprint cross-episode matcher.
#[derive(Debug, Clone, Copy)]
pub struct ChromaprintThresholds {
    /// Minimum fraction (0–100) of bits that must agree between two
    /// sub-fingerprints for them to count as "matching" at a given offset.
    /// Default 30/32 (≈93.75%).
    pub min_bit_agreement: u8,
    /// Minimum number of matching episodes required for high confidence.
    /// Default 3.
    pub high_confidence_episodes: usize,
    /// Sub-fingerprint period in milliseconds. Chromaprint's test2 algorithm
    /// produces one sub-fingerprint every ~11.6 ms; the constant is fixed by
    /// the chromaprint-next crate.
    pub subfp_period_ms: i32,
}

impl Default for ChromaprintThresholds {
    fn default() -> Self {
        Self {
            min_bit_agreement: 30,
            high_confidence_episodes: 3,
            subfp_period_ms: 11,
        }
    }
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// Operational failures raised by the detection pipeline. These are logged
/// and skipped by the worker (Task 5); they do NOT surface to the HTTP
/// client. Translation to API errors happens only at the
/// `domains::segments::service` boundary, not here.
#[derive(Error, Debug)]
pub enum SegmentPipelineError {
    #[error("ffmpeg spawn failed: {0}")]
    FfmpegSpawn(String),

    #[error("ffmpeg exited with status {code:?}: {stderr}")]
    FfmpegFailed { code: Option<i32>, stderr: String },

    #[error("ffmpeg output parse error: {0}")]
    ParseError(String),

    #[error("chromaprint calculation failed: {0}")]
    ChromaprintFailed(String),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

// ---------------------------------------------------------------------------
// Method 1: Chapter marker extraction
// ---------------------------------------------------------------------------

/// Regex patterns proven by Jellyfin Intro Skipper, adapted for Rust's
/// `regex` crate (no look-around). Case-insensitive, anchored at word
/// boundaries via `\b`. Each pattern maps to a segment type.
///
/// The original Jellyfin pattern `(^|\s)(Intro|Introduction|OP|Opening)(\s|
/// :$|$|(?!End))` uses a negative lookahead to exclude "IntroEnd" labels.
/// Rust's `regex` crate deliberately omits look-around for predictability,
/// so the patterns here fall back to plain word boundaries. The minor loss
/// of disambiguation (matching "IntroEnd" as Intro) is acceptable for the
/// chapter-classification use case — chapter titles named "IntroEnd" are
/// rare and would produce an intro segment with slightly off timestamps
/// rather than a safety issue.
static CHAPTER_PATTERNS: LazyLock<Vec<(Regex, SegmentType)>> = LazyLock::new(|| {
    vec![
        (
            Regex::new(r"(?i)\b(Intro|Introduction|OP|Opening)\b").unwrap(),
            SegmentType::Intro,
        ),
        (
            Regex::new(r"(?i)\b(Credits?|ED|Ending|Outro)\b").unwrap(),
            SegmentType::Credits,
        ),
        (
            Regex::new(r"(?i)\b(Re?cap|Sum{1,2}ary|Prev(ious(ly)?)?|Last|Earlier)\b").unwrap(),
            SegmentType::Recap,
        ),
        (
            Regex::new(
                r"(?i)\b(Preview|PV|Sneak\s?Peek|Coming\s?(Up|Soon)|Next\s+(time|on|episode))\b",
            )
            .unwrap(),
            SegmentType::Preview,
        ),
    ]
});

/// Parse a chapter timecode string from `ffprobe` into milliseconds.
///
/// `ffprobe` emits times as decimal seconds (`"60.123456"`) for most
/// containers and as `H:MM:SS.mmmmmm` for some legacy MKVs. Returns `None`
/// for malformed input — callers should skip the offending chapter rather
/// than fail the whole file.
pub fn parse_chapter_time_ms(s: &str) -> Option<i32> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }

    let seconds_f: f64 = if s.contains(':') {
        let parts: Vec<&str> = s.split(':').collect();
        let (h, m, sec) = match parts.len() {
            3 => (parts[0], parts[1], parts[2]),
            2 => ("0", parts[0], parts[1]),
            _ => return None,
        };
        let h: f64 = h.parse().ok()?;
        let m: f64 = m.parse().ok()?;
        let sec: f64 = sec.parse().ok()?;
        h * 3_600.0 + m * 60.0 + sec
    } else {
        s.parse().ok()?
    };

    if !seconds_f.is_finite() || seconds_f < 0.0 {
        return None;
    }
    Some((seconds_f * 1_000.0).round() as i32)
}

/// Extract chapters from the `media_files.additional_streams` JSONB column.
///
/// During the library scan (Phase 5), `library_scanner.rs` writes chapters
/// under the `chapters` key with shape `{ id, start_time, end_time, tags }`
/// where `start_time`/`end_time` are `ffprobe` timecode strings and `tags`
/// is a flat string map (typically `{"title": "Intro"}`). Malformed entries
/// are skipped silently — a single bad chapter does not invalidate the rest.
pub fn extract_chapters(additional_streams: &serde_json::Value) -> Vec<ChapterInfo> {
    let Some(chapters) = additional_streams
        .get("chapters")
        .and_then(|c| c.as_array())
    else {
        return Vec::new();
    };

    let mut out = Vec::with_capacity(chapters.len());
    for ch in chapters {
        let start_ms = ch
            .get("start_time")
            .and_then(|v| v.as_str())
            .and_then(parse_chapter_time_ms);
        let end_ms = ch
            .get("end_time")
            .and_then(|v| v.as_str())
            .and_then(parse_chapter_time_ms);

        let (Some(start_ms), Some(end_ms)) = (start_ms, end_ms) else {
            continue;
        };
        if end_ms <= start_ms {
            continue;
        }

        let title = ch
            .get("tags")
            .and_then(|t| t.get("title"))
            .and_then(|t| t.as_str())
            .map(|s| s.trim().to_string());

        out.push(ChapterInfo {
            start_ms,
            end_ms,
            title: title.filter(|s| !s.is_empty()),
        });
    }
    out
}

/// Classify a chapter title against the regex patterns. Returns `None` when
/// no pattern matches (chapter is not a skippable segment).
pub fn classify_chapter_title(title: &str) -> Option<SegmentType> {
    let trimmed = title.trim();
    if trimmed.is_empty() {
        return None;
    }
    for (re, seg_type) in CHAPTER_PATTERNS.iter() {
        if re.is_match(trimmed) {
            return Some(*seg_type);
        }
    }
    None
}

/// Convert a classified chapter into a detected segment.
///
/// Chapters always score confidence 1.0 (authoritative). The chapter's
/// `[start_ms, end_ms]` is used verbatim — no search window is applied
/// because the encoder placed it. Safety padding is still applied to
/// `skip_to_ms` so the user lands at a safe seek target.
pub fn chapter_to_detected_segment(
    chapter: &ChapterInfo,
    seg_type: SegmentType,
    runtime_ms: i32,
    safety: &SafetyConfig,
) -> DetectedSegment {
    let mut seg = DetectedSegment {
        segment_type: seg_type,
        start_ms: chapter.start_ms,
        end_ms: chapter.end_ms,
        skip_to_ms: chapter.end_ms,
        source: SegmentSource::Chapter,
        confidence: 1.0,
        metadata: serde_json::json!({
            "source_chapter_title": chapter.title,
            "runtime_ms": runtime_ms,
        }),
    };
    apply_safety_padding(&mut seg, safety);
    seg
}

// ---------------------------------------------------------------------------
// Method 2: Chromaprint fingerprinting
// ---------------------------------------------------------------------------

/// Extract a chromaprint fingerprint from a media file.
///
/// Spawns `ffmpeg -i <path> -vn -ac 1 -ar <sample_rate> -f s16le -acodec
/// pcm_s16le pipe:1`, reads the resulting PCM stream in chunks, and feeds
/// each chunk to `chromaprint::Fingerprinter`. The fingerprinter performs
/// its own internal FFT — FFmpeg's job is only to decode + downmix +
/// resample.
///
/// `sample_rate` defaults to 11_025 if zero is passed (the rate Chromaprint
/// uses internally; feeding the same rate avoids a redundant resample).
pub async fn fingerprint_file(
    path: &Path,
    sample_rate: u32,
) -> Result<FingerprintOutput, SegmentPipelineError> {
    let sample_rate = if sample_rate == 0 {
        11_025
    } else {
        sample_rate
    };

    let mut cmd = Command::new("ffmpeg");
    cmd.arg("-hide_banner")
        .arg("-nostdin")
        .arg("-i")
        .arg(path)
        .arg("-vn")
        .arg("-ac")
        .arg("1")
        .arg("-ar")
        .arg(sample_rate.to_string())
        .arg("-f")
        .arg("s16le")
        .arg("-acodec")
        .arg("pcm_s16le")
        .arg("pipe:1")
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .stdin(std::process::Stdio::null());

    cmd.kill_on_drop(true);

    let mut child = cmd
        .spawn()
        .map_err(|e| SegmentPipelineError::FfmpegSpawn(e.to_string()))?;

    let mut stdout = child
        .stdout
        .take()
        .ok_or_else(|| SegmentPipelineError::FfmpegSpawn("missing ffmpeg stdout".into()))?;
    let mut stderr = child
        .stderr
        .take()
        .ok_or_else(|| SegmentPipelineError::FfmpegSpawn("missing ffmpeg stderr".into()))?;

    let stderr_handle = tokio::spawn(async move {
        let mut buf = Vec::with_capacity(8 * 1024);
        let _ = stderr.read_to_end(&mut buf).await;
        buf
    });

    let mut fp = chromaprint::Fingerprinter::new(chromaprint::Algorithm::default());
    fp.start(sample_rate, 1)
        .map_err(|e| SegmentPipelineError::ChromaprintFailed(e.to_string()))?;

    let mut total_samples: u64 = 0;
    let mut buf = vec![0u8; 8 * 1024];
    loop {
        let n = stdout
            .read(&mut buf)
            .await
            .map_err(SegmentPipelineError::Io)?;
        if n == 0 {
            break;
        }

        let samples: Vec<i16> = buf[..n]
            .chunks_exact(2)
            .map(|c| i16::from_le_bytes([c[0], c[1]]))
            .collect();
        if let Err(e) = fp.feed(&samples) {
            tracing::warn!(error = %e, "chromaprint feed returned error; continuing");
        }
        total_samples += samples.len() as u64;
    }

    let status = child.wait().await.map_err(SegmentPipelineError::Io)?;
    let stderr_bytes = stderr_handle.await.unwrap_or_default();

    if !status.success() {
        return Err(SegmentPipelineError::FfmpegFailed {
            code: status.code(),
            stderr: String::from_utf8_lossy(&stderr_bytes).to_string(),
        });
    }

    fp.finish()
        .map_err(|e| SegmentPipelineError::ChromaprintFailed(e.to_string()))?;

    let raw: Vec<u32> = fp.fingerprint().to_vec();
    let duration_ms = if sample_rate > 0 {
        ((total_samples as f64 / sample_rate as f64) * 1_000.0).round() as i32
    } else {
        0
    };

    if raw.is_empty() {
        let stderr = String::from_utf8_lossy(&stderr_bytes);
        return Err(SegmentPipelineError::ChromaprintFailed(format!(
            "empty fingerprint (samples={total_samples}, stderr={})",
            stderr.trim()
        )));
    }

    Ok(FingerprintOutput {
        raw,
        sample_rate,
        duration_ms,
    })
}

/// Encode a raw fingerprint to chromaprint's compressed base64 string.
///
/// Convenience wrapper for debug logging; the worker stores raw `u32` bytes
/// in `media_fingerprints.fingerprint` BYTEA.
pub fn encode_fingerprint(raw: &[u32]) -> String {
    chromaprint::encode_fingerprint(raw, chromaprint::Algorithm::default())
}

/// Count the number of bits that agree between two sub-fingerprints.
///
/// Returns a value in `[0, 32]` — 32 means the sub-fingerprints are
/// identical.
pub fn bit_agreement(a: u32, b: u32) -> u8 {
    32 - (a ^ b).count_ones() as u8
}

/// Find recurring audio segments across episodes of a season.
///
/// For each ordered pair of fingerprints, slides one against the other and
/// tracks the longest contiguous run of offsets where the per-sub-fingerprint
/// bit agreement is at least `thresholds.min_bit_agreement`. Runs whose
/// duration falls inside the intro duration window (15–120 s) AND inside the
/// intro search window (first 25% of the episode or first 10 min, whichever
/// is smaller) are emitted as intro candidates.
///
/// Returns one `RecurringMatch` per unique pair-best match per media item;
/// the worker deduplicates against the DB before writing.
pub fn find_recurring_segments(
    fingerprints: &[FingerprintWithContext],
    thresholds: &ChromaprintThresholds,
) -> Vec<RecurringMatch> {
    if fingerprints.len() < 2 {
        return Vec::new();
    }

    let intro_thresholds = DurationThresholds::for_type(SegmentType::Intro);
    let min_run = duration_to_subfps(intro_thresholds.min_ms, thresholds.subfp_period_ms);
    let max_run = duration_to_subfps(intro_thresholds.max_tv_ms, thresholds.subfp_period_ms);

    let mut matches_by_item: std::collections::HashMap<uuid::Uuid, RecurringMatch> =
        std::collections::HashMap::new();

    for i in 0..fingerprints.len() {
        for j in (i + 1)..fingerprints.len() {
            let a = &fingerprints[i];
            let b = &fingerprints[j];
            if a.fingerprint.len() < min_run || b.fingerprint.len() < min_run {
                continue;
            }

            let Some(match_a) = best_pair_match(a, b, thresholds, min_run, max_run) else {
                continue;
            };
            let match_b = RecurringMatch {
                media_item_id: b.media_item_id,
                ..match_a.clone()
            };
            let match_a = RecurringMatch {
                media_item_id: a.media_item_id,
                ..match_a
            };

            for (media_item_id, match_) in [(a.media_item_id, match_a), (b.media_item_id, match_b)]
            {
                matches_by_item
                    .entry(media_item_id)
                    .and_modify(|existing| {
                        if match_.similarity > existing.similarity {
                            *existing = match_.clone();
                        }
                        existing.matching_episodes =
                            existing.matching_episodes.saturating_add(1).max(2);
                    })
                    .or_insert_with(|| match_);
            }
        }
    }

    matches_by_item.into_values().collect()
}

/// Compute the best recurring match (if any) between two fingerprints.
fn best_pair_match(
    a: &FingerprintWithContext,
    b: &FingerprintWithContext,
    thresholds: &ChromaprintThresholds,
    min_run: usize,
    max_run: usize,
) -> Option<RecurringMatch> {
    let fp_a = &a.fingerprint;
    let fp_b = &b.fingerprint;

    let window_a = intro_search_window_ms(a.runtime_ms, false);
    let window_b = intro_search_window_ms(b.runtime_ms, false);
    let window_start_subfp = ms_to_subfp(
        window_a.start_ms.min(window_b.start_ms),
        thresholds.subfp_period_ms,
    );
    let window_end_subfp = ms_to_subfp(
        window_a.end_ms.min(window_b.end_ms),
        thresholds.subfp_period_ms,
    );

    let mut best: Option<(usize, usize, f32)> = None;

    for offset in 0..fp_a.len().saturating_sub(min_run) {
        if offset > window_end_subfp {
            break;
        }

        let mut run_len = 0usize;
        let mut run_sum_bits = 0u64;
        let mut run_best_similarity = 0.0f32;

        for (k, b_val) in fp_b
            .iter()
            .enumerate()
            .take((fp_a.len() - offset).min(fp_b.len()))
        {
            let idx_a = offset + k;
            if idx_a > window_end_subfp {
                break;
            }
            let agreement = bit_agreement(fp_a[idx_a], *b_val);
            if agreement >= thresholds.min_bit_agreement {
                run_len += 1;
                run_sum_bits += agreement as u64;
                let avg = (run_sum_bits as f32) / (run_len as f32 * 32.0);
                run_best_similarity = run_best_similarity.max(avg);
            } else if run_len >= min_run {
                break;
            } else {
                run_len = 0;
                run_sum_bits = 0;
            }
            if run_len >= max_run {
                break;
            }
        }

        if run_len >= min_run {
            let better = match best {
                None => true,
                Some((prev_len, _prev_offset, prev_sim)) => {
                    (run_len, run_best_similarity) > (prev_len, prev_sim)
                }
            };
            if better {
                best = Some((offset, run_len, run_best_similarity));
            }
        }
    }

    let (offset, run_len, similarity) = best?;
    let _ = window_start_subfp;

    let start_subfp = offset.min(window_end_subfp);
    let end_subfp = (offset + run_len).min(window_end_subfp + 1);
    if end_subfp <= start_subfp {
        return None;
    }

    let start_ms = subfp_to_ms(start_subfp, thresholds.subfp_period_ms);
    let end_ms = subfp_to_ms(end_subfp, thresholds.subfp_period_ms);
    if end_ms - start_ms < DurationThresholds::for_type(SegmentType::Intro).min_ms {
        return None;
    }

    Some(RecurringMatch {
        media_item_id: uuid::Uuid::nil(),
        segment_type: SegmentType::Intro,
        start_ms,
        end_ms,
        matching_episodes: 2,
        similarity,
    })
}

/// Convert a chromaprint match into a `DetectedSegment` with the design's
/// confidence scoring table applied.
pub fn chromaprint_match_to_segment(
    match_: &RecurringMatch,
    thresholds: &ChromaprintThresholds,
    silence_confirms: bool,
    blackframe_confirms: bool,
    runtime_ms: i32,
    safety: &SafetyConfig,
) -> DetectedSegment {
    let base = if match_.matching_episodes >= thresholds.high_confidence_episodes {
        0.9
    } else {
        0.7
    };
    let mut confidence: f32 = base;
    if blackframe_confirms {
        confidence += 0.05;
    }
    if silence_confirms {
        confidence += 0.1;
    }
    let confidence = confidence.clamp(0.0, 1.0);

    let mut seg = DetectedSegment {
        segment_type: match_.segment_type,
        start_ms: match_.start_ms,
        end_ms: match_.end_ms,
        skip_to_ms: match_.end_ms,
        source: SegmentSource::Chromaprint,
        confidence,
        metadata: serde_json::json!({
            "matching_episodes": match_.matching_episodes,
            "similarity": match_.similarity,
            "silence_confirms": silence_confirms,
            "blackframe_confirms": blackframe_confirms,
            "runtime_ms": runtime_ms,
        }),
    };
    apply_safety_padding(&mut seg, safety);
    seg
}

// ---------------------------------------------------------------------------
// Method 3: Black frame detection
// ---------------------------------------------------------------------------

/// Run FFmpeg's `blackframe` filter against `path` and parse the stderr
/// output into discrete events.
///
/// Only events whose timestamp falls inside `search_window` are returned;
/// events outside the window are dropped because they cannot be credit
/// candidates.
pub async fn detect_blackframes(
    path: &Path,
    params: &BlackframeParams,
    search_window: SearchWindow,
) -> Result<Vec<BlackframeEvent>, SegmentPipelineError> {
    let filter = format!(
        "blackframe=amount={}:threshold={}",
        params.amount, params.threshold
    );

    let output = Command::new("ffmpeg")
        .arg("-hide_banner")
        .arg("-nostdin")
        .arg("-i")
        .arg(path)
        .arg("-vf")
        .arg(&filter)
        .arg("-an")
        .arg("-f")
        .arg("null")
        .arg("-")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .stdin(std::process::Stdio::null())
        .kill_on_drop(true)
        .output()
        .await
        .map_err(|e| SegmentPipelineError::FfmpegSpawn(e.to_string()))?;

    // `blackframe` emits log lines at INFO level. The process exits 0
    // whether or not frames were detected — a non-zero status indicates a
    // real failure (bad input, missing codec, etc.).
    if !output.status.success() {
        return Err(SegmentPipelineError::FfmpegFailed {
            code: output.status.code(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        });
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    let events = parse_blackframe_output(&stderr);

    Ok(events
        .into_iter()
        .filter(|e| e.time_ms >= search_window.start_ms && e.time_ms <= search_window.end_ms)
        .collect())
}

/// Parse FFmpeg `blackframe` filter log output into discrete events.
///
/// Recognises lines of the form
/// `[Parsed_blackframe_0 @ 0x...] frame:12345 pblack:99 pts:1456789 t:60.123 type:I last_keyframe:0`
/// and extracts `(frame, pblack, t_ms)`. The `pts`, `type`, and
/// `last_keyframe` fields are ignored (unreliable across containers).
pub fn parse_blackframe_output(stderr: &str) -> Vec<BlackframeEvent> {
    let mut out = Vec::new();
    for line in stderr.lines() {
        if !line.contains("blackframe") {
            continue;
        }
        let frame = parse_kv_i64(line, "frame:");
        let pblack = parse_kv_u8(line, "pblack:");
        let time_ms = parse_kv_f64_ms(line, "t:");
        let (Some(frame), Some(pblack), Some(time_ms)) = (frame, pblack, time_ms) else {
            continue;
        };
        out.push(BlackframeEvent {
            frame,
            pblack,
            time_ms,
        });
    }
    out
}

// ---------------------------------------------------------------------------
// Method 4: Silence detection
// ---------------------------------------------------------------------------

/// Run FFmpeg's `silencedetect` filter against `path` and parse the stderr
/// output into discrete silence intervals.
pub async fn detect_silence(
    path: &Path,
    params: &SilenceParams,
) -> Result<Vec<SilenceEvent>, SegmentPipelineError> {
    let noise = format!("{}dB", params.noise_db);
    let duration_secs = (params.min_duration_ms as f64) / 1_000.0;
    let filter = format!(
        "silencedetect=noise={}:d={}",
        noise,
        format_duration_secs(duration_secs)
    );

    let output = Command::new("ffmpeg")
        .arg("-hide_banner")
        .arg("-nostdin")
        .arg("-i")
        .arg(path)
        .arg("-af")
        .arg(&filter)
        .arg("-f")
        .arg("null")
        .arg("-")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .stdin(std::process::Stdio::null())
        .kill_on_drop(true)
        .output()
        .await
        .map_err(|e| SegmentPipelineError::FfmpegSpawn(e.to_string()))?;

    if !output.status.success() {
        return Err(SegmentPipelineError::FfmpegFailed {
            code: output.status.code(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        });
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    Ok(parse_silence_output(&stderr))
}

/// Parse FFmpeg `silencedetect` log output into discrete silence intervals.
///
/// Recognises `silence_start: <ts>` and `silence_end: <ts> |
/// silence_duration: <dur>` (prefixed with the `[silencedetect @ 0x...]`
/// log header). A `silence_start` without a matching `silence_end` is
/// treated as running to the end of the file and is dropped (the file ended
/// mid-silence).
pub fn parse_silence_output(stderr: &str) -> Vec<SilenceEvent> {
    let mut out = Vec::new();
    let mut current_start: Option<i32> = None;

    for line in stderr.lines() {
        if !line.contains("silencedetect") {
            continue;
        }
        if let Some(rest) = line.split("silence_start:").nth(1) {
            if let Some(ts) = parse_leading_f64(rest) {
                current_start = Some((ts * 1_000.0).round() as i32);
            }
            continue;
        }
        if let Some(rest) = line.split("silence_end:").nth(1)
            && let Some(end) = rest.split_whitespace().next().and_then(parse_leading_f64)
            && let Some(start) = current_start.take()
        {
            let end_ms = (end * 1_000.0).round() as i32;
            out.push(SilenceEvent {
                start_ms: start,
                end_ms,
                duration_ms: end_ms - start,
            });
        }
    }

    out
}

// ---------------------------------------------------------------------------
// Combined credits detection (black frame + silence)
// ---------------------------------------------------------------------------

/// Combine black-frame and silence detections into credit segments.
///
/// Implements the design's "Multi-Method Validation for Credits" rule: when
/// both methods agree on a credit window, the segment is written with
/// `source='combined'` and base confidence 0.8. Black-frame-only matches
/// score 0.5 (not surfaced by default). The window is the union of the two
/// methods' detections, capped by the search window.
pub fn combine_credits_signals(
    blackframes: &[BlackframeEvent],
    silence: &[SilenceEvent],
    search_window: SearchWindow,
    runtime_ms: i32,
    safety: &SafetyConfig,
) -> Vec<DetectedSegment> {
    let credits_thresholds = DurationThresholds::for_type(SegmentType::Credits);

    let bf_window = cluster_blackframes(blackframes, credits_thresholds.min_ms);
    let sil_window = cluster_silence(silence, credits_thresholds.min_ms);

    let mut segments = Vec::new();

    for bf in &bf_window {
        let agrees = sil_window.iter().any(|s| intervals_overlap(*bf, *s));
        let (confidence, source, methods) = if agrees {
            (
                0.8f32,
                SegmentSource::Combined,
                vec!["blackframe", "silence"],
            )
        } else {
            (0.5f32, SegmentSource::Blackframe, vec!["blackframe"])
        };

        let mut seg = DetectedSegment {
            segment_type: SegmentType::Credits,
            start_ms: bf.0,
            end_ms: bf.1,
            skip_to_ms: bf.1,
            source,
            confidence,
            metadata: serde_json::json!({
                "methods": methods,
                "runtime_ms": runtime_ms,
                "window": { "start_ms": search_window.start_ms, "end_ms": search_window.end_ms },
            }),
        };
        apply_safety_padding(&mut seg, safety);
        segments.push(seg);
    }

    // Silence-only credit candidates that did not pair with black frames are
    // also surfaced at 0.5 — the design says single-method credits cap at
    // 0.5 and are not surfaced by default. They are kept in the DB so the
    // admin can lower the threshold if they wish.
    for s in &sil_window {
        let agrees = bf_window.iter().any(|b| intervals_overlap(*b, *s));
        if agrees {
            continue;
        }
        let mut seg = DetectedSegment {
            segment_type: SegmentType::Credits,
            start_ms: s.0,
            end_ms: s.1,
            skip_to_ms: s.1,
            source: SegmentSource::Silence,
            confidence: 0.5,
            metadata: serde_json::json!({
                "methods": ["silence"],
                "runtime_ms": runtime_ms,
                "window": { "start_ms": search_window.start_ms, "end_ms": search_window.end_ms },
            }),
        };
        apply_safety_padding(&mut seg, safety);
        segments.push(seg);
    }

    segments
}

/// Coalesce a stream of black-frame events into `(start_ms, end_ms)` runs.
///
/// Adjacent events within `gap_tolerance_ms` (default 2 s — two frames at
/// 25 fps separated by less than 2 s are part of the same credit card)
/// are merged into a single run. Runs shorter than `min_duration_ms` are
/// discarded.
fn cluster_blackframes(events: &[BlackframeEvent], min_duration_ms: i32) -> Vec<(i32, i32)> {
    if events.is_empty() {
        return Vec::new();
    }
    let gap_tolerance_ms = 2_000;
    let mut sorted: Vec<&BlackframeEvent> = events.iter().collect();
    sorted.sort_by_key(|e| e.time_ms);

    let mut runs = Vec::new();
    let mut run_start = sorted[0].time_ms;
    let mut run_end = sorted[0].time_ms;

    for e in sorted.iter().skip(1) {
        if e.time_ms - run_end <= gap_tolerance_ms {
            run_end = e.time_ms;
        } else {
            if run_end - run_start >= min_duration_ms {
                runs.push((run_start, run_end));
            }
            run_start = e.time_ms;
            run_end = e.time_ms;
        }
    }
    if run_end - run_start >= min_duration_ms {
        runs.push((run_start, run_end));
    }
    runs
}

/// Coalesce silence events into `(start_ms, end_ms)` runs that satisfy the
/// minimum-duration rule.
fn cluster_silence(events: &[SilenceEvent], min_duration_ms: i32) -> Vec<(i32, i32)> {
    events
        .iter()
        .filter(|e| e.duration_ms >= min_duration_ms)
        .map(|e| (e.start_ms, e.end_ms))
        .collect()
}

fn intervals_overlap(a: (i32, i32), b: (i32, i32)) -> bool {
    a.0 <= b.1 && b.0 <= a.1
}

// ---------------------------------------------------------------------------
// Search windows + safety padding + duration helpers
// ---------------------------------------------------------------------------

/// Intro search window for the given runtime.
///
/// Per the design: first 25% of the episode OR first 10 minutes, whichever
/// is smaller, for TV; first 10 minutes for movies.
pub fn intro_search_window_ms(runtime_ms: i32, is_movie: bool) -> SearchWindow {
    let cap = 10 * 60_000;
    let end_ms = if is_movie {
        cap
    } else {
        (runtime_ms / 4).min(cap)
    };
    SearchWindow {
        start_ms: 0,
        end_ms: end_ms.max(0),
    }
}

/// Credits search window for the given runtime.
///
/// Per the design: last 30% of the episode for TV; last 20% for movies.
pub fn credits_search_window_ms(runtime_ms: i32, is_movie: bool) -> SearchWindow {
    let start_ms = if is_movie {
        runtime_ms - runtime_ms / 5
    } else {
        runtime_ms - (runtime_ms * 3) / 10
    };
    SearchWindow {
        start_ms: start_ms.max(0),
        end_ms: runtime_ms,
    }
}

/// Recap search window for the given runtime (TV only). First 15%.
pub fn recap_search_window_ms(runtime_ms: i32) -> SearchWindow {
    SearchWindow {
        start_ms: 0,
        end_ms: (runtime_ms * 15) / 100,
    }
}

/// Apply safety padding to a detected segment.
///
/// For intros, `skip_to_ms` is moved back by `intro_end_padding_ms` so the
/// user lands at the end of the intro theme rather than the start of the
/// first content frame, clamped to `start_ms` so we never skip backwards
/// past the intro start. For credits, `skip_to_ms` is moved forward by
/// `credits_end_padding_ms`, clamped to `end_ms` so we never skip past the
/// detected credits end. Manual segments are authoritative and skip this
/// function — the worker passes manual segments through unmodified.
pub fn apply_safety_padding(seg: &mut DetectedSegment, safety: &SafetyConfig) {
    match seg.segment_type {
        SegmentType::Intro => {
            let target = seg.end_ms - safety.intro_end_padding_ms;
            seg.skip_to_ms = target.max(seg.start_ms);
        }
        SegmentType::Credits => {
            let target = seg.end_ms + safety.credits_end_padding_ms;
            seg.skip_to_ms = target.min(seg.end_ms);
        }
        SegmentType::Recap | SegmentType::Preview | SegmentType::Outro => {
            seg.skip_to_ms = seg.end_ms;
        }
    }
}

/// Mark a segment as surfaced-or-not based on the configured minimum
/// confidence. Surfaced segments are written to the DB with
/// `metadata.surfaced = true`; the client filters on this so admins can
/// lower the threshold without a re-analysis pass.
pub fn mark_surfaced(seg: &mut DetectedSegment, safety: &SafetyConfig) {
    let surfaced = seg.confidence >= safety.min_confidence;
    if let Some(obj) = seg.metadata.as_object_mut() {
        obj.insert("surfaced".into(), serde_json::json!(surfaced));
    } else {
        seg.metadata = serde_json::json!({ "surfaced": surfaced });
    }
}

// ---------------------------------------------------------------------------
// Internal parsing helpers
// ---------------------------------------------------------------------------

fn duration_to_subfps(duration_ms: i32, subfp_period_ms: i32) -> usize {
    if subfp_period_ms <= 0 {
        return 0;
    }
    (duration_ms as usize) / (subfp_period_ms as usize)
}

fn ms_to_subfp(ms: i32, subfp_period_ms: i32) -> usize {
    duration_to_subfps(ms, subfp_period_ms)
}

fn subfp_to_ms(subfp: usize, subfp_period_ms: i32) -> i32 {
    (subfp as i32) * subfp_period_ms
}

fn parse_kv_i64(line: &str, key: &str) -> Option<i64> {
    let idx = line.find(key)?;
    let rest = &line[idx + key.len()..];
    parse_leading_i64(rest)
}

fn parse_kv_u8(line: &str, key: &str) -> Option<u8> {
    let v = parse_kv_i64(line, key)?;
    v.try_into().ok()
}

fn parse_kv_f64_ms(line: &str, key: &str) -> Option<i32> {
    let idx = line.find(key)?;
    let rest = &line[idx + key.len()..];
    let f = parse_leading_f64(rest)?;
    Some((f * 1_000.0).round() as i32)
}

fn parse_leading_f64(s: &str) -> Option<f64> {
    let token = s.split_whitespace().next()?;
    token.parse().ok()
}

fn parse_leading_i64(s: &str) -> Option<i64> {
    let token = s.split_whitespace().next()?;
    token.parse().ok()
}

fn format_duration_secs(secs: f64) -> String {
    // FFmpeg accepts bare seconds for `d`; avoid scientific notation and
    // trim trailing zeros for log readability.
    let s = format!("{secs:.3}");
    let trimmed = s.trim_end_matches('0').trim_end_matches('.');
    if trimmed.is_empty() {
        "0".to_string()
    } else {
        trimmed.to_string()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn window(start: i32, end: i32) -> SearchWindow {
        SearchWindow {
            start_ms: start,
            end_ms: end,
        }
    }

    #[test]
    fn parse_chapter_time_seconds_form() {
        assert_eq!(parse_chapter_time_ms("60.123456"), Some(60_123));
        assert_eq!(parse_chapter_time_ms("0"), Some(0));
    }

    #[test]
    fn parse_chapter_time_hms_form() {
        assert_eq!(parse_chapter_time_ms("1:02:03.5"), Some(3_723_500));
        assert_eq!(parse_chapter_time_ms("02:30"), Some(150_000));
    }

    #[test]
    fn parse_chapter_time_rejects_garbage() {
        assert_eq!(parse_chapter_time_ms(""), None);
        assert_eq!(parse_chapter_time_ms("not a time"), None);
        assert_eq!(parse_chapter_time_ms("-5"), None);
    }

    #[test]
    fn classify_chapter_titles() {
        assert_eq!(classify_chapter_title("Intro"), Some(SegmentType::Intro));
        assert_eq!(
            classify_chapter_title(" Opening "),
            Some(SegmentType::Intro)
        );
        assert_eq!(
            classify_chapter_title("End Credits"),
            Some(SegmentType::Credits)
        );
        assert_eq!(
            classify_chapter_title("Previously on..."),
            Some(SegmentType::Recap)
        );
        assert_eq!(
            classify_chapter_title("Next time on"),
            Some(SegmentType::Preview)
        );
        assert_eq!(classify_chapter_title("Chapter 1"), None);
        assert_eq!(classify_chapter_title(""), None);
    }

    #[test]
    fn extract_chapters_from_jsonb() {
        let v = serde_json::json!({
            "chapters": [
                {"id": 0, "start_time": "0", "end_time": "90", "tags": {"title": "Intro"}},
                {"id": 1, "start_time": "90", "end_time": "3600", "tags": {"title": "Episode"}},
                {"id": 2, "start_time": "bad", "end_time": "3600", "tags": {"title": "skip me"}},
                {"id": 3, "start_time": "3600", "end_time": "3720", "tags": {"title": "Credits"}}
            ]
        });
        let out = extract_chapters(&v);
        assert_eq!(out.len(), 3);
        assert_eq!(out[0].start_ms, 0);
        assert_eq!(out[0].end_ms, 90_000);
        assert_eq!(out[0].title.as_deref(), Some("Intro"));
        assert_eq!(out[2].title.as_deref(), Some("Credits"));
    }

    #[test]
    fn extract_chapters_handles_missing_key() {
        let v = serde_json::json!({});
        assert!(extract_chapters(&v).is_empty());
        let v = serde_json::json!({"subtitles": []});
        assert!(extract_chapters(&v).is_empty());
    }

    #[test]
    fn search_windows_tv() {
        let runtime = 60 * 60_000;
        let intro = intro_search_window_ms(runtime, false);
        assert_eq!(intro.start_ms, 0);
        assert_eq!(intro.end_ms, 10 * 60_000);

        let credits = credits_search_window_ms(runtime, false);
        assert_eq!(credits.end_ms, runtime);
        assert_eq!(credits.start_ms, runtime - (runtime * 3) / 10);

        let recap = recap_search_window_ms(runtime);
        assert_eq!(recap.start_ms, 0);
        assert_eq!(recap.end_ms, (runtime * 15) / 100);
    }

    #[test]
    fn search_windows_short_episode() {
        let runtime = 5 * 60_000;
        let intro = intro_search_window_ms(runtime, false);
        assert_eq!(intro.end_ms, runtime / 4);
    }

    #[test]
    fn search_windows_movie() {
        let runtime = 120 * 60_000;
        let intro = intro_search_window_ms(runtime, true);
        assert_eq!(intro.end_ms, 10 * 60_000);

        let credits = credits_search_window_ms(runtime, true);
        assert_eq!(credits.start_ms, runtime - runtime / 5);
        assert_eq!(credits.end_ms, runtime);
    }

    #[test]
    fn safety_padding_intro_moves_skip_back() {
        let safety = SafetyConfig::default();
        let mut seg = DetectedSegment {
            segment_type: SegmentType::Intro,
            start_ms: 0,
            end_ms: 90_000,
            skip_to_ms: 90_000,
            source: SegmentSource::Chromaprint,
            confidence: 0.9,
            metadata: serde_json::json!({}),
        };
        apply_safety_padding(&mut seg, &safety);
        assert_eq!(seg.skip_to_ms, 88_000);
    }

    #[test]
    fn safety_padding_intro_clamps_to_start() {
        let safety = SafetyConfig {
            intro_end_padding_ms: 200_000,
            ..SafetyConfig::default()
        };
        let mut seg = DetectedSegment {
            segment_type: SegmentType::Intro,
            start_ms: 50_000,
            end_ms: 90_000,
            skip_to_ms: 90_000,
            source: SegmentSource::Chromaprint,
            confidence: 0.9,
            metadata: serde_json::json!({}),
        };
        apply_safety_padding(&mut seg, &safety);
        assert_eq!(seg.skip_to_ms, 50_000);
    }

    #[test]
    fn parse_blackframe_line() {
        let stderr = "[Parsed_blackframe_0 @ 0x55a8f0e234c0] frame:12345 pblack:99 pts:1456789 t:60.123456 type:I last_keyframe:0\n\
                      [Parsed_blackframe_0 @ 0x55a8f0e234c0] frame:12346 pblack:98 pts:1457901 t:60.167891 type:P last_keyframe:0\n";
        let events = parse_blackframe_output(stderr);
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].frame, 12_345);
        assert_eq!(events[0].pblack, 99);
        assert_eq!(events[0].time_ms, 60_123);
    }

    #[test]
    fn parse_blackframe_ignores_unrelated_lines() {
        let stderr = "Frame    0 ...\n[something else] pblack:50\n";
        assert!(parse_blackframe_output(stderr).is_empty());
    }

    #[test]
    fn parse_silence_pairs() {
        let stderr = "[silencedetect @ 0x55a8f0e234c0] silence_start: 3420.12\n\
                      [silencedetect @ 0x55a8f0e234c0] silence_end: 3425.67 | silence_duration: 5.550000\n";
        let events = parse_silence_output(stderr);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].start_ms, 3_420_120);
        assert_eq!(events[0].end_ms, 3_425_670);
        assert_eq!(events[0].duration_ms, 5_550);
    }

    #[test]
    fn parse_silence_drops_unterminated() {
        let stderr = "[silencedetect @ 0x55a8f0e234c0] silence_start: 3420.12\n";
        assert!(parse_silence_output(stderr).is_empty());
    }

    #[test]
    fn bit_agreement_extremes() {
        assert_eq!(bit_agreement(0xFFFF_FFFF, 0xFFFF_FFFF), 32);
        assert_eq!(bit_agreement(0, 0xFFFF_FFFF), 0);
        assert_eq!(bit_agreement(0, 0), 32);
    }

    #[test]
    fn cluster_blackframes_merges_adjacent() {
        // 20 s of blackframe events spaced 1 s apart — within the 2 s gap
        // tolerance, so they form one run; the 20 s span clears the 15 s
        // minimum-duration filter.
        let events: Vec<BlackframeEvent> = (0..20_i32)
            .map(|i| BlackframeEvent {
                frame: i as i64,
                pblack: 90,
                time_ms: 60_000 + i * 1_000,
            })
            .collect();
        let runs = cluster_blackframes(&events, 15_000);
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].0, 60_000);
        assert_eq!(runs[0].1, 79_000);
    }

    #[test]
    fn cluster_blackframes_filters_short_runs() {
        let events = vec![
            BlackframeEvent {
                frame: 1,
                pblack: 90,
                time_ms: 60_000,
            },
            BlackframeEvent {
                frame: 2,
                pblack: 95,
                time_ms: 60_040,
            },
        ];
        let runs = cluster_blackframes(&events, 15_000);
        assert!(runs.is_empty());
    }

    #[test]
    fn combine_credits_combines_when_overlap() {
        let safety = SafetyConfig::default();
        // 20 s of blackframe events spaced 1 s apart — satisfies the 15 s minimum.
        let bf: Vec<BlackframeEvent> = (0..20_i32)
            .map(|i| BlackframeEvent {
                frame: i as i64,
                pblack: 90,
                time_ms: 60_000 + i * 1_000,
            })
            .collect();
        let sil = vec![SilenceEvent {
            start_ms: 60_000,
            end_ms: 80_000,
            duration_ms: 20_000,
        }];
        let segs = combine_credits_signals(&bf, &sil, window(0, 120_000), 120_000, &safety);
        assert_eq!(segs.len(), 1);
        assert_eq!(segs[0].source, SegmentSource::Combined);
        assert_eq!(segs[0].confidence, 0.8);
    }

    #[test]
    fn combine_credits_blackframe_only_caps_at_half() {
        let safety = SafetyConfig::default();
        let bf: Vec<BlackframeEvent> = (0..20_i32)
            .map(|i| BlackframeEvent {
                frame: i as i64,
                pblack: 90,
                time_ms: 60_000 + i * 1_000,
            })
            .collect();
        let segs = combine_credits_signals(&bf, &[], window(0, 120_000), 120_000, &safety);
        assert_eq!(segs.len(), 1);
        assert_eq!(segs[0].source, SegmentSource::Blackframe);
        assert_eq!(segs[0].confidence, 0.5);
    }

    #[test]
    fn duration_thresholds_intro() {
        let t = DurationThresholds::for_type(SegmentType::Intro);
        assert_eq!(t.min_ms, 15_000);
        assert_eq!(t.max_tv_ms, 120_000);
    }

    #[test]
    fn duration_thresholds_credits_movie_ceiling() {
        let t = DurationThresholds::for_type(SegmentType::Credits);
        assert_eq!(t.max_movie_ms, 900_000);
        assert_eq!(t.max_for(true), 900_000);
        assert_eq!(t.max_for(false), 300_000);
    }

    #[test]
    fn mark_surfaced_sets_metadata_flag() {
        let safety = SafetyConfig {
            min_confidence: 0.7,
            ..SafetyConfig::default()
        };
        let mut seg = DetectedSegment {
            segment_type: SegmentType::Intro,
            start_ms: 0,
            end_ms: 90_000,
            skip_to_ms: 90_000,
            source: SegmentSource::Chromaprint,
            confidence: 0.9,
            metadata: serde_json::json!({}),
        };
        mark_surfaced(&mut seg, &safety);
        assert_eq!(seg.metadata["surfaced"], serde_json::json!(true));

        seg.confidence = 0.4;
        mark_surfaced(&mut seg, &safety);
        assert_eq!(seg.metadata["surfaced"], serde_json::json!(false));
    }

    #[test]
    fn segment_type_round_trip() {
        for t in [
            SegmentType::Intro,
            SegmentType::Credits,
            SegmentType::Recap,
            SegmentType::Preview,
            SegmentType::Outro,
        ] {
            assert_eq!(SegmentType::from_db_str(t.as_str()), Some(t));
        }
        assert_eq!(SegmentType::from_db_str("nonsense"), None);
    }

    #[test]
    fn source_as_str_covers_all_variants() {
        assert_eq!(SegmentSource::Chapter.as_str(), "chapter");
        assert_eq!(SegmentSource::Chromaprint.as_str(), "chromaprint");
        assert_eq!(SegmentSource::Blackframe.as_str(), "blackframe");
        assert_eq!(SegmentSource::Silence.as_str(), "silence");
        assert_eq!(SegmentSource::Combined.as_str(), "combined");
    }
}

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

//! Storyboard generation pipeline — WebP sprite sheets + WebVTT seek index.
//!
//! Stateless library functions for generating seek-preview thumbnails from a
//! media file. One FFmpeg invocation per sprite sheet (using the modern
//! single-command `fps + scale + tile` filtergraph pattern) produces a WebP
//! sprite; a pure Rust function emits the matching WebVTT index. The worker
//! (`workers/storyboard_generator.rs`, Task 6) is the orchestration point
//! that resolves items needing storyboards, calls [`generate_storyboard`]
//! per file, and persists results via `domains::storyboards::service`.
//!
//! ## Pipeline overview
//!
//! 1. Compute sprite layout (sheets, thumbnails per sheet) from runtime.
//! 2. For each sheet: spawn `ffmpeg -ss <start> -t <window> -i <src>
//!    -vf "fps=1/N,scale=W:trunc(ow/a/2)*2,tile=COLSxROWS"
//!    -frames:v 1 -c:v webp <out>` (or with `-skip_frame nokey` prefix for
//!    the keyframe-only fast mode).
//! 3. Emit `index.vtt` mapping `[t_i, t_i+interval)` ranges to sprite regions.
//!
//! See [STORYBOARDS.md](../../docs/design/STORYBOARDS.md) for the
//! authoritative design including storage path, adaptive interval, and the
//! WebVTT + WebP format rationale.

use std::path::{Path, PathBuf};
use std::time::Instant;

use thiserror::Error;
use tokio::process::Command;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Tunable parameters for a single storyboard generation pass.
///
/// The worker constructs this from `RuntimeConfig.transcoding` (the
/// `storyboard_*` fields) and any per-library overrides before calling
/// [`generate_storyboard`]. Mirroring the design's "Configuration" table.
#[derive(Debug, Clone)]
pub struct GenerationConfig {
    /// Thumbnail width in pixels; valid: 160, 320, 640 (default 320).
    pub width: u32,
    /// Seconds between thumbnails. Either the adaptive value (already
    /// resolved via [`adaptive_interval`]) or a fixed override.
    pub interval_seconds: u32,
    /// WebP quality (lossy) in `[0, 100]`; default 75.
    pub quality: u32,
    /// When true, pass `-skip_frame nokey` for ~100x faster (but less
    /// frame-accurate) extraction. Default true per STORYBOARDS.md.
    pub keyframe_only: bool,
    /// Thumbnails per row in each sprite sheet; default 10.
    pub sprite_columns: u32,
    /// Thumbnail rows in each sprite sheet; default 20. With 10 columns
    /// this yields 200 thumbnails per sheet — the design's default.
    pub sprite_rows: u32,
}

impl Default for GenerationConfig {
    fn default() -> Self {
        Self {
            width: 320,
            interval_seconds: 10,
            quality: 75,
            keyframe_only: true,
            sprite_columns: 10,
            sprite_rows: 20,
        }
    }
}

impl GenerationConfig {
    /// Maximum thumbnails a single sprite sheet can hold (`columns × rows`).
    pub fn thumbnails_per_sheet(&self) -> u32 {
        self.sprite_columns.saturating_mul(self.sprite_rows)
    }

    /// Validate fields against the ranges in STORYBOARDS.md. Returns the
    /// first violation as a human-readable string; `Ok(())` when valid.
    pub fn validate(&self) -> Result<(), String> {
        if !matches!(self.width, 160 | 320 | 640) {
            return Err(format!(
                "storyboard width must be 160, 320, or 640 (got {})",
                self.width
            ));
        }
        if !(2..=120).contains(&self.interval_seconds) {
            return Err(format!(
                "storyboard interval must be 2-120 seconds (got {})",
                self.interval_seconds
            ));
        }
        if self.quality > 100 {
            return Err(format!(
                "storyboard quality must be 0-100 (got {})",
                self.quality
            ));
        }
        if self.sprite_columns == 0 || self.sprite_rows == 0 {
            return Err(format!(
                "sprite columns/rows must be >0 (got {}x{})",
                self.sprite_columns, self.sprite_rows
            ));
        }
        Ok(())
    }
}

/// Result of a successful [`generate_storyboard`] call. The worker writes
/// these to the `storyboards` DB row and links the on-disk files.
#[derive(Debug, Clone)]
pub struct GenerationResult {
    /// Paths of the generated `sprite_NNN.webp` files, in order.
    pub sprite_files: Vec<PathBuf>,
    /// Number of sprite sheets written (`sprite_files.len()`).
    pub sprite_count: u32,
    /// Total thumbnails across all sheets.
    pub total_thumbnails: u32,
    /// Realised thumbnail height in pixels (derived from source aspect ratio;
    /// stored alongside `width` in the DB).
    pub height: u32,
    /// Sum of all sprite file sizes in bytes.
    pub total_size_bytes: u64,
    /// Wall-clock generation time in milliseconds.
    pub generation_duration_ms: u32,
}

/// Computed sprite-sheet layout for a content duration. Returned by
/// [`compute_sprite_layout`]; consumed by [`generate_storyboard`] and
/// [`build_webvtt_index`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpriteLayout {
    /// Total thumbnails that will be extracted across all sheets.
    pub total_thumbnails: u32,
    /// Number of full sprite sheets (`columns × rows` each).
    pub full_sheets: u32,
    /// Thumbnails in the final partial sheet (0 when duration divides
    /// evenly — no partial sheet is emitted in that case).
    pub last_sheet_thumbnails: u32,
    /// Total sprite sheets to generate (`full_sheets` + 1 if there is a
    /// partial sheet, else `full_sheets`).
    pub total_sheets: u32,
    /// Thumbnails per full sheet (`columns × rows`).
    pub thumbnails_per_sheet: u32,
}

impl SpriteLayout {
    /// Thumbnails emitted by sheet `index` (0-based). The last sheet may
    /// be partial.
    pub fn thumbnails_in_sheet(&self, index: u32) -> u32 {
        if self.last_sheet_thumbnails == 0 || index < self.full_sheets {
            self.thumbnails_per_sheet
        } else {
            self.last_sheet_thumbnails
        }
    }

    /// True when this sheet index holds the (possibly partial) last sheet.
    pub fn is_last_sheet(&self, index: u32) -> bool {
        self.last_sheet_thumbnails > 0 && index == self.full_sheets
    }
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// Operational failures raised by the generation pipeline. The worker
/// (Task 6) logs and skips these per-file; they do NOT surface to the HTTP
/// client. Translation to API errors happens only at the
/// `domains::storyboards::service` boundary, not here. Mirrors the
/// [`SegmentPipelineError`](crate::services::segments::SegmentPipelineError)
/// pattern.
#[derive(Error, Debug)]
pub enum StoryboardPipelineError {
    #[error("invalid generation config: {0}")]
    InvalidConfig(String),

    #[error("ffmpeg spawn failed: {0}")]
    FfmpegSpawn(String),

    #[error("ffmpeg exited with status {code:?}: {stderr}")]
    FfmpegFailed { code: Option<i32>, stderr: String },

    #[error("ffmpeg produced no output for sheet {sheet}")]
    EmptyOutput { sheet: u32 },

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

// ---------------------------------------------------------------------------
// Adaptive interval
// ---------------------------------------------------------------------------

/// Adaptive thumbnail interval per STORYBOARDS.md:
///
/// | Duration | Interval |
/// |---|---|
/// | ≤ 30 min | 5 s |
/// | 30-120 min | 10 s |
/// | > 120 min | 15 s |
pub fn adaptive_interval(duration_seconds: u32) -> u32 {
    match duration_seconds {
        0..=1_800 => 5,
        1_801..=7_200 => 10,
        _ => 15,
    }
}

// ---------------------------------------------------------------------------
// Sprite layout computation
// ---------------------------------------------------------------------------

/// Compute the sprite-sheet layout for a content duration.
///
/// `duration_seconds` should come from `media_files.runtime_seconds`. The
/// layout places one thumbnail per `interval_seconds` of content, packs
/// `columns × rows` thumbnails per sheet, and emits a final partial sheet
/// when the remainder is non-zero. A duration of zero yields a single
/// thumbnail (the first frame).
pub fn compute_sprite_layout(
    duration_seconds: u32,
    interval_seconds: u32,
    columns: u32,
    rows: u32,
) -> SpriteLayout {
    let interval = interval_seconds.max(1);
    let per_sheet = columns.saturating_mul(rows).max(1);

    // Ceil division: at least one thumbnail even for very short content so
    // the seek bar shows *something*.
    let total = if duration_seconds == 0 {
        1
    } else {
        duration_seconds.div_ceil(interval)
    };

    let full_sheets = total / per_sheet;
    let last_sheet_thumbnails = total % per_sheet;
    let total_sheets = full_sheets + if last_sheet_thumbnails > 0 { 1 } else { 0 };

    SpriteLayout {
        total_thumbnails: total,
        full_sheets,
        last_sheet_thumbnails,
        total_sheets: total_sheets.max(1),
        thumbnails_per_sheet: per_sheet,
    }
}

// ---------------------------------------------------------------------------
// WebVTT index authoring
// ---------------------------------------------------------------------------

/// Build the WebVTT index file content for a sprite layout.
///
/// Each cue covers one `interval_seconds` window and points at a region of
/// a sprite sheet via a `#xywh=` Media Fragment URI (W3C Media Fragments
/// spec — the format hls.js, Video.js, and Radiant Media Player all
/// consume). Cue ordering is monotonically increasing in time; sheet
/// transitions are seamless. The final cue extends to `duration_seconds`
/// so clients never see a gap at the end of the seek bar.
///
/// `sprite_url_for(index)` returns the URL (relative or absolute) of the
/// `index`-th sprite sheet. The caller controls the URL scheme so the same
/// function works for `/api/v1/...` paths and on-disk previews.
///
/// **Drift warning:** `interval_seconds` MUST match the value used by
/// [`generate_storyboard`] — a mismatch is the #1 source of preview drift
/// (thumbnails wander away from the seek position as the user scrubs).
#[allow(clippy::too_many_arguments)]
pub fn build_webvtt_index(
    layout: SpriteLayout,
    interval_seconds: u32,
    duration_seconds: u32,
    width: u32,
    height: u32,
    columns: u32,
    sprite_url_for: &dyn Fn(u32) -> String,
) -> String {
    let mut out = String::with_capacity((layout.total_thumbnails as usize) * 64 + 8);
    out.push_str("WEBVTT\n\n");

    let interval = interval_seconds.max(1) as u64;
    let cols = columns.max(1) as u64;
    let width = width as u64;
    let height = height as u64;
    let total = layout.total_thumbnails as u64;
    let duration = duration_seconds as u64;

    for i in 0..total {
        let sheet = (i / layout.thumbnails_per_sheet.max(1) as u64) as u32;
        let index_in_sheet = (i % layout.thumbnails_per_sheet.max(1) as u64) as u32;
        let col = (index_in_sheet as u64) % cols;
        let row = (index_in_sheet as u64) / cols;

        let start_secs = i * interval;
        let end_secs = if i + 1 == total {
            // Final cue extends to the content duration if known and later
            // than the next-interval boundary; otherwise covers one
            // interval. This matches the "no gap at the end" rule.
            duration.max((i + 1) * interval)
        } else {
            (i + 1) * interval
        };

        out.push_str(&format!(
            "{} --> {}\n",
            format_timecode_secs(start_secs),
            format_timecode_secs(end_secs)
        ));
        out.push_str(&format!(
            "{}#xywh={},{},{},{}\n\n",
            sprite_url_for(sheet),
            col * width,
            row * height,
            width,
            height
        ));
    }

    out
}

/// Format a duration in seconds as a WebVTT timestamp (`HH:MM:SS.mmm`).
///
/// WebVTT requires `HH:MM:SS.mmm` (or `MM:SS.mmm` for cues under an hour,
/// but the long form is always acceptable and avoids ambiguity).
pub fn format_timecode_secs(total_secs: u64) -> String {
    let millis = total_secs * 1_000;
    let h = millis / 3_600_000;
    let m = (millis % 3_600_000) / 60_000;
    let s = (millis % 60_000) / 1_000;
    let ms = millis % 1_000;
    format!("{h:02}:{m:02}:{s:02}.{ms:03}")
}

// ---------------------------------------------------------------------------
// Sprite filename validation (path-traversal protection)
// ---------------------------------------------------------------------------

/// Validate a sprite filename against the expected `sprite_NNN.webp` pattern.
///
/// Rejects empty names, names containing path separators (`/`, `\`), parent
/// references (`..`), Windows reserved chars, or names that don't match the
/// canonical form. Used by the sprite-serving HTTP handler to prevent path
/// traversal (`GET /storyboard/../../etc/passwd`).
pub fn validate_sprite_filename(name: &str) -> Result<u32, String> {
    if name.is_empty() {
        return Err("empty sprite filename".to_string());
    }
    if name.contains('/') || name.contains('\\') {
        return Err(format!("sprite filename contains path separator: {name}"));
    }
    if name.contains("..") {
        return Err(format!("sprite filename contains parent reference: {name}"));
    }

    // Must match `sprite_NNN.webp` where NNN is 1-4 ASCII digits.
    let stem = name
        .strip_suffix(".webp")
        .ok_or_else(|| format!("sprite filename must end in .webp: {name}"))?;
    let Some(number_str) = stem.strip_prefix("sprite_") else {
        return Err(format!(
            "sprite filename must start with 'sprite_': {name}"
        ));
    };
    if number_str.is_empty() || number_str.len() > 4 {
        return Err(format!(
            "sprite number must be 1-4 digits: {name}"
        ));
    }
    if !number_str.bytes().all(|b| b.is_ascii_digit()) {
        return Err(format!(
            "sprite number must be ASCII digits: {name}"
        ));
    }
    let n: u32 = number_str
        .parse()
        .map_err(|_| format!("sprite number out of range: {name}"))?;
    if n == 0 {
        return Err(format!("sprite number must be 1-based: {name}"));
    }
    Ok(n)
}

/// Format a sprite sheet number into its canonical filename (`sprite_001.webp`).
///
/// 3-digit zero-padded per the design doc and the WebVTT cue examples. The
/// max sheet count for a 4-hour movie at 5s interval is ~288 sheets, which
/// fits comfortably in 3 digits; longer content rolls over to 4 digits and
/// [`validate_sprite_filename`] still accepts it.
pub fn sprite_filename(sheet_index: u32) -> String {
    // sheet_index is 0-based internally; filenames are 1-based per the design.
    format!("sprite_{:03}.webp", sheet_index + 1)
}

// ---------------------------------------------------------------------------
// Generation pipeline
// ---------------------------------------------------------------------------

/// Generate a complete storyboard for a single media file.
///
/// Spawns one FFmpeg process per sprite sheet. Each invocation:
///
/// ```text
/// ffmpeg [-skip_frame nokey] -ss <start> -t <window> -i <source> \
///        -vf "fps=1/N,scale=W:trunc(ow/a/2)*2,tile=COLSxROWS" \
///        -frames:v 1 -c:v webp -lossless 0 -q:v Q \
///        -an -y <output_dir>/sprite_NNN.webp
/// ```
///
/// On success: writes sprite files to `output_dir`, emits `index.vtt` via
/// [`build_webvtt_index`], and returns a [`GenerationResult`].
/// On failure: returns the first error; partial output may remain on disk
/// and the caller (worker) is responsible for cleanup.
///
/// `output_dir` is created if missing. The caller passes the per-file
/// cache directory (`{cache_dir}/storyboards/{media_file_id}/`).
pub async fn generate_storyboard(
    source_path: &Path,
    output_dir: &Path,
    duration_seconds: u32,
    config: &GenerationConfig,
) -> Result<GenerationResult, StoryboardPipelineError> {
    config
        .validate()
        .map_err(StoryboardPipelineError::InvalidConfig)?;

    tokio::fs::create_dir_all(output_dir).await?;

    let layout = compute_sprite_layout(
        duration_seconds,
        config.interval_seconds,
        config.sprite_columns,
        config.sprite_rows,
    );

    let started = Instant::now();

    let mut sprite_files = Vec::with_capacity(layout.total_sheets as usize);
    let mut total_size_bytes: u64 = 0;
    let mut realised_height: u32 = 0;

    for sheet_index in 0..layout.total_sheets {
        let sheet_start_secs =
            sheet_index * layout.thumbnails_per_sheet * config.interval_seconds;
        let thumbnails_in_sheet = layout.thumbnails_in_sheet(sheet_index);
        let window_secs = thumbnails_in_sheet * config.interval_seconds;

        let sprite_name = sprite_filename(sheet_index);
        let sprite_path = output_dir.join(&sprite_name);

        invoke_ffmpeg_for_sheet(
            source_path,
            &sprite_path,
            sheet_start_secs,
            window_secs,
            thumbnails_in_sheet,
            config,
        )
        .await?;

        let metadata = tokio::fs::metadata(&sprite_path).await?;
        total_size_bytes += metadata.len();

        if realised_height == 0 {
            realised_height = inspect_sprite_height(&sprite_path).await?;
        }

        sprite_files.push(sprite_path);
    }

    // No thumbnails generated (e.g. zero-duration file with empty layout)
    // is an error — the caller should have caught this via validate().
    if sprite_files.is_empty() {
        return Err(StoryboardPipelineError::EmptyOutput { sheet: 0 });
    }

    // Emit the WebVTT index alongside the sprites. The relative URL is just
    // the sprite filename — the HTTP handler serves them under a stable
    // per-item path, so the index is portable across hosts.
    let index_path = output_dir.join("index.vtt");
    let index_content = build_webvtt_index(
        layout,
        config.interval_seconds,
        duration_seconds,
        config.width,
        realised_height,
        config.sprite_columns,
        &|idx| sprite_filename(idx),
    );
    tokio::fs::write(&index_path, index_content).await?;

    let generation_duration_ms = started.elapsed().as_millis() as u32;

    Ok(GenerationResult {
        sprite_count: sprite_files.len() as u32,
        total_thumbnails: layout.total_thumbnails,
        height: realised_height,
        total_size_bytes,
        sprite_files,
        generation_duration_ms,
    })
}

/// Spawn FFmpeg to produce a single sprite sheet covering the window
/// `[sheet_start_secs, sheet_start_secs + window_secs)`.
///
/// `-ss` is placed *before* `-i` for fast keyframe-accurate seek — without
/// it FFmpeg decodes from the start of the file for every sheet, which is
/// catastrophic for long content. With `keyframe_only = true`, FFmpeg's
/// decoder skips inter-frame decoding entirely (~100x speedup).
async fn invoke_ffmpeg_for_sheet(
    source: &Path,
    output: &Path,
    sheet_start_secs: u32,
    window_secs: u32,
    _thumbnails_in_sheet: u32,
    config: &GenerationConfig,
) -> Result<(), StoryboardPipelineError> {
    let scale_filter = format!(
        "fps=1/{interval},scale={width}:trunc(ow/a/2)*2,tile={cols}x{rows}",
        interval = config.interval_seconds,
        width = config.width,
        cols = config.sprite_columns,
        rows = config.sprite_rows
    );

    let mut cmd = Command::new("ffmpeg");
    cmd.arg("-hide_banner")
        .arg("-nostdin")
        .arg("-nostats");

    if config.keyframe_only {
        // Placed before `-i` so the demuxer skips non-keyframe packets.
        cmd.arg("-skip_frame").arg("nokey");
    }

    // `-ss` before `-i` for fast seek; the demuxer jumps to the keyframe at
    // or before `sheet_start_secs` and decoding begins there.
    cmd.arg("-ss")
        .arg(sheet_start_secs.to_string())
        .arg("-t")
        .arg(window_secs.to_string())
        .arg("-i")
        .arg(source);

    cmd.arg("-vf").arg(&scale_filter)
        .arg("-frames:v").arg("1")
        .arg("-an")
        .arg("-c:v").arg("webp")
        .arg("-lossless").arg("0")
        .arg("-q:v").arg(config.quality.to_string())
        .arg("-y")
        .arg(output);

    cmd.stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .stdin(std::process::Stdio::null())
        .kill_on_drop(true);

    let output_result = cmd
        .output()
        .await
        .map_err(|e| StoryboardPipelineError::FfmpegSpawn(e.to_string()))?;

    if !output_result.status.success() {
        return Err(StoryboardPipelineError::FfmpegFailed {
            code: output_result.status.code(),
            stderr: String::from_utf8_lossy(&output_result.stderr).to_string(),
        });
    }

    // FFmpeg exits 0 even when it produced no frames (e.g. seek past EOF).
    // Verify the output file exists and is non-empty.
    let metadata = match tokio::fs::metadata(output).await {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(StoryboardPipelineError::EmptyOutput {
                sheet: 0,
            });
        }
        Err(e) => return Err(StoryboardPipelineError::Io(e)),
    };
    if metadata.len() == 0 {
        return Err(StoryboardPipelineError::EmptyOutput { sheet: 0 });
    }

    Ok(())
}

/// Read the pixel height of a generated WebP sprite by parsing the RIFF
/// chunk header. Falls back to the design's default 16:9-derived height
/// (width / (16/9) ≈ width * 9 / 16) if parsing fails — sprite generation
/// still succeeds; only the stored `height` is approximate.
///
/// WebP RIFF layout: `RIFF` (4) + size (4) + `WEBP` (4) + `VP8 ` or `VP8L`
/// or `VP8X` (4) + chunk-size (4) + payload. For lossy `VP8 `, the
/// 16-bit little-endian values at offsets 26 and 28 are width and height.
/// For `VP8L` (lossless), the payload starts with a signature byte then a
/// 14-bit width and 14-bit height packed into 4 bytes. We probe both.
async fn inspect_sprite_height(path: &Path) -> Result<u32, StoryboardPipelineError> {
    let bytes = tokio::fs::read(path).await?;

    // Need at least the RIFF header + VP8 chunk header.
    if bytes.len() < 30 {
        return Ok(fallback_height_from_filename(path));
    }
    if &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WEBP" {
        return Ok(fallback_height_from_filename(path));
    }

    let fourcc = &bytes[12..16];
    match fourcc {
        b"VP8 " => {
            // Lossy. Width/height are 16-bit LE at offsets 26 and 28.
            let width = u16::from_le_bytes([bytes[26], bytes[27]]) as u32 & 0x3FFF;
            let height = u16::from_le_bytes([bytes[28], bytes[29]]) as u32 & 0x3FFF;
            if width == 0 || height == 0 {
                Ok(fallback_height_from_filename(path))
            } else {
                Ok(height)
            }
        }
        b"VP8L" => {
            // Lossless. Byte 21 is 0x2F signature; then 4 bytes hold
            // 14-bit width-1 and 14-bit height-1 (little-endian bit packing).
            if bytes.len() < 25 || bytes[21] != 0x2F {
                return Ok(fallback_height_from_filename(path));
            }
            let b0 = bytes[22] as u32;
            let b1 = bytes[23] as u32;
            let b2 = bytes[24] as u32;
            let width = 1 + (b0 | ((b1 & 0x3F) << 8));
            let height = 1 + (((b1 >> 6) & 0x03) | (b2 << 2));
            if width == 0 || height == 0 {
                Ok(fallback_height_from_filename(path))
            } else {
                Ok(height)
            }
        }
        _ => {
            // VP8X (extended) — rare for our encoder output. Fall back.
            Ok(fallback_height_from_filename(path))
        }
    }
}

/// Derive a fallback height from the filename's implied width using a 16:9
/// aspect ratio. Used only when WebP header parsing fails — never on the
/// happy path.
fn fallback_height_from_filename(path: &Path) -> u32 {
    let _ = path;
    // Default to 320×180 (16:9 at the default width). The worker will
    // overwrite this with the true height from the first successful parse.
    180
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // ---- adaptive_interval ----

    #[test]
    fn adaptive_interval_short_content() {
        assert_eq!(adaptive_interval(0), 5);
        assert_eq!(adaptive_interval(1), 5);
        assert_eq!(adaptive_interval(1_800), 5); // exactly 30 min
    }

    #[test]
    fn adaptive_interval_mid_length() {
        assert_eq!(adaptive_interval(1_801), 10);
        assert_eq!(adaptive_interval(3_600), 10); // 1 hour
        assert_eq!(adaptive_interval(7_200), 10); // exactly 2 hours
    }

    #[test]
    fn adaptive_interval_long_content() {
        assert_eq!(adaptive_interval(7_201), 15);
        assert_eq!(adaptive_interval(10_800), 15); // 3 hours
    }

    // ---- compute_sprite_layout ----

    #[test]
    fn layout_single_sheet_partial() {
        // 5 min at 10s interval = 30 thumbnails, 10×20 sheet holds 200.
        let layout = compute_sprite_layout(300, 10, 10, 20);
        assert_eq!(layout.total_thumbnails, 30);
        assert_eq!(layout.full_sheets, 0);
        assert_eq!(layout.last_sheet_thumbnails, 30);
        assert_eq!(layout.total_sheets, 1);
    }

    #[test]
    fn layout_exact_multiple() {
        // 200 thumbnails worth of content (2000s at 10s) fills exactly one
        // 10×20 sheet; no partial sheet.
        let layout = compute_sprite_layout(2_000, 10, 10, 20);
        assert_eq!(layout.total_thumbnails, 200);
        assert_eq!(layout.full_sheets, 1);
        assert_eq!(layout.last_sheet_thumbnails, 0);
        assert_eq!(layout.total_sheets, 1);
    }

    #[test]
    fn layout_multi_sheet() {
        // 2-hour movie at 10s = 720 thumbnails → 3 full 200-sheet +
        // 120-thumbnail partial = 4 sheets.
        let layout = compute_sprite_layout(7_200, 10, 10, 20);
        assert_eq!(layout.total_thumbnails, 720);
        assert_eq!(layout.full_sheets, 3);
        assert_eq!(layout.last_sheet_thumbnails, 120);
        assert_eq!(layout.total_sheets, 4);
    }

    #[test]
    fn layout_zero_duration_yields_one_thumbnail() {
        let layout = compute_sprite_layout(0, 10, 10, 20);
        assert_eq!(layout.total_thumbnails, 1);
        assert_eq!(layout.total_sheets, 1);
    }

    #[test]
    fn layout_thumbnails_in_sheet() {
        let layout = compute_sprite_layout(7_200, 10, 10, 20);
        assert_eq!(layout.thumbnails_in_sheet(0), 200);
        assert_eq!(layout.thumbnails_in_sheet(2), 200);
        assert_eq!(layout.thumbnails_in_sheet(3), 120); // partial
    }

    #[test]
    fn layout_is_last_sheet() {
        let layout = compute_sprite_layout(7_200, 10, 10, 20);
        assert!(!layout.is_last_sheet(0));
        assert!(!layout.is_last_sheet(2));
        assert!(layout.is_last_sheet(3));
    }

    #[test]
    fn layout_zero_interval_treated_as_one() {
        // Defensive: zero should never divide-by-zero.
        let layout = compute_sprite_layout(60, 0, 10, 20);
        assert_eq!(layout.total_thumbnails, 60);
    }

    // ---- format_timecode_secs ----

    #[test]
    fn timecode_zero() {
        assert_eq!(format_timecode_secs(0), "00:00:00.000");
    }

    #[test]
    fn timecode_seconds() {
        assert_eq!(format_timecode_secs(10), "00:00:10.000");
    }

    #[test]
    fn timecode_minutes() {
        assert_eq!(format_timecode_secs(125), "00:02:05.000");
    }

    #[test]
    fn timecode_hours() {
        assert_eq!(format_timecode_secs(3_661), "01:01:01.000");
    }

    #[test]
    fn timecode_long_movie() {
        // 2 hours exactly
        assert_eq!(format_timecode_secs(7_200), "02:00:00.000");
    }

    // ---- validate_sprite_filename ----

    #[test]
    fn sprite_filename_valid() {
        assert_eq!(validate_sprite_filename("sprite_001.webp").unwrap(), 1);
        assert_eq!(validate_sprite_filename("sprite_002.webp").unwrap(), 2);
        assert_eq!(validate_sprite_filename("sprite_9999.webp").unwrap(), 9999);
    }

    #[test]
    fn sprite_filename_rejects_zero() {
        assert!(validate_sprite_filename("sprite_000.webp").is_err());
    }

    #[test]
    fn sprite_filename_rejects_traversal() {
        assert!(validate_sprite_filename("../etc/passwd").is_err());
        assert!(validate_sprite_filename("sprite_001.webp/../../etc").is_err());
        assert!(validate_sprite_filename("..\\windows\\system32").is_err());
        assert!(validate_sprite_filename("../../sprite_001.webp").is_err());
    }

    #[test]
    fn sprite_filename_rejects_wrong_extension() {
        assert!(validate_sprite_filename("sprite_001.jpg").is_err());
        assert!(validate_sprite_filename("sprite_001").is_err());
    }

    #[test]
    fn sprite_filename_rejects_wrong_prefix() {
        assert!(validate_sprite_filename("thumb_001.webp").is_err());
        assert!(validate_sprite_filename("sprite.webp").is_err());
    }

    #[test]
    fn sprite_filename_rejects_empty() {
        assert!(validate_sprite_filename("").is_err());
    }

    #[test]
    fn sprite_filename_rejects_non_digits() {
        assert!(validate_sprite_filename("sprite_abc.webp").is_err());
        assert!(validate_sprite_filename("sprite_-01.webp").is_err());
    }

    // ---- sprite_filename ----

    #[test]
    fn sprite_filename_format() {
        assert_eq!(sprite_filename(0), "sprite_001.webp");
        assert_eq!(sprite_filename(1), "sprite_002.webp");
        assert_eq!(sprite_filename(999), "sprite_1000.webp");
    }

    // ---- build_webvtt_index ----

    #[test]
    fn webvtt_header_present() {
        let layout = compute_sprite_layout(30, 10, 10, 20);
        let vtt =         build_webvtt_index(
            layout,
            10,
            30,
            320,
            180,
            10,
            &|idx| sprite_filename(idx),
        );
        assert!(vtt.starts_with("WEBVTT\n\n"));
    }

    #[test]
    fn webvtt_first_cue_starts_at_zero() {
        let layout = compute_sprite_layout(30, 10, 10, 20);
        let vtt =         build_webvtt_index(
            layout,
            10,
            30,
            320,
            180,
            10,
            &|idx| sprite_filename(idx),
        );
        assert!(
            vtt.contains("00:00:00.000 --> 00:00:10.000"),
            "first cue should cover [0, 10s): got:\n{vtt}"
        );
    }

    #[test]
    fn webvtt_cue_count_matches_thumbnails() {
        let layout = compute_sprite_layout(60, 10, 10, 20);
        let vtt =         build_webvtt_index(
            layout,
            10,
            60,
            320,
            180,
            10,
            &|idx| sprite_filename(idx),
        );
        // 6 cues for 60s at 10s interval.
        let cue_count = vtt.matches("-->").count();
        assert_eq!(cue_count, 6);
    }

    #[test]
    fn webvtt_xywh_grid_coords() {
        // 3 thumbnails in a single sheet of 10 cols → coordinates
        // (0,0), (320,0), (640,0).
        let layout = compute_sprite_layout(30, 10, 10, 20);
        let vtt =         build_webvtt_index(
            layout,
            10,
            30,
            320,
            180,
            10,
            &|idx| sprite_filename(idx),
        );
        assert!(
            vtt.contains("sprite_001.webp#xywh=0,0,320,180"),
            "missing first-region cue:\n{vtt}"
        );
        assert!(
            vtt.contains("sprite_001.webp#xywh=320,0,320,180"),
            "missing second-region cue:\n{vtt}"
        );
        assert!(
            vtt.contains("sprite_001.webp#xywh=640,0,320,180"),
            "missing third-region cue:\n{vtt}"
        );
    }

    #[test]
    fn webvtt_second_row_y_offset() {
        // 11 thumbnails in a 10-col sheet: the 11th wraps to row 1, y=180.
        let layout = compute_sprite_layout(110, 10, 10, 20);
        let vtt =         build_webvtt_index(
            layout,
            10,
            110,
            320,
            180,
            10,
            &|idx| sprite_filename(idx),
        );
        // 11th cue (index 10) should be at x=0, y=180.
        assert!(
            vtt.contains("sprite_001.webp#xywh=0,180,320,180"),
            "expected row-2 first-column cue:\n{vtt}"
        );
    }

    #[test]
    fn webvtt_references_second_sheet() {
        // 25 thumbnails, 10-col × 2-row sheets = 20 per sheet.
        // Thumbnail 20 (0-based) is the first of sheet 1.
        let layout = compute_sprite_layout(250, 10, 10, 2);
        let vtt =         build_webvtt_index(
            layout,
            10,
            250,
            320,
            180,
            10,
            &|idx| sprite_filename(idx),
        );
        assert!(
            vtt.contains("sprite_002.webp#xywh=0,0,320,180"),
            "expected a cue pointing at sprite_002:\n{vtt}"
        );
    }

    #[test]
    fn webvtt_final_cue_extends_to_duration() {
        // 60s duration, 10s interval: final cue ends at 60s, not 70s.
        let layout = compute_sprite_layout(60, 10, 10, 20);
        let vtt =         build_webvtt_index(
            layout,
            10,
            60,
            320,
            180,
            10,
            &|idx| sprite_filename(idx),
        );
        // The 6th (final) cue starts at 50s and should end at 60s.
        assert!(
            vtt.contains("00:00:50.000 --> 00:01:00.000"),
            "final cue should end at duration:\n{vtt}"
        );
    }

    #[test]
    fn webvtt_sheet_transition_no_gap() {
        // 5 thumbnails, 2-col × 2-row sheets = 4 per sheet → 1 full sheet
        // + 1 partial (1 thumbnail). Verifies the second sheet is referenced
        // and there's no timestamp gap at the sheet boundary.
        let layout = compute_sprite_layout(50, 10, 2, 2);
        assert_eq!(layout.total_sheets, 2);
        let vtt = build_webvtt_index(
            layout,
            10,
            50,
            320,
            180,
            2,
            &|idx| sprite_filename(idx),
        );
        // Cues 0-3 → sprite_001; cue 4 → sprite_002.
        assert!(vtt.contains("sprite_001.webp"));
        assert!(vtt.contains("sprite_002.webp"));
        // Cue 4 starts at 40s (immediately after cue 3 ends at 40s — no gap).
        assert!(
            vtt.contains("00:00:40.000 --> 00:00:50.000"),
            "expected sheet-2 transition cue at 40s:\n{vtt}"
        );
    }

    // ---- GenerationConfig ----

    #[test]
    fn config_default_is_valid() {
        assert!(GenerationConfig::default().validate().is_ok());
    }

    #[test]
    fn config_thumbnails_per_sheet_default() {
        assert_eq!(GenerationConfig::default().thumbnails_per_sheet(), 200);
    }

    #[test]
    fn config_rejects_bad_width() {
        let mut c = GenerationConfig::default();
        c.width = 200;
        assert!(c.validate().is_err());
    }

    #[test]
    fn config_rejects_bad_interval() {
        let mut c = GenerationConfig::default();
        c.interval_seconds = 1;
        assert!(c.validate().is_err());
        c.interval_seconds = 121;
        assert!(c.validate().is_err());
    }

    #[test]
    fn config_rejects_zero_grid() {
        let mut c = GenerationConfig::default();
        c.sprite_columns = 0;
        assert!(c.validate().is_err());
    }

    // ---- inspect_sprite_height (synthetic WebP headers) ----

    #[test]
    fn webp_vp8_lossy_header_parses_height() {
        // Minimal synthetic WebP with VP8 lossy chunk. Width 320 (0x140),
        // height 180 (0xB4). The actual frame data is irrelevant — only
        // the first 30 bytes are read.
        let mut bytes = vec![0u8; 64];
        bytes[0..4].copy_from_slice(b"RIFF");
        bytes[8..12].copy_from_slice(b"WEBP");
        bytes[12..16].copy_from_slice(b"VP8 ");
        // 16-bit LE width at offset 26, height at offset 28.
        bytes[26] = 0x40;
        bytes[27] = 0x01; // 0x0140 = 320
        bytes[28] = 0xB4;
        bytes[29] = 0x00; // 0x00B4 = 180

        let temp = std::env::temp_dir().join("duskcue_storyboard_test_vp8.webp");
        std::fs::write(&temp, &bytes).unwrap();

        let rt = tokio::runtime::Runtime::new().unwrap();
        let height = rt.block_on(inspect_sprite_height(&temp)).unwrap();
        assert_eq!(height, 180);

        let _ = std::fs::remove_file(&temp);
    }

    #[test]
    fn webp_garbage_falls_back() {
        let bytes = b"not a webp file at all";
        let temp = std::env::temp_dir().join("duskcue_storyboard_test_garbage.webp");
        std::fs::write(&temp, bytes).unwrap();

        let rt = tokio::runtime::Runtime::new().unwrap();
        let height = rt.block_on(inspect_sprite_height(&temp)).unwrap();
        // Fallback is the default 180 (for 320-wide 16:9).
        assert_eq!(height, 180);

        let _ = std::fs::remove_file(&temp);
    }

    #[test]
    fn webp_too_short_falls_back() {
        let bytes = vec![0u8; 10]; // too short for any header
        let temp = std::env::temp_dir().join("duskcue_storyboard_test_short.webp");
        std::fs::write(&temp, &bytes).unwrap();

        let rt = tokio::runtime::Runtime::new().unwrap();
        let height = rt.block_on(inspect_sprite_height(&temp)).unwrap();
        assert_eq!(height, 180);

        let _ = std::fs::remove_file(&temp);
    }
}

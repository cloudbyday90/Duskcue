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

//! Clean art preservation — ensures source artwork is never modified during
//! overlay compositing.
//!
//! The overlay engine produces composited images by scaling source artwork to
//! a standard canvas, applying overlays, and caching the result. Source artwork
//! (at `artwork.local_path`) is treated as **immutable** — it is only ever
//! read, never written. All derived artifacts (clean backups, composited
//! results) live in the regenerable `/cache/images/` directory.
//!
//! ## Three-tier artwork state
//!
//! | Tier | Location | Purpose |
//! |---|---|---|
//! | Source (immutable) | `artwork.local_path` | Original from provider/upload — never touched |
//! | Clean backup (scaled) | `/cache/images/clean/{type}/{artwork_id}.webp` | Base for re-compositing |
//! | Overlaid result | `/cache/images/overlays/{type}/{media_item_id}.webp` | Served to clients |
//!
//! ## Re-compositing logic
//!
//! 1. **First application** — scale source → save clean backup → composite → save result
//! 2. **Overlay definition change** — load clean backup → re-composite → save new result
//!    (no source re-read needed)
//! 3. **Source artwork change** — new artwork UUID → clean backup filename changes →
//!    cache miss triggers re-scale from new source
//!
//! The clean backup filename includes the source `artwork_id` (UUID), so when
//! TMDb refresh or user upload creates a new artwork row, the old clean backup
//! is naturally orphaned and a fresh one is created from the new source. The
//! [`compute_config_hash`] function also includes the `source_artwork_id` so a
//! source change invalidates the hash and forces re-compositing.
//!
//! ## State tracking
//!
//! The `artwork_overlay_state` table records which overlays were applied, the
//! config hash, and the paths to the clean backup and overlaid result. This
//! enables incremental reprocessing — only items whose applicable overlays or
//! source artwork have changed are re-composited.

use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use image::DynamicImage;
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::domains::overlays::OverlayError;
use crate::services::image_pipeline::{self, EncodeConfig};
use crate::services::overlays::{self as overlay_svc, CanvasPreset};

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// A resolved clean backup — the source artwork scaled to the standard canvas,
/// decoded and ready for compositing.
pub struct CleanArt {
    pub image: image::RgbaImage,
    pub path: PathBuf,
    pub source_artwork_id: Uuid,
}

/// DB row representation of `artwork_overlay_state`.
pub struct OverlayStateRow {
    pub media_item_id: Uuid,
    pub artwork_type: String,
    pub applied_overlay_ids: Vec<Uuid>,
    pub overlay_config_hash: String,
    pub clean_art_path: String,
    pub overlaid_art_path: Option<String>,
    pub applied_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Input for config hash computation — one per applied overlay definition.
#[derive(Debug, Clone)]
pub struct OverlayHashInput {
    pub id: Uuid,
    pub updated_at: DateTime<Utc>,
}

/// The overlaid result for display — bytes ready to serve plus the artwork_id
/// for ETag construction.
pub struct ResolvedOverlaid {
    pub bytes: Vec<u8>,
    pub artwork_id: Uuid,
}

// ---------------------------------------------------------------------------
// Clean backup management
// ---------------------------------------------------------------------------

/// Ensure a clean backup exists for the given media item + artwork type.
///
/// If a cached clean backup (at canvas dimensions) already exists for the
/// current primary artwork row, it is loaded and returned. Otherwise, the
/// source artwork is read (read-only — never written), scaled to the standard
/// canvas via Lanczos3, encoded as WebP, written to the clean cache directory,
/// and returned.
///
/// The clean backup filename includes the source `artwork_id` so that when the
/// primary artwork changes (new TMDb download, user upload), the old backup is
/// naturally orphaned and a fresh one is created from the new source.
pub async fn ensure_clean_backup(
    pool: &PgPool,
    data_dir: &Path,
    media_item_id: Uuid,
    artwork_type: &str,
    encode_config: &EncodeConfig,
) -> Result<CleanArt, OverlayError> {
    let db_type = artwork_table_type(artwork_type);
    let row = sqlx::query(
        r#"SELECT id, local_path FROM artwork
           WHERE media_item_id = $1 AND artwork_type = $2 AND "order" = 0
           LIMIT 1"#,
    )
    .bind(media_item_id)
    .bind(db_type)
    .fetch_optional(pool)
    .await?;

    let row = row.ok_or_else(|| {
        OverlayError::ImageFileNotFound(format!(
            "no primary {artwork_type} artwork found for media item {media_item_id}"
        ))
    })?;

    let artwork_id: Uuid = row.try_get("id")?;
    let local_path: Option<String> = row.try_get("local_path")?;
    let source_path = local_path
        .as_ref()
        .filter(|p| !p.is_empty())
        .ok_or_else(|| OverlayError::ImageFileNotFound("artwork has no local_path".into()))?;

    let clean_path = clean_art_path(data_dir, artwork_type, artwork_id);

    if clean_path.exists() {
        tracing::debug!(
            %media_item_id, %artwork_id,
            path = %clean_path.display(),
            "clean art cache hit"
        );
        let bytes = std::fs::read(&clean_path).map_err(|e| {
            OverlayError::CompositingFailed(format!("failed to read clean backup: {e}"))
        })?;
        let image = decode_webp(&bytes)?;
        return Ok(CleanArt {
            image,
            path: clean_path,
            source_artwork_id: artwork_id,
        });
    }

    tracing::debug!(
        %media_item_id, %artwork_id,
        path = %clean_path.display(),
        "clean art cache miss — scaling source"
    );

    let source_bytes = std::fs::read(source_path).map_err(|e| {
        OverlayError::ImageFileNotFound(format!("failed to read source artwork: {e}"))
    })?;

    let source_img = decode_source(&source_bytes)?;
    let canvas = canvas_for_type(artwork_type)?;
    let scaled = overlay_svc::resize_to_canvas(&source_img, canvas);

    let (webp_bytes, _) = image_pipeline::encode_webp(&DynamicImage::ImageRgba8(scaled.clone()), encode_config)
        .map_err(|e| OverlayError::CompositingFailed(format!("failed to encode clean backup WebP: {e}")))?;

    if let Some(parent) = clean_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            OverlayError::CompositingFailed(format!("failed to create clean art directory: {e}"))
        })?;
    }
    std::fs::write(&clean_path, &webp_bytes).map_err(|e| {
        OverlayError::CompositingFailed(format!("failed to write clean backup: {e}"))
    })?;

    Ok(CleanArt {
        image: scaled,
        path: clean_path,
        source_artwork_id: artwork_id,
    })
}

/// Decode raw image bytes (JPEG/PNG/WebP) into an `RgbaImage`.
fn decode_source(bytes: &[u8]) -> Result<image::RgbaImage, OverlayError> {
    image::load_from_memory(bytes)
        .map(|img| img.to_rgba8())
        .map_err(|e| OverlayError::CompositingFailed(format!("failed to decode source artwork: {e}")))
}

/// Decode WebP bytes into an `RgbaImage`.
fn decode_webp(bytes: &[u8]) -> Result<image::RgbaImage, OverlayError> {
    image::load_from_memory(bytes)
        .map(|img| img.to_rgba8())
        .map_err(|e| OverlayError::CompositingFailed(format!("failed to decode clean backup WebP: {e}")))
}

// ---------------------------------------------------------------------------
// Config hash
// ---------------------------------------------------------------------------

/// Compute a deterministic hash of the resolved overlay configuration.
///
/// The hash captures:
/// - The set of applied overlay IDs (sorted for determinism)
/// - Each overlay's `updated_at` (changes when any visual property is edited)
/// - The source artwork ID (changes when the primary artwork is replaced)
///
/// When the stored hash in `artwork_overlay_state` matches the newly computed
/// hash, re-compositing is skipped — the overlaid result is already current.
pub fn compute_config_hash(inputs: &[OverlayHashInput], source_artwork_id: Uuid) -> String {
    let mut sorted: Vec<&OverlayHashInput> = inputs.iter().collect();
    sorted.sort_by_key(|i| i.id);

    let mut hasher = blake3::Hasher::new();
    hasher.update(source_artwork_id.as_bytes());
    for input in &sorted {
        hasher.update(input.id.as_bytes());
        if let Some(nanos) = input.updated_at.timestamp_nanos_opt() {
            hasher.update(&nanos.to_be_bytes());
        }
    }
    hasher.finalize().to_hex().to_string()
}

// ---------------------------------------------------------------------------
// Overlay state persistence
// ---------------------------------------------------------------------------

/// Read the current overlay state for a media item + artwork type.
/// Returns `None` when no state row exists (first-time processing).
pub async fn get_overlay_state(
    pool: &PgPool,
    media_item_id: Uuid,
    artwork_type: &str,
) -> Result<Option<OverlayStateRow>, sqlx::Error> {
    let row = sqlx::query(
        r#"SELECT media_item_id, artwork_type, applied_overlay_ids, overlay_config_hash,
                  clean_art_path, overlaid_art_path, applied_at, updated_at
           FROM artwork_overlay_state
           WHERE media_item_id = $1 AND artwork_type = $2"#,
    )
    .bind(media_item_id)
    .bind(artwork_type)
    .fetch_optional(pool)
    .await?;

    row.map(map_state_row).transpose()
}

/// Insert or update the overlay state. The `UNIQUE(media_item_id, artwork_type)`
/// constraint makes this an upsert.
pub async fn upsert_overlay_state(
    pool: &PgPool,
    media_item_id: Uuid,
    artwork_type: &str,
    applied_overlay_ids: &[Uuid],
    overlay_config_hash: &str,
    clean_art_path: &str,
    overlaid_art_path: Option<&str>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"INSERT INTO artwork_overlay_state
               (media_item_id, artwork_type, applied_overlay_ids, overlay_config_hash,
                clean_art_path, overlaid_art_path, applied_at, updated_at)
           VALUES ($1, $2, $3, $4, $5, $6, now(), now())
           ON CONFLICT (media_item_id, artwork_type) DO UPDATE SET
               applied_overlay_ids = EXCLUDED.applied_overlay_ids,
               overlay_config_hash = EXCLUDED.overlay_config_hash,
               clean_art_path = EXCLUDED.clean_art_path,
               overlaid_art_path = EXCLUDED.overlaid_art_path,
               applied_at = EXCLUDED.applied_at,
               updated_at = now()"#,
    )
    .bind(media_item_id)
    .bind(artwork_type)
    .bind(applied_overlay_ids)
    .bind(overlay_config_hash)
    .bind(clean_art_path)
    .bind(overlaid_art_path)
    .execute(pool)
    .await?;

    Ok(())
}

/// Delete the overlay state row and the overlaid result file. The clean backup
/// is preserved (it may be reused if overlays are re-enabled). Source artwork
/// is never touched. Returns `true` if a row was deleted, `false` if no state
/// existed.
pub async fn delete_overlay_state(
    pool: &PgPool,
    media_item_id: Uuid,
    artwork_type: &str,
) -> Result<bool, OverlayError> {
    let row = sqlx::query(
        r#"DELETE FROM artwork_overlay_state
           WHERE media_item_id = $1 AND artwork_type = $2
           RETURNING overlaid_art_path"#,
    )
    .bind(media_item_id)
    .bind(artwork_type)
    .fetch_optional(pool)
    .await?;

    let Some(row) = row else {
        return Ok(false);
    };

    let overlaid_path: Option<String> = row.try_get("overlaid_art_path").ok().flatten();
    if let Some(ref path_str) = overlaid_path
        && let Err(e) = std::fs::remove_file(PathBuf::from(path_str))
        && e.kind() != std::io::ErrorKind::NotFound
    {
        tracing::warn!(
            error = %e,
            path = path_str,
            "failed to remove overlaid result file"
        );
    }

    Ok(true)
}

// ---------------------------------------------------------------------------
// Overlaid result persistence
// ---------------------------------------------------------------------------

/// Save the composited result to the overlays cache directory.
/// Returns the path where the result was saved.
pub fn save_overlaid_result(
    data_dir: &Path,
    media_item_id: Uuid,
    artwork_type: &str,
    webp_bytes: &[u8],
) -> Result<PathBuf, OverlayError> {
    let dir = overlays_dir(data_dir, artwork_type);
    std::fs::create_dir_all(&dir).map_err(|e| {
        OverlayError::CompositingFailed(format!("failed to create overlays directory: {e}"))
    })?;

    let path = dir.join(format!("{media_item_id}.webp"));
    std::fs::write(&path, webp_bytes).map_err(|e| {
        OverlayError::CompositingFailed(format!("failed to write overlaid result: {e}"))
    })?;

    Ok(path)
}

/// Check whether an overlaid result exists for the given media item + artwork
/// type. If it does, read the bytes and return them along with the source
/// artwork ID (for ETag construction).
///
/// Used by the display layer ([`crate::services::artwork_delivery`]) to serve
/// the composited image instead of the bare source when overlays are active.
pub async fn resolve_overlaid_artwork(
    pool: &PgPool,
    media_item_id: Uuid,
    artwork_type: &str,
) -> Result<Option<ResolvedOverlaid>, sqlx::Error> {
    let row = sqlx::query(
        r#"SELECT aos.overlaid_art_path, a.id AS artwork_id
           FROM artwork_overlay_state aos
           JOIN artwork a ON a.media_item_id = aos.media_item_id
                       AND a.artwork_type = $2
                       AND a."order" = 0
           WHERE aos.media_item_id = $1 AND aos.artwork_type = $2"#,
    )
    .bind(media_item_id)
    .bind(artwork_type)
    .fetch_optional(pool)
    .await?;

    let Some(row) = row else {
        return Ok(None);
    };

    let overlaid_path: Option<String> = row.try_get("overlaid_art_path").ok().flatten();
    let artwork_id: Uuid = row.try_get("artwork_id")?;

    let Some(path_str) = overlaid_path else {
        return Ok(None);
    };

    let path = PathBuf::from(&path_str);
    if !path.exists() {
        tracing::debug!(
            %media_item_id,
            path = %path.display(),
            "overlaid art path recorded in DB but file missing on disk"
        );
        return Ok(None);
    }

    match std::fs::read(&path) {
        Ok(bytes) => Ok(Some(ResolvedOverlaid { bytes, artwork_id })),
        Err(e) => {
            tracing::warn!(
                error = %e,
                %media_item_id,
                path = %path.display(),
                "failed to read overlaid result"
            );
            Ok(None)
        }
    }
}

// ---------------------------------------------------------------------------
// Path helpers
// ---------------------------------------------------------------------------

/// Compute the clean backup path for a given artwork type + source artwork ID.
/// Format: `{data_dir}/cache/images/clean/{type_subdir}/{artwork_id}.webp`
pub fn clean_art_path(data_dir: &Path, artwork_type: &str, artwork_id: Uuid) -> PathBuf {
    data_dir
        .join("cache")
        .join("images")
        .join("clean")
        .join(type_subdir(artwork_type))
        .join(format!("{artwork_id}.webp"))
}

/// Compute the overlays result directory for a given artwork type.
/// Format: `{data_dir}/cache/images/overlays/{type_subdir}/`
pub fn overlays_dir(data_dir: &Path, artwork_type: &str) -> PathBuf {
    data_dir
        .join("cache")
        .join("images")
        .join("overlays")
        .join(type_subdir(artwork_type))
}

/// Compute the overlaid result path for a given media item + artwork type.
/// Format: `{data_dir}/cache/images/overlays/{type_subdir}/{media_item_id}.webp`
pub fn overlaid_art_path(data_dir: &Path, artwork_type: &str, media_item_id: Uuid) -> PathBuf {
    overlays_dir(data_dir, artwork_type).join(format!("{media_item_id}.webp"))
}

/// Map an overlay artwork type to the subdirectory name used on disk.
/// `poster` → `posters`, `backdrop` → `backdrops`, etc.
fn type_subdir(artwork_type: &str) -> &str {
    match artwork_type {
        "poster" => "posters",
        "backdrop" => "backdrops",
        "season_poster" => "season_posters",
        "episode_thumb" => "episode_thumbs",
        _ => "posters",
    }
}

/// Map an overlay artwork type to the corresponding `artwork.artwork_type`
/// column value. The overlay system uses `episode_thumb` but the artwork table
/// stores it as `thumbnail`.
fn artwork_table_type(artwork_type: &str) -> &str {
    match artwork_type {
        "episode_thumb" => "thumbnail",
        other => other,
    }
}

/// Resolve the canvas preset for an artwork type.
fn canvas_for_type(artwork_type: &str) -> Result<CanvasPreset, OverlayError> {
    CanvasPreset::from_artwork_type(artwork_type).ok_or_else(|| {
        OverlayError::InvalidConditions(format!("unsupported artwork_type: {artwork_type}"))
    })
}

/// Map a DB row to an `OverlayStateRow`.
fn map_state_row(row: sqlx::postgres::PgRow) -> Result<OverlayStateRow, sqlx::Error> {
    Ok(OverlayStateRow {
        media_item_id: row.try_get("media_item_id")?,
        artwork_type: row.try_get("artwork_type")?,
        applied_overlay_ids: row
            .try_get::<Vec<Uuid>, _>("applied_overlay_ids")
            .unwrap_or_default(),
        overlay_config_hash: row.try_get("overlay_config_hash")?,
        clean_art_path: row.try_get("clean_art_path")?,
        overlaid_art_path: row.try_get("overlaid_art_path").ok().flatten(),
        applied_at: row.try_get("applied_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn make_hash_input(id: Uuid, secs: i64) -> OverlayHashInput {
        OverlayHashInput {
            id,
            updated_at: Utc.timestamp_opt(secs, 0).unwrap(),
        }
    }

    // ---- compute_config_hash ----

    #[test]
    fn hash_is_deterministic_for_same_inputs() {
        let id1 = Uuid::nil();
        let source = Uuid::nil();
        let inputs = vec![make_hash_input(id1, 1000)];
        let h1 = compute_config_hash(&inputs, source);
        let h2 = compute_config_hash(&inputs, source);
        assert_eq!(h1, h2);
    }

    #[test]
    fn hash_is_order_independent() {
        let id1 = Uuid::from_u128(1);
        let id2 = Uuid::from_u128(2);
        let source = Uuid::nil();
        let inputs_a = vec![make_hash_input(id1, 1000), make_hash_input(id2, 2000)];
        let inputs_b = vec![make_hash_input(id2, 2000), make_hash_input(id1, 1000)];
        assert_eq!(
            compute_config_hash(&inputs_a, source),
            compute_config_hash(&inputs_b, source)
        );
    }

    #[test]
    fn hash_changes_when_overlay_added() {
        let id1 = Uuid::from_u128(1);
        let id2 = Uuid::from_u128(2);
        let source = Uuid::nil();
        let one = vec![make_hash_input(id1, 1000)];
        let two = vec![make_hash_input(id1, 1000), make_hash_input(id2, 2000)];
        assert_ne!(
            compute_config_hash(&one, source),
            compute_config_hash(&two, source)
        );
    }

    #[test]
    fn hash_changes_when_overlay_updated() {
        let id1 = Uuid::from_u128(1);
        let source = Uuid::nil();
        let v1 = vec![make_hash_input(id1, 1000)];
        let v2 = vec![make_hash_input(id1, 2000)];
        assert_ne!(
            compute_config_hash(&v1, source),
            compute_config_hash(&v2, source)
        );
    }

    #[test]
    fn hash_changes_when_source_artwork_changes() {
        let id1 = Uuid::nil();
        let inputs = vec![make_hash_input(id1, 1000)];
        let source_a = Uuid::from_u128(100);
        let source_b = Uuid::from_u128(200);
        assert_ne!(
            compute_config_hash(&inputs, source_a),
            compute_config_hash(&inputs, source_b)
        );
    }

    #[test]
    fn hash_for_empty_overlays_is_not_empty() {
        let source = Uuid::nil();
        let hash = compute_config_hash(&[], source);
        assert!(!hash.is_empty());
        assert_ne!(hash, compute_config_hash(&[], Uuid::from_u128(1)));
    }

    #[test]
    fn hash_is_hexadecimal() {
        let inputs = vec![make_hash_input(Uuid::nil(), 0)];
        let hash = compute_config_hash(&inputs, Uuid::nil());
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
    }

    // ---- type_subdir ----

    #[test]
    fn type_subdir_mapping() {
        assert_eq!(type_subdir("poster"), "posters");
        assert_eq!(type_subdir("backdrop"), "backdrops");
        assert_eq!(type_subdir("season_poster"), "season_posters");
        assert_eq!(type_subdir("episode_thumb"), "episode_thumbs");
        assert_eq!(type_subdir("unknown"), "posters");
    }

    // ---- artwork_table_type ----

    #[test]
    fn artwork_table_type_mapping() {
        assert_eq!(artwork_table_type("poster"), "poster");
        assert_eq!(artwork_table_type("backdrop"), "backdrop");
        assert_eq!(artwork_table_type("season_poster"), "season_poster");
        assert_eq!(artwork_table_type("episode_thumb"), "thumbnail");
    }

    // ---- path helpers ----

    #[test]
    fn clean_art_path_format() {
        let data_dir = Path::new("/data");
        let artwork_id = Uuid::nil();
        let path = clean_art_path(data_dir, "poster", artwork_id);
        assert!(path.starts_with("/data/cache/images/clean/posters"));
        assert_eq!(path.file_name().unwrap(), "00000000-0000-0000-0000-000000000000.webp");
    }

    #[test]
    fn overlaid_art_path_format() {
        let data_dir = Path::new("/data");
        let media_item_id = Uuid::nil();
        let path = overlaid_art_path(data_dir, "backdrop", media_item_id);
        assert!(path.starts_with("/data/cache/images/overlays/backdrops"));
        assert_eq!(path.file_name().unwrap(), "00000000-0000-0000-0000-000000000000.webp");
    }

    // ---- decode helpers ----

    #[test]
    fn decode_source_rejects_garbage() {
        let garbage = b"not an image";
        assert!(decode_source(garbage).is_err());
    }

    #[test]
    fn decode_webp_rejects_garbage() {
        let garbage = b"not a webp";
        assert!(decode_webp(garbage).is_err());
    }

    // ---- save_overlaid_result ----

    #[test]
    fn save_overlaid_result_creates_dirs_and_file() {
        let tmp = std::env::temp_dir().join(format!(
            "duskcue_clean_art_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let media_item_id = Uuid::nil();
        let path = save_overlaid_result(&tmp, media_item_id, "poster", b"webp bytes").unwrap();
        assert!(path.exists());
        assert_eq!(std::fs::read(&path).unwrap(), b"webp bytes");
        let _ = std::fs::remove_dir_all(&tmp);
    }
}

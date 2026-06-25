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

//! Artwork delivery orchestration — resolve the primary artwork row for a
//! media item, serve a cached WebP variant, or generate one on demand.
//!
//! This module is the orchestration layer between the HTTP handler (thin
//! binary serving in [`crate::domains::media::handlers`] and the stateless
//! encoding library ([`crate::services::image_pipeline`]). It owns:
//!
//! - Querying the `artwork` table for the primary artwork of a given type
//!   (`order = 0` — the best artwork by TMDb vote count)
//! - Disk cache lookup at `{data_dir}/cache/images/webp/{category}/{variant}/{stem}.webp`
//! - On-demand variant generation on cache miss (decode source → resize →
//!   encode WebP → write to cache)
//!
//! Per [IMAGE_FORMATS.md](../../docs/design/IMAGE_FORMATS.md) "Conversion
//! Pipeline": the background-first strategy pre-warms the cache after scans;
//! on-demand generation is the cache-miss fallback with a <500ms latency
//! budget per image.

use std::path::{Path, PathBuf};

use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::domains::media::MediaError;
use crate::services::clean_art;
use crate::services::image_pipeline::{self, ArtworkCategory, EncodeConfig};

/// The resolved bytes and identity of a served artwork variant.
///
/// `artwork_id` is the UUID of the `artwork` table row — used by the handler
/// to construct a strong `ETag` (`"{artwork_id}-{variant_label}"`). The
/// encoding is deterministic, so the same source + variant always yields
/// identical bytes; when TMDb refresh replaces artwork, a new row (new UUID)
/// is created, naturally invalidating the old ETag.
pub struct ResolvedArtwork {
    pub bytes: Vec<u8>,
    pub artwork_id: Uuid,
}

/// Resolve (and serve or generate) the WebP variant bytes for a media item's
/// primary artwork of the given category and size.
///
/// Flow:
/// 1. Query `artwork` for the primary row (`order = 0`) matching the category.
/// 2. If no row → `ArtworkNotFound` (MEDIA_004, 404).
/// 3. Compute the cache path using the artwork row UUID as the stem.
/// 4. Cache hit → read and return bytes.
/// 5. Cache miss → read the source original from `local_path`, generate the
///    variant via `image_pipeline::generate_variant`, best-effort write to
///    cache (failures logged but do not block serving), return bytes.
/// 6. Source file missing or corrupt → `ArtworkNotFound` (404) per
///    IMAGE_FORMATS.md edge-case policy.
pub async fn resolve_variant(
    pool: &PgPool,
    media_item_id: Uuid,
    category: ArtworkCategory,
    variant_label: &str,
    images_cache_root: &Path,
    encode_config: &EncodeConfig,
) -> Result<ResolvedArtwork, MediaError> {
    let overlay_type = overlay_artwork_type(category);
    let overlaid = if let Some(ot) = overlay_type {
        clean_art::resolve_overlaid_artwork(pool, media_item_id, ot).await?
    } else {
        None
    };

    if let Some(overlaid) = overlaid {
        let stem = format!("{}_overlay", overlaid.artwork_id);
        let cache_path = image_pipeline::variant_path(
            images_cache_root,
            category,
            variant_label,
            &stem,
        );

        if let Some(bytes) = try_read_cache(&cache_path).await {
            return Ok(ResolvedArtwork { bytes, artwork_id: overlaid.artwork_id });
        }

        let variant = image_pipeline::generate_variant(
            &overlaid.bytes,
            category,
            variant_label,
            encode_config,
        )
        .map_err(|e| {
            tracing::warn!(
                error = %e,
                %media_item_id,
                "overlaid variant generation failed"
            );
            MediaError::ArtworkNotFound
        })?;

        if let Err(e) = image_pipeline::write_variant(
            images_cache_root,
            category,
            &stem,
            &variant,
        ) {
            tracing::warn!(
                error = %e,
                path = %cache_path.display(),
                "failed to cache overlaid variant; serving uncached"
            );
        }

        return Ok(ResolvedArtwork {
            bytes: variant.bytes,
            artwork_id: overlaid.artwork_id,
        });
    }

    let row = sqlx::query(
        r#"SELECT id, local_path FROM artwork
           WHERE media_item_id = $1 AND artwork_type = $2 AND "order" = 0
           LIMIT 1"#,
    )
    .bind(media_item_id)
    .bind(category.as_str())
    .fetch_optional(pool)
    .await?;

    let row = row.ok_or(MediaError::ArtworkNotFound)?;

    let artwork_id: Uuid = row.get("id");
    let local_path_str: Option<String> = row.get("local_path");
    let local_path = match local_path_str {
        Some(p) if !p.is_empty() => PathBuf::from(p),
        _ => return Err(MediaError::ArtworkNotFound),
    };

    let stem = artwork_id.to_string();
    let cache_path = image_pipeline::variant_path(
        images_cache_root,
        category,
        variant_label,
        &stem,
    );

    if let Some(bytes) = try_read_cache(&cache_path).await {
        return Ok(ResolvedArtwork { bytes, artwork_id });
    }

    let source_bytes = tokio::fs::read(&local_path)
        .await
        .map_err(|e| {
            tracing::warn!(
                error = %e,
                %media_item_id,
                path = %local_path.display(),
                "artwork source file unreadable"
            );
            MediaError::ArtworkNotFound
        })?;

    let variant = image_pipeline::generate_variant(
        &source_bytes,
        category,
        variant_label,
        encode_config,
    )
    .map_err(|e| {
        tracing::warn!(
            error = %e,
            %media_item_id,
            path = %local_path.display(),
            "artwork variant generation failed (corrupt source?)"
        );
        MediaError::ArtworkNotFound
    })?;

    if let Err(e) = image_pipeline::write_variant(
        images_cache_root,
        category,
        &stem,
        &variant,
    ) {
        tracing::warn!(
            error = %e,
            path = %cache_path.display(),
            "failed to cache artwork variant; serving uncached"
        );
    }

    Ok(ResolvedArtwork {
        bytes: variant.bytes,
        artwork_id,
    })
}

/// Default variant label for a category when the client omits `?size=`.
///
/// Per IMAGE_FORMATS.md sizing rationale: the default balances quality and
/// bandwidth for the most common display context of each type.
pub fn default_variant_label(category: ArtworkCategory) -> &'static str {
    match category {
        ArtworkCategory::Poster | ArtworkCategory::SeasonPoster => "w342",
        ArtworkCategory::Backdrop | ArtworkCategory::Banner => "w780",
        ArtworkCategory::Thumbnail => "w300",
        ArtworkCategory::Logo => "original",
    }
}

async fn try_read_cache(path: &Path) -> Option<Vec<u8>> {
    tokio::fs::read(path).await.ok()
}

/// Map an artwork category to the overlay system's artwork type vocabulary.
/// Returns `None` for categories that don't participate in the overlay system
/// (logos, banners).
fn overlay_artwork_type(category: ArtworkCategory) -> Option<&'static str> {
    match category {
        ArtworkCategory::Poster => Some("poster"),
        ArtworkCategory::Backdrop => Some("backdrop"),
        ArtworkCategory::SeasonPoster => Some("season_poster"),
        ArtworkCategory::Thumbnail => Some("episode_thumb"),
        ArtworkCategory::Logo | ArtworkCategory::Banner => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_labels_are_valid_variants() {
        for category in [
            ArtworkCategory::Poster,
            ArtworkCategory::Backdrop,
            ArtworkCategory::Thumbnail,
            ArtworkCategory::Logo,
            ArtworkCategory::Banner,
            ArtworkCategory::SeasonPoster,
        ] {
            let label = default_variant_label(category);
            assert!(
                image_pipeline::resolve_variant(category, label).is_some(),
                "default label {label} is not a valid variant for {category:?}"
            );
        }
    }

    #[test]
    fn default_poster_is_w342() {
        assert_eq!(default_variant_label(ArtworkCategory::Poster), "w342");
    }

    #[test]
    fn default_backdrop_is_w780() {
        assert_eq!(default_variant_label(ArtworkCategory::Backdrop), "w780");
    }

    #[test]
    fn default_logo_is_original() {
        assert_eq!(default_variant_label(ArtworkCategory::Logo), "original");
    }

    #[test]
    fn default_season_poster_shares_poster_default() {
        assert_eq!(
            default_variant_label(ArtworkCategory::SeasonPoster),
            "w342"
        );
    }
}

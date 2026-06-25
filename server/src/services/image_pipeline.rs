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

//! Shared image processing pipeline — decode, resize, and WebP encode.
//!
//! Stateless library functions for generating WebP delivery variants from
//! source artwork (JPEG/PNG/WebP). The domain layer (Task 10 artwork
//! endpoint) and the background worker (future `artwork_variant_generator`
//! scheduled task) are the orchestration points that read source bytes from
//! disk, call [`generate_variant`] / [`generate_variants`], and persist
//! results under `/cache/images/webp/`.
//!
//! ## Pipeline overview
//!
//! 1. Decode source bytes (`image::load_from_memory`) → `DynamicImage`.
//! 2. For each size variant (smallest first per IMAGE_FORMATS.md), resize to
//!    target width preserving aspect ratio (Lanczos3 filter), skipping resize
//!    when the target ≥ source width (never upscale).
//! 3. Encode to WebP — lossy for opaque images, lossless for images with
//!    alpha (logos/clearart per the format policy).
//! 3. Return the variant bytes; the caller writes them to disk.
//!
//! ## Alpha-aware encoding
//!
//! Per [IMAGE_FORMATS.md](../../docs/design/IMAGE_FORMATS.md) "Encoding
//! Settings": photographic content (posters, backdrops) is encoded lossy at
//! quality 90; transparent content (logos, clearart with alpha channel) is
//! encoded lossless to preserve transparency exactly. The pipeline detects
//! alpha via `ColorType::has_alpha()` and switches modes automatically —
//! category is a hint for the variant catalog, not the encode mode.
//!
//! ## Crate strategy
//!
//! `image` 0.25 (decode + resize) is decoupled from `webp` 0.3 (encode) by
//! disabling the `webp` crate's `img` feature and passing raw RGBA/RGB bytes
//! to `Encoder::from_rgba` / `Encoder::from_rgb`. The two crates evolve
//! independently; no version coupling.

use std::path::{Path, PathBuf};

use image::imageops::FilterType;
use image::{DynamicImage, GenericImageView};
use thiserror::Error;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const DEFAULT_LOSSY_QUALITY: f32 = 90.0;

const POSTER_VARIANTS: &[SizeVariant] = &[
    SizeVariant { label: "w185", target_width: Some(185) },
    SizeVariant { label: "w342", target_width: Some(342) },
    SizeVariant { label: "w500", target_width: Some(500) },
    SizeVariant { label: "original", target_width: None },
];

const BACKDROP_VARIANTS: &[SizeVariant] = &[
    SizeVariant { label: "w300", target_width: Some(300) },
    SizeVariant { label: "w780", target_width: Some(780) },
    SizeVariant { label: "w1280", target_width: Some(1280) },
    SizeVariant { label: "original", target_width: None },
];

const THUMBNAIL_VARIANTS: &[SizeVariant] = &[
    SizeVariant { label: "w185", target_width: Some(185) },
    SizeVariant { label: "w300", target_width: Some(300) },
    SizeVariant { label: "original", target_width: None },
];

const LOGO_VARIANTS: &[SizeVariant] = &[
    SizeVariant { label: "original", target_width: None },
];

const BANNER_VARIANTS: &[SizeVariant] = &[
    SizeVariant { label: "w300", target_width: Some(300) },
    SizeVariant { label: "w780", target_width: Some(780) },
    SizeVariant { label: "original", target_width: None },
];

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArtworkCategory {
    Poster,
    Backdrop,
    Thumbnail,
    Logo,
    Banner,
    SeasonPoster,
}

impl ArtworkCategory {
    pub fn as_str(self) -> &'static str {
        match self {
            ArtworkCategory::Poster => "poster",
            ArtworkCategory::Backdrop => "backdrop",
            ArtworkCategory::Thumbnail => "thumbnail",
            ArtworkCategory::Logo => "logo",
            ArtworkCategory::Banner => "banner",
            ArtworkCategory::SeasonPoster => "season_poster",
        }
    }

    pub fn from_db_str(s: &str) -> Option<Self> {
        match s {
            "poster" => Some(ArtworkCategory::Poster),
            "backdrop" => Some(ArtworkCategory::Backdrop),
            "thumbnail" => Some(ArtworkCategory::Thumbnail),
            "logo" => Some(ArtworkCategory::Logo),
            "banner" => Some(ArtworkCategory::Banner),
            "season_poster" => Some(ArtworkCategory::SeasonPoster),
            _ => None,
        }
    }

    pub fn subdir(self) -> &'static str {
        match self {
            ArtworkCategory::Poster => "posters",
            ArtworkCategory::Backdrop => "backdrops",
            ArtworkCategory::Thumbnail => "thumbnails",
            ArtworkCategory::Logo => "logos",
            ArtworkCategory::Banner => "banners",
            ArtworkCategory::SeasonPoster => "season_posters",
        }
    }

    pub fn variants(self) -> &'static [SizeVariant] {
        match self {
            ArtworkCategory::Poster | ArtworkCategory::SeasonPoster => POSTER_VARIANTS,
            ArtworkCategory::Backdrop => BACKDROP_VARIANTS,
            ArtworkCategory::Thumbnail => THUMBNAIL_VARIANTS,
            ArtworkCategory::Logo => LOGO_VARIANTS,
            ArtworkCategory::Banner => BANNER_VARIANTS,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SizeVariant {
    pub label: &'static str,
    pub target_width: Option<u32>,
}

#[derive(Debug, Clone)]
pub struct EncodeConfig {
    pub lossy_quality: f32,
}

impl Default for EncodeConfig {
    fn default() -> Self {
        Self {
            lossy_quality: DEFAULT_LOSSY_QUALITY,
        }
    }
}

#[derive(Debug, Clone)]
pub struct GeneratedVariant {
    pub label: &'static str,
    pub width: u32,
    pub height: u32,
    pub bytes: Vec<u8>,
    pub lossless: bool,
}

#[derive(Debug, Clone)]
pub struct VariantResult {
    pub source_width: u32,
    pub source_height: u32,
    pub has_alpha: bool,
    pub variants: Vec<GeneratedVariant>,
}

#[derive(Error, Debug)]
pub enum ImagePipelineError {
    #[error("failed to decode source image: {0}")]
    Decode(String),

    #[error("failed to encode WebP: {0}")]
    Encode(String),

    #[error("invalid variant `{0}` for category")]
    InvalidVariant(String),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

// ---------------------------------------------------------------------------
// Variant resolution
// ---------------------------------------------------------------------------

pub fn resolve_variant(category: ArtworkCategory, label: &str) -> Option<SizeVariant> {
    category
        .variants()
        .iter()
        .copied()
        .find(|v| v.label == label)
}

// ---------------------------------------------------------------------------
// Core pipeline
// ---------------------------------------------------------------------------

fn decode_source(bytes: &[u8]) -> Result<DynamicImage, ImagePipelineError> {
    image::load_from_memory(bytes).map_err(|e| ImagePipelineError::Decode(e.to_string()))
}

fn resize_to_width(img: &DynamicImage, target_width: u32) -> DynamicImage {
    let (src_w, src_h) = img.dimensions();
    if target_width >= src_w {
        return img.clone();
    }
    let target_height = ((u64::from(target_width) * u64::from(src_h) / u64::from(src_w)) as u32).max(1);
    img.resize_exact(target_width, target_height, FilterType::Lanczos3)
}

pub fn encode_webp(
    img: &DynamicImage,
    config: &EncodeConfig,
) -> Result<(Vec<u8>, bool), ImagePipelineError> {
    let has_alpha = img.color().has_alpha();
    let width = img.width();
    let height = img.height();

    let memory = if has_alpha {
        let rgba = img.to_rgba8().into_raw();
        let encoder = webp::Encoder::from_rgba(&rgba, width, height);
        encoder
            .encode_simple(true, 100.0)
            .map_err(|e| ImagePipelineError::Encode(format!("{e:?}")))?
    } else {
        let rgb = img.to_rgb8().into_raw();
        let encoder = webp::Encoder::from_rgb(&rgb, width, height);
        encoder
            .encode_simple(false, config.lossy_quality)
            .map_err(|e| ImagePipelineError::Encode(format!("{e:?}")))?
    };

    Ok((memory.to_vec(), has_alpha))
}

pub fn generate_variant(
    source_bytes: &[u8],
    category: ArtworkCategory,
    label: &str,
    config: &EncodeConfig,
) -> Result<GeneratedVariant, ImagePipelineError> {
    let variant = resolve_variant(category, label)
        .ok_or_else(|| ImagePipelineError::InvalidVariant(label.to_string()))?;

    let source = decode_source(source_bytes)?;
    let resized = match variant.target_width {
        Some(target) => resize_to_width(&source, target),
        None => source.clone(),
    };

    let (bytes, lossless) = encode_webp(&resized, config)?;
    let (width, height) = resized.dimensions();

    Ok(GeneratedVariant {
        label: variant.label,
        width,
        height,
        bytes,
        lossless,
    })
}

pub fn generate_variants(
    source_bytes: &[u8],
    category: ArtworkCategory,
    config: &EncodeConfig,
) -> Result<VariantResult, ImagePipelineError> {
    let source = decode_source(source_bytes)?;
    let (source_width, source_height) = source.dimensions();
    let has_alpha = source.color().has_alpha();

    let mut variants = Vec::with_capacity(category.variants().len());

    for variant in category.variants() {
        let resized = match variant.target_width {
            Some(target) => resize_to_width(&source, target),
            None => source.clone(),
        };

        let (bytes, lossless) = encode_webp(&resized, config)?;
        let (width, height) = resized.dimensions();

        variants.push(GeneratedVariant {
            label: variant.label,
            width,
            height,
            bytes,
            lossless,
        });
    }

    Ok(VariantResult {
        source_width,
        source_height,
        has_alpha,
        variants,
    })
}

// ---------------------------------------------------------------------------
// Disk layout
// ---------------------------------------------------------------------------

pub fn variant_path(
    images_cache_root: &Path,
    category: ArtworkCategory,
    variant_label: &str,
    source_stem: &str,
) -> PathBuf {
    images_cache_root
        .join("webp")
        .join(category.subdir())
        .join(variant_label)
        .join(format!("{source_stem}.webp"))
}

pub fn write_variant(
    images_cache_root: &Path,
    category: ArtworkCategory,
    source_stem: &str,
    variant: &GeneratedVariant,
) -> Result<PathBuf, ImagePipelineError> {
    let path = variant_path(
        images_cache_root,
        category,
        variant.label,
        source_stem,
    );

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    std::fs::write(&path, &variant.bytes)?;
    Ok(path)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageBuffer, Rgb, Rgba};
    use std::io::Cursor;

    fn make_rgba(width: u32, height: u32) -> DynamicImage {
        let buf: ImageBuffer<Rgba<u8>, Vec<u8>> =
            ImageBuffer::from_fn(width, height, |x, y| {
                Rgba([(x * 30) as u8, (y * 30) as u8, 128, if x % 2 == 0 { 255 } else { 128 }])
            });
        DynamicImage::ImageRgba8(buf)
    }

    fn make_rgb(width: u32, height: u32) -> DynamicImage {
        let buf: ImageBuffer<Rgb<u8>, Vec<u8>> =
            ImageBuffer::from_fn(width, height, |x, y| {
                Rgb([(x * 30) as u8, (y * 30) as u8, 128])
            });
        DynamicImage::ImageRgb8(buf)
    }

    fn encode_png(img: &DynamicImage) -> Vec<u8> {
        let mut buf = Cursor::new(Vec::new());
        img.write_to(&mut buf, image::ImageFormat::Png).unwrap();
        buf.into_inner()
    }

    fn encode_jpeg(img: &DynamicImage) -> Vec<u8> {
        let mut buf = Cursor::new(Vec::new());
        img.write_to(&mut buf, image::ImageFormat::Jpeg).unwrap();
        buf.into_inner()
    }

    fn is_webp(bytes: &[u8]) -> bool {
        bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP"
    }

    // ---- ArtworkCategory ----

    #[test]
    fn category_round_trip() {
        for cat in [
            ArtworkCategory::Poster,
            ArtworkCategory::Backdrop,
            ArtworkCategory::Thumbnail,
            ArtworkCategory::Logo,
            ArtworkCategory::Banner,
            ArtworkCategory::SeasonPoster,
        ] {
            let s = cat.as_str();
            assert_eq!(ArtworkCategory::from_db_str(s), Some(cat));
        }
    }

    #[test]
    fn category_from_unknown() {
        assert_eq!(ArtworkCategory::from_db_str("unknown"), None);
        assert_eq!(ArtworkCategory::from_db_str(""), None);
    }

    #[test]
    fn category_subdir_plural() {
        assert_eq!(ArtworkCategory::Poster.subdir(), "posters");
        assert_eq!(ArtworkCategory::Backdrop.subdir(), "backdrops");
        assert_eq!(ArtworkCategory::Thumbnail.subdir(), "thumbnails");
        assert_eq!(ArtworkCategory::Logo.subdir(), "logos");
        assert_eq!(ArtworkCategory::Banner.subdir(), "banners");
        assert_eq!(ArtworkCategory::SeasonPoster.subdir(), "season_posters");
    }

    // ---- Variant catalog ----

    #[test]
    fn poster_variants_match_tmdb_catalog() {
        let labels: Vec<_> = ArtworkCategory::Poster
            .variants()
            .iter()
            .map(|v| v.label)
            .collect();
        assert_eq!(labels, vec!["w185", "w342", "w500", "original"]);
    }

    #[test]
    fn backdrop_variants_match_tmdb_catalog() {
        let labels: Vec<_> = ArtworkCategory::Backdrop
            .variants()
            .iter()
            .map(|v| v.label)
            .collect();
        assert_eq!(labels, vec!["w300", "w780", "w1280", "original"]);
    }

    #[test]
    fn thumbnail_variants() {
        let labels: Vec<_> = ArtworkCategory::Thumbnail
            .variants()
            .iter()
            .map(|v| v.label)
            .collect();
        assert_eq!(labels, vec!["w185", "w300", "original"]);
    }

    #[test]
    fn logo_variants_single_original() {
        let labels: Vec<_> = ArtworkCategory::Logo
            .variants()
            .iter()
            .map(|v| v.label)
            .collect();
        assert_eq!(labels, vec!["original"]);
    }

    #[test]
    fn season_poster_shares_poster_variants() {
        assert_eq!(
            ArtworkCategory::SeasonPoster.variants(),
            ArtworkCategory::Poster.variants(),
        );
    }

    #[test]
    fn banner_variants() {
        let labels: Vec<_> = ArtworkCategory::Banner
            .variants()
            .iter()
            .map(|v| v.label)
            .collect();
        assert_eq!(labels, vec!["w300", "w780", "original"]);
    }

    // ---- resolve_variant ----

    #[test]
    fn resolve_valid_variant() {
        let v = resolve_variant(ArtworkCategory::Poster, "w185").unwrap();
        assert_eq!(v.label, "w185");
        assert_eq!(v.target_width, Some(185));
    }

    #[test]
    fn resolve_original_variant() {
        let v = resolve_variant(ArtworkCategory::Backdrop, "original").unwrap();
        assert_eq!(v.label, "original");
        assert_eq!(v.target_width, None);
    }

    #[test]
    fn resolve_unknown_variant_for_category() {
        assert_eq!(resolve_variant(ArtworkCategory::Logo, "w185"), None);
        assert_eq!(resolve_variant(ArtworkCategory::Poster, "w1000"), None);
    }

    // ---- decode_source ----

    #[test]
    fn decode_png_source() {
        let img = make_rgb(100, 150);
        let png = encode_png(&img);
        let decoded = decode_source(&png).unwrap();
        assert_eq!(decoded.dimensions(), (100, 150));
    }

    #[test]
    fn decode_jpeg_source() {
        let img = make_rgb(80, 60);
        let jpeg = encode_jpeg(&img);
        let decoded = decode_source(&jpeg).unwrap();
        assert_eq!(decoded.dimensions(), (80, 60));
    }

    #[test]
    fn decode_garbage_fails() {
        let result = decode_source(b"not an image");
        assert!(result.is_err());
    }

    // ---- resize_to_width ----

    #[test]
    fn resize_narrows_width_preserving_aspect() {
        let img = make_rgb(400, 200);
        let resized = resize_to_width(&img, 200);
        assert_eq!(resized.dimensions(), (200, 100));
    }

    #[test]
    fn resize_preserves_aspect_with_odd_ratio() {
        let img = make_rgb(1000, 300);
        let resized = resize_to_width(&img, 200);
        assert_eq!(resized.dimensions(), (200, 60));
    }

    #[test]
    fn resize_no_upscale_when_target_exceeds_source() {
        let img = make_rgb(100, 50);
        let resized = resize_to_width(&img, 200);
        assert_eq!(resized.dimensions(), (100, 50));
    }

    #[test]
    fn resize_no_op_when_target_equals_source() {
        let img = make_rgb(200, 100);
        let resized = resize_to_width(&img, 200);
        assert_eq!(resized.dimensions(), (200, 100));
    }

    // ---- encode_webp ----

    #[test]
    fn encode_rgb_lossy() {
        let img = make_rgb(40, 30);
        let config = EncodeConfig::default();
        let (bytes, lossless) = encode_webp(&img, &config).unwrap();
        assert!(is_webp(&bytes));
        assert!(!lossless);
        assert!(!bytes.is_empty());
    }

    #[test]
    fn encode_rgba_lossless() {
        let img = make_rgba(40, 30);
        let config = EncodeConfig::default();
        let (bytes, lossless) = encode_webp(&img, &config).unwrap();
        assert!(is_webp(&bytes));
        assert!(lossless);
    }

    #[test]
    fn encode_quality_does_not_affect_lossless() {
        let img = make_rgba(20, 20);
        let low = EncodeConfig { lossy_quality: 10.0 };
        let high = EncodeConfig { lossy_quality: 100.0 };
        let (bytes_low, _) = encode_webp(&img, &low).unwrap();
        let (bytes_high, _) = encode_webp(&img, &high).unwrap();
        assert!(is_webp(&bytes_low));
        assert!(is_webp(&bytes_high));
    }

    // ---- generate_variant ----

    #[test]
    fn generate_single_poster_w185() {
        let img = make_rgb(1000, 1500);
        let png = encode_png(&img);
        let variant = generate_variant(
            &png,
            ArtworkCategory::Poster,
            "w185",
            &EncodeConfig::default(),
        )
        .unwrap();
        assert_eq!(variant.label, "w185");
        assert_eq!(variant.width, 185);
        assert!((variant.height - 277) <= 1);
        assert!(!variant.lossless);
        assert!(is_webp(&variant.bytes));
    }

    #[test]
    fn generate_original_variant_no_resize() {
        let img = make_rgb(500, 750);
        let png = encode_png(&img);
        let variant = generate_variant(
            &png,
            ArtworkCategory::Poster,
            "original",
            &EncodeConfig::default(),
        )
        .unwrap();
        assert_eq!(variant.width, 500);
        assert_eq!(variant.height, 750);
    }

    #[test]
    fn generate_variant_no_upscale_for_small_source() {
        let img = make_rgb(100, 150);
        let png = encode_png(&img);
        let variant = generate_variant(
            &png,
            ArtworkCategory::Poster,
            "w500",
            &EncodeConfig::default(),
        )
        .unwrap();
        assert_eq!(variant.width, 100);
        assert_eq!(variant.height, 150);
    }

    #[test]
    fn generate_variant_rejects_invalid_label() {
        let img = make_rgb(100, 150);
        let png = encode_png(&img);
        let result = generate_variant(
            &png,
            ArtworkCategory::Poster,
            "w1000",
            &EncodeConfig::default(),
        );
        assert!(matches!(result, Err(ImagePipelineError::InvalidVariant(_))));
    }

    #[test]
    fn generate_logo_variant_is_lossless() {
        let img = make_rgba(200, 100);
        let png = encode_png(&img);
        let variant = generate_variant(
            &png,
            ArtworkCategory::Logo,
            "original",
            &EncodeConfig::default(),
        )
        .unwrap();
        assert!(variant.lossless);
        assert!(is_webp(&variant.bytes));
    }

    // ---- generate_variants ----

    #[test]
    fn generate_all_poster_variants() {
        let img = make_rgb(1000, 1500);
        let png = encode_png(&img);
        let result = generate_variants(
            &png,
            ArtworkCategory::Poster,
            &EncodeConfig::default(),
        )
        .unwrap();
        assert_eq!(result.variants.len(), 4);
        assert_eq!(result.source_width, 1000);
        assert_eq!(result.source_height, 1500);
        assert!(!result.has_alpha);
    }

    #[test]
    fn generate_all_variants_smallest_first() {
        let img = make_rgb(1000, 1500);
        let png = encode_png(&img);
        let result = generate_variants(
            &png,
            ArtworkCategory::Poster,
            &EncodeConfig::default(),
        )
        .unwrap();
        let labels: Vec<_> = result.variants.iter().map(|v| v.label).collect();
        assert_eq!(labels, vec!["w185", "w342", "w500", "original"]);
    }

    #[test]
    fn generate_all_backdrop_variants() {
        let img = make_rgb(3840, 2160);
        let png = encode_png(&img);
        let result = generate_variants(
            &png,
            ArtworkCategory::Backdrop,
            &EncodeConfig::default(),
        )
        .unwrap();
        assert_eq!(result.variants.len(), 4);
        let widths: Vec<_> = result.variants.iter().map(|v| v.width).collect();
        assert_eq!(widths, vec![300, 780, 1280, 3840]);
    }

    #[test]
    fn generate_all_logo_variants_single() {
        let img = make_rgba(500, 200);
        let png = encode_png(&img);
        let result = generate_variants(
            &png,
            ArtworkCategory::Logo,
            &EncodeConfig::default(),
        )
        .unwrap();
        assert_eq!(result.variants.len(), 1);
        assert!(result.has_alpha);
        assert!(result.variants[0].lossless);
    }

    #[test]
    fn generate_all_variants_all_webp() {
        let img = make_rgb(800, 600);
        let png = encode_png(&img);
        let result = generate_variants(
            &png,
            ArtworkCategory::Thumbnail,
            &EncodeConfig::default(),
        )
        .unwrap();
        for v in &result.variants {
            assert!(is_webp(&v.bytes), "variant {} is not WebP", v.label);
        }
    }

    // ---- variant_path ----

    #[test]
    fn variant_path_layout() {
        let root = Path::new("/cache/images");
        let path = variant_path(root, ArtworkCategory::Poster, "w185", "272_abc");
        assert_eq!(
            path,
            Path::new("/cache/images/webp/posters/w185/272_abc.webp")
        );
    }

    #[test]
    fn variant_path_backdrop() {
        let root = Path::new("/cache/images");
        let path = variant_path(root, ArtworkCategory::Backdrop, "w780", "550_def");
        assert_eq!(
            path,
            Path::new("/cache/images/webp/backdrops/w780/550_def.webp")
        );
    }

    #[test]
    fn variant_path_logo() {
        let root = Path::new("/cache/images");
        let path = variant_path(root, ArtworkCategory::Logo, "original", "x");
        assert_eq!(
            path,
            Path::new("/cache/images/webp/logos/original/x.webp")
        );
    }

    // ---- write_variant ----

    #[test]
    fn write_variant_creates_file() {
        let dir = std::env::temp_dir().join("duskcue_imgpipe_test");
        let variant = GeneratedVariant {
            label: "w185",
            width: 185,
            height: 277,
            bytes: b"fake-webp".to_vec(),
            lossless: false,
        };
        let path = write_variant(&dir, ArtworkCategory::Poster, "stem1", &variant).unwrap();
        assert!(path.exists());
        assert_eq!(std::fs::read(&path).unwrap(), b"fake-webp");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn write_variant_creates_nested_dirs() {
        let dir = std::env::temp_dir().join("duskcue_imgpipe_nested");
        let _ = std::fs::remove_dir_all(&dir);
        let variant = GeneratedVariant {
            label: "original",
            width: 10,
            height: 10,
            bytes: vec![1, 2, 3],
            lossless: true,
        };
        let path = write_variant(&dir, ArtworkCategory::Logo, "stem2", &variant).unwrap();
        assert!(path.exists());
        assert_eq!(
            path,
            dir.join("webp").join("logos").join("original").join("stem2.webp")
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    // ---- EncodeConfig ----

    #[test]
    fn default_config_quality_90() {
        let config = EncodeConfig::default();
        assert!((config.lossy_quality - 90.0).abs() < f32::EPSILON);
    }
}

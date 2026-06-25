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

//! Overlay compositing pipeline — composite badges, text, and backdrops onto artwork.
//!
//! Stateless library that takes a source image, a list of resolved overlays,
//! and a font registry, and returns the composited [`RgbaImage`]. The domain
//! layer (`domains::overlays::service`) and the future `overlay_compositor`
//! worker (Task 8) are the orchestration points — they load artwork bytes,
//! resolve overlay definitions, evaluate conditions, resolve groups/suppress/
//! queues, substitute text variables from media-item context, and then call
//! [`composite`] to produce the final image.
//!
//! ## Pipeline overview
//!
//! 1. Scale the source artwork to a standard canvas (poster 1000×1500 or
//!    backdrop 1920×1080) via Lanczos3, no upscaling.
//! 2. Sort overlays by layer order: backdrops first, then images, then text.
//! 3. For each overlay, compute its absolute `(x, y)` position on the canvas
//!    from the align/offset attributes and the overlay's natural dimensions.
//! 4. Composite: backdrop fills, image alpha-blending, text rasterization.
//! 5. Return the `RgbaImage`; the caller encodes to WebP via `image_pipeline`.
//!
//! ## Text rendering
//!
//! Uses [`ab_glyph`] for glyph rasterization — pure Rust, no system font
//! dependency. Fonts are loaded from `/data/fonts/` by the [`FontRegistry`],
//! keyed by lowercased filename stem. Text template variables (`<<title>>`,
//! `<<resolution>>`, etc.) are resolved by the domain layer **before** the
//! `text` string reaches this service.
//!
//! ## SVG support
//!
//! Image overlays may be PNG or SVG. SVG rendering uses [`resvg`] 0.47 (pure
//! Rust via `tiny-skia`). The resulting premultiplied `tiny_skia::Pixmap` is
//! converted to non-premultiplied `RgbaImage` before alpha-blending onto the
//! canvas.
//!
//! ## Error model
//!
//! [`OverlayPipelineError`] is separate from the domain [`OverlayError`].
//! Pipeline failures (decode, font load, SVG parse) are operational errors
//! that the worker logs and skips. The domain layer translates
//! `OverlayPipelineError` to `OverlayError::CompositingFailed` for API
//! responses — matching the `segments`/`storyboards` precedent.

use std::collections::HashMap;
use std::path::Path;

use ab_glyph::{Font, FontArc, GlyphId, Point, ScaleFont};
use image::imageops::{FilterType, overlay, resize};
use image::{GenericImage, ImageBuffer, Rgba, RgbaImage};
use thiserror::Error;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const POSTER_CANVAS: (u32, u32) = (1000, 1500);
const BACKDROP_CANVAS: (u32, u32) = (1920, 1080);
pub const DEFAULT_QUEUE_SPACING: u32 = 8;

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

#[derive(Error, Debug)]
pub enum OverlayPipelineError {
    #[error("failed to decode source image: {0}")]
    Decode(String),

    #[error("failed to load font `{family}`: {reason}")]
    FontLoad { family: String, reason: String },

    #[error("no fonts available — place at least one .ttf/.otf in /data/fonts/")]
    NoFontAvailable,

    #[error("failed to parse SVG overlay: {0}")]
    SvgParse(String),

    #[error("invalid color `{0}` — expected #RGB, #RGBA, #RRGGBB, or #RRGGBBAA")]
    InvalidColor(String),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

// ---------------------------------------------------------------------------
// Enums
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverlayType {
    Image,
    Text,
    Backdrop,
}

impl OverlayType {
    pub fn from_db_str(s: &str) -> Option<Self> {
        match s {
            "image" => Some(Self::Image),
            "text" => Some(Self::Text),
            "backdrop" => Some(Self::Backdrop),
            _ => None,
        }
    }

    fn layer_order(self) -> u8 {
        match self {
            Self::Backdrop => 0,
            Self::Image => 1,
            Self::Text => 2,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HorizontalAlignment {
    Left,
    Center,
    Right,
}

impl HorizontalAlignment {
    pub fn from_db_str(s: &str) -> Option<Self> {
        match s {
            "left" => Some(Self::Left),
            "center" => Some(Self::Center),
            "right" => Some(Self::Right),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerticalAlignment {
    Top,
    Center,
    Bottom,
}

impl VerticalAlignment {
    pub fn from_db_str(s: &str) -> Option<Self> {
        match s {
            "top" => Some(Self::Top),
            "center" => Some(Self::Center),
            "bottom" => Some(Self::Bottom),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CanvasPreset {
    Poster,
    Backdrop,
}

impl CanvasPreset {
    pub fn dimensions(self) -> (u32, u32) {
        match self {
            Self::Poster => POSTER_CANVAS,
            Self::Backdrop => BACKDROP_CANVAS,
        }
    }

    pub fn from_artwork_type(s: &str) -> Option<Self> {
        match s {
            "poster" | "season_poster" => Some(Self::Poster),
            "backdrop" | "episode_thumb" => Some(Self::Backdrop),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// Input types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct ResolvedOverlay {
    pub id: Uuid,
    pub slug: String,
    pub overlay_type: OverlayType,

    pub horizontal_align: HorizontalAlignment,
    pub horizontal_offset: i32,
    pub vertical_align: VerticalAlignment,
    pub vertical_offset: i32,

    pub group_name: Option<String>,
    pub weight: i32,
    pub queue_name: Option<String>,
    pub suppresses: Vec<String>,

    pub image_bytes: Option<Vec<u8>>,
    pub image_is_svg: bool,
    pub scale_width: Option<u32>,
    pub scale_height: Option<u32>,

    pub text: Option<String>,
    pub font_family: String,
    pub font_size: f32,
    pub font_color: Rgba<u8>,
    pub stroke_color: Option<Rgba<u8>>,
    pub stroke_width: u32,

    pub back_color: Option<Rgba<u8>>,
    pub back_width: Option<u32>,
    pub back_height: Option<u32>,
    pub back_radius: u32,
    pub back_padding: u32,
}

// ---------------------------------------------------------------------------
// Font registry
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
pub struct FontRegistry {
    fonts: HashMap<String, FontArc>,
    first_family: Option<String>,
}

impl FontRegistry {
    pub fn empty() -> Self {
        Self::default()
    }

    pub fn scan_dir(dir: &Path) -> Self {
        let mut registry = Self::empty();
        let Ok(entries) = std::fs::read_dir(dir) else {
            tracing::warn!("font directory not readable: {}", dir.display());
            return registry;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let Some(ext) = path.extension().and_then(|e| e.to_str()) else {
                continue;
            };
            let ext_lower = ext.to_ascii_lowercase();
            if ext_lower != "ttf" && ext_lower != "otf" && ext_lower != "ttc" {
                continue;
            }
            let Ok(bytes) = std::fs::read(&path) else {
                tracing::warn!("failed to read font file: {}", path.display());
                continue;
            };
            let stem = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("unknown")
                .to_ascii_lowercase();
            match FontArc::try_from_vec(bytes) {
                Ok(font) => {
                    tracing::debug!(family = %stem, "loaded overlay font");
                    registry.add(stem, font);
                }
                Err(e) => {
                    tracing::warn!(family = %stem, error = %e, "skipping invalid font file");
                }
            }
        }
        registry
    }

    pub fn add(&mut self, family: String, font: FontArc) {
        let key = family.to_ascii_lowercase();
        if self.first_family.is_none() {
            self.first_family = Some(key.clone());
        }
        self.fonts.insert(key, font);
    }

    pub fn resolve(&self, family: &str) -> Option<&FontArc> {
        let key = family.to_ascii_lowercase();
        self.fonts.get(&key)
    }

    pub fn first(&self) -> Option<&FontArc> {
        self.first_family.as_ref().and_then(|f| self.fonts.get(f))
    }

    pub fn len(&self) -> usize {
        self.fonts.len()
    }

    pub fn is_empty(&self) -> bool {
        self.fonts.is_empty()
    }

    fn resolve_or_first(&self, family: &str) -> Result<&FontArc, OverlayPipelineError> {
        self.resolve(family)
            .or_else(|| self.first())
            .ok_or(OverlayPipelineError::NoFontAvailable)
    }
}

// ---------------------------------------------------------------------------
// Main entry point
// ---------------------------------------------------------------------------

pub fn composite(
    source: &RgbaImage,
    canvas: CanvasPreset,
    overlays: &[ResolvedOverlay],
    fonts: &FontRegistry,
) -> Result<RgbaImage, OverlayPipelineError> {
    let mut canvas_img = resize_to_canvas(source, canvas);

    let mut sorted: Vec<&ResolvedOverlay> = overlays.iter().collect();
    sorted.sort_by_key(|o| o.overlay_type.layer_order());

    for overlay in &sorted {
        match overlay.overlay_type {
            OverlayType::Backdrop => composite_backdrop(&mut canvas_img, overlay)?,
            OverlayType::Image => composite_image(&mut canvas_img, overlay)?,
            OverlayType::Text => composite_text(&mut canvas_img, overlay, fonts)?,
        }
    }

    Ok(canvas_img)
}

// ---------------------------------------------------------------------------
// Canvas scaling
// ---------------------------------------------------------------------------

pub fn resize_to_canvas(source: &RgbaImage, canvas: CanvasPreset) -> RgbaImage {
    let (target_w, target_h) = canvas.dimensions();
    let (src_w, src_h) = source.dimensions();
    if src_w == target_w && src_h == target_h {
        return source.clone();
    }
    if src_w > target_w || src_h > target_h {
        return resize(source, target_w, target_h, FilterType::Lanczos3);
    }
    let mut padded = ImageBuffer::from_pixel(target_w, target_h, Rgba([0, 0, 0, 0]));
    let x = (target_w - src_w) / 2;
    let y = (target_h - src_h) / 2;
    let _ = padded.copy_from(source, x, y);
    padded
}

// ---------------------------------------------------------------------------
// Image overlay
// ---------------------------------------------------------------------------

fn composite_image(
    canvas: &mut RgbaImage,
    overlay_def: &ResolvedOverlay,
) -> Result<(), OverlayPipelineError> {
    let bytes = overlay_def
        .image_bytes
        .as_ref()
        .ok_or_else(|| OverlayPipelineError::Decode("image overlay has no image bytes".into()))?;

    let img = if overlay_def.image_is_svg {
        let target_w = overlay_def.scale_width.unwrap_or(0);
        let target_h = overlay_def.scale_height.unwrap_or(0);
        render_svg(bytes, target_w, target_h)?
    } else {
        let decoded = image::load_from_memory(bytes)
            .map_err(|e| OverlayPipelineError::Decode(e.to_string()))?;
        let mut rgba = decoded.to_rgba8();
        if let (Some(sw), Some(sh)) = (overlay_def.scale_width, overlay_def.scale_height)
            && sw > 0
            && sh > 0
            && (sw != rgba.width() || sh != rgba.height())
        {
            rgba = resize(&rgba, sw, sh, FilterType::Lanczos3);
        }
        rgba
    };

    let (canvas_w, canvas_h) = canvas.dimensions();
    let (x, y) = compute_position(
        overlay_def.horizontal_align,
        overlay_def.horizontal_offset,
        overlay_def.vertical_align,
        overlay_def.vertical_offset,
        img.width(),
        img.height(),
        canvas_w,
        canvas_h,
    );
    overlay(canvas, &img, x.max(0) as i64, y.max(0) as i64);
    Ok(())
}

// ---------------------------------------------------------------------------
// Text overlay
// ---------------------------------------------------------------------------

fn composite_text(
    canvas: &mut RgbaImage,
    overlay_def: &ResolvedOverlay,
    fonts: &FontRegistry,
) -> Result<(), OverlayPipelineError> {
    let text = overlay_def
        .text
        .as_ref()
        .ok_or_else(|| OverlayPipelineError::Decode("text overlay has no text".into()))?;

    if text.is_empty() {
        return Ok(());
    }

    let font = fonts.resolve_or_first(&overlay_def.font_family)?;

    let text_buf = render_text_to_buffer(
        text,
        font,
        overlay_def.font_size,
        overlay_def.font_color,
        overlay_def.stroke_color,
        overlay_def.stroke_width,
    )?;

    let composite_target: RgbaImage = if let Some(back) = overlay_def.back_color {
        let bw = overlay_def
            .back_width
            .unwrap_or_else(|| text_buf.width() + 2 * overlay_def.back_padding);
        let bh = overlay_def
            .back_height
            .unwrap_or_else(|| text_buf.height() + 2 * overlay_def.back_padding);
        let mut backdrop =
            fill_rounded_rect_buffer(bw.max(1), bh.max(1), overlay_def.back_radius, back);
        let tx = (bw.saturating_sub(text_buf.width())) / 2;
        let ty = (bh.saturating_sub(text_buf.height())) / 2;
        overlay(&mut backdrop, &text_buf, tx as i64, ty as i64);
        backdrop
    } else {
        text_buf
    };

    let (canvas_w, canvas_h) = canvas.dimensions();
    let (x, y) = compute_position(
        overlay_def.horizontal_align,
        overlay_def.horizontal_offset,
        overlay_def.vertical_align,
        overlay_def.vertical_offset,
        composite_target.width(),
        composite_target.height(),
        canvas_w,
        canvas_h,
    );
    overlay(canvas, &composite_target, x.max(0) as i64, y.max(0) as i64);
    Ok(())
}

fn render_text_to_buffer(
    text: &str,
    font: &FontArc,
    size: f32,
    color: Rgba<u8>,
    stroke_color: Option<Rgba<u8>>,
    stroke_width: u32,
) -> Result<RgbaImage, OverlayPipelineError> {
    if size <= 0.0 {
        return Ok(ImageBuffer::from_pixel(1, 1, TRANSPARENT));
    }
    let scaled = font.as_scaled(size);

    let glyphs = layout_glyphs(font, size, text);

    if glyphs.is_empty() {
        return Ok(ImageBuffer::from_pixel(1, 1, TRANSPARENT));
    }

    let (min_x, min_y, max_x, max_y) = glyphs_bounds(&glyphs);

    let pad = if stroke_width > 0 {
        stroke_width as i32 + 2
    } else {
        2
    };
    let origin_x = min_x.floor() as i32 - pad;
    let origin_y = min_y.floor() as i32 - pad;
    let buf_w = ((max_x.ceil() as i32) - origin_x + pad).max(1) as u32;
    let buf_h = ((max_y.ceil() as i32) - origin_y + pad).max(1) as u32;

    let mut buf = ImageBuffer::from_pixel(buf_w, buf_h, TRANSPARENT);
    let _ = scaled;

    if let Some(sc) = stroke_color
        && stroke_width > 0
    {
        for outline_glyph in &glyphs {
            draw_glyph_stroke(
                &mut buf,
                outline_glyph,
                origin_x,
                origin_y,
                sc,
                stroke_width,
            );
        }
    }

    for outline_glyph in &glyphs {
        draw_glyph_fill(&mut buf, outline_glyph, origin_x, origin_y, color);
    }

    Ok(buf)
}

fn layout_glyphs(font: &FontArc, size: f32, text: &str) -> Vec<ab_glyph::OutlinedGlyph> {
    let scaled = font.as_scaled(size);
    let mut caret = Point {
        x: 0.0,
        y: scaled.ascent(),
    };
    let mut last_id: Option<GlyphId> = None;
    let mut result = Vec::new();

    for c in text.chars() {
        if c.is_control() {
            continue;
        }
        let glyph_id: GlyphId = scaled.glyph_id(c);
        if let Some(prev) = last_id {
            caret.x += scaled.kern(prev, glyph_id);
        }
        let glyph = glyph_id.with_scale_and_position(size, caret);
        caret.x += scaled.h_advance(glyph_id);
        last_id = Some(glyph_id);

        if let Some(outlined) = font.outline_glyph(glyph) {
            result.push(outlined);
        }
    }
    result
}

fn glyphs_bounds(glyphs: &[ab_glyph::OutlinedGlyph]) -> (f32, f32, f32, f32) {
    let mut min_x = f32::MAX;
    let mut min_y = f32::MAX;
    let mut max_x = f32::MIN;
    let mut max_y = f32::MIN;
    for g in glyphs {
        let b = g.px_bounds();
        min_x = min_x.min(b.min.x);
        min_y = min_y.min(b.min.y);
        max_x = max_x.max(b.max.x);
        max_y = max_y.max(b.max.y);
    }
    (min_x, min_y, max_x, max_y)
}

fn draw_glyph_fill(
    buf: &mut RgbaImage,
    outline_glyph: &ab_glyph::OutlinedGlyph,
    origin_x: i32,
    origin_y: i32,
    color: Rgba<u8>,
) {
    let bounds = outline_glyph.px_bounds();
    let offset_x = bounds.min.x.floor() as i32 - origin_x;
    let offset_y = bounds.min.y.floor() as i32 - origin_y;
    outline_glyph.draw(|x, y, coverage| {
        let px = offset_x + x as i32;
        let py = offset_y + y as i32;
        if px < 0 || py < 0 {
            return;
        }
        let (pu, pv) = (px as u32, py as u32);
        if pu >= buf.width() || pv >= buf.height() {
            return;
        }
        let alpha = (coverage * color.0[3] as f32).round() as u32;
        blend_pixel(
            buf,
            pu,
            pv,
            Rgba([color.0[0], color.0[1], color.0[2], alpha.min(255) as u8]),
        );
    });
}

fn draw_glyph_stroke(
    buf: &mut RgbaImage,
    outline_glyph: &ab_glyph::OutlinedGlyph,
    origin_x: i32,
    origin_y: i32,
    color: Rgba<u8>,
    width: u32,
) {
    let bounds = outline_glyph.px_bounds();
    let offset_x = bounds.min.x.floor() as i32 - origin_x;
    let offset_y = bounds.min.y.floor() as i32 - origin_y;
    let radius = width as i32;
    let mut temp = ImageBuffer::from_pixel(buf.width(), buf.height(), TRANSPARENT);
    outline_glyph.draw(|x, y, coverage| {
        let px = offset_x + x as i32;
        let py = offset_y + y as i32;
        if px < 0 || py < 0 {
            return;
        }
        let (pu, pv) = (px as u32, py as u32);
        if pu >= temp.width() || pv >= temp.height() {
            return;
        }
        let alpha = (coverage * 255.0).round() as u8;
        temp.put_pixel(pu, pv, Rgba([255, 255, 255, alpha]));
    });
    let stroke_buf = dilate_mask(&temp, radius);
    for y in 0..buf.height() {
        for x in 0..buf.width() {
            let mask_px = stroke_buf.get_pixel(x, y);
            if mask_px.0[3] == 0 {
                continue;
            }
            let existing = buf.get_pixel(x, y);
            if existing.0[3] > 0 {
                continue;
            }
            let alpha = mask_px.0[3];
            blend_pixel(buf, x, y, Rgba([color.0[0], color.0[1], color.0[2], alpha]));
        }
    }
}

fn dilate_mask(mask: &RgbaImage, radius: i32) -> RgbaImage {
    let (w, h) = mask.dimensions();
    let mut out = ImageBuffer::from_pixel(w, h, TRANSPARENT);
    for y in 0..h as i32 {
        for x in 0..w as i32 {
            let mut max_alpha = 0u8;
            for dy in -radius..=radius {
                for dx in -radius..=radius {
                    if dx * dx + dy * dy > radius * radius {
                        continue;
                    }
                    let nx = x + dx;
                    let ny = y + dy;
                    if nx < 0 || ny < 0 || nx >= w as i32 || ny >= h as i32 {
                        continue;
                    }
                    let a = mask.get_pixel(nx as u32, ny as u32).0[3];
                    if a > max_alpha {
                        max_alpha = a;
                    }
                }
            }
            if max_alpha > 0 {
                out.put_pixel(x as u32, y as u32, Rgba([255, 255, 255, max_alpha]));
            }
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Backdrop overlay
// ---------------------------------------------------------------------------

fn composite_backdrop(
    canvas: &mut RgbaImage,
    overlay_def: &ResolvedOverlay,
) -> Result<(), OverlayPipelineError> {
    let color = overlay_def
        .back_color
        .ok_or_else(|| OverlayPipelineError::Decode("backdrop overlay has no back_color".into()))?;
    let w = overlay_def.back_width.unwrap_or(100).max(1);
    let h = overlay_def.back_height.unwrap_or(100).max(1);

    let backdrop = fill_rounded_rect_buffer(w, h, overlay_def.back_radius, color);

    let (canvas_w, canvas_h) = canvas.dimensions();
    let (x, y) = compute_position(
        overlay_def.horizontal_align,
        overlay_def.horizontal_offset,
        overlay_def.vertical_align,
        overlay_def.vertical_offset,
        w,
        h,
        canvas_w,
        canvas_h,
    );
    overlay(canvas, &backdrop, x.max(0) as i64, y.max(0) as i64);
    Ok(())
}

fn fill_rounded_rect_buffer(w: u32, h: u32, radius: u32, color: Rgba<u8>) -> RgbaImage {
    let r = radius.min(w / 2).min(h / 2);
    let mut buf = ImageBuffer::from_pixel(w, h, color);
    if r == 0 {
        return buf;
    }
    let r_i = r as i32;
    let w_i = w as i32;
    let h_i = h as i32;
    for y in 0..h_i {
        for x in 0..w_i {
            if is_in_corner_region(x, y, w_i, h_i, r_i) {
                let cx = if x < r_i { r_i } else { w_i - 1 - r_i };
                let cy = if y < r_i { r_i } else { h_i - 1 - r_i };
                let dx = (x - cx) as f64;
                let dy = (y - cy) as f64;
                let dist_sq = dx * dx + dy * dy;
                let r_f = r_i as f64;
                if dist_sq > r_f * r_f {
                    buf.put_pixel(x as u32, y as u32, TRANSPARENT);
                }
            }
        }
    }
    buf
}

fn is_in_corner_region(x: i32, y: i32, w: i32, h: i32, r: i32) -> bool {
    (x < r || x > w - 1 - r) && (y < r || y > h - 1 - r)
}

// ---------------------------------------------------------------------------
// SVG rendering
// ---------------------------------------------------------------------------

fn render_svg(
    svg_bytes: &[u8],
    target_w: u32,
    target_h: u32,
) -> Result<RgbaImage, OverlayPipelineError> {
    let tree = resvg::usvg::Tree::from_data(svg_bytes, &Default::default())
        .map_err(|e| OverlayPipelineError::SvgParse(e.to_string()))?;

    let svg_size = tree.size();
    let svg_w = svg_size.width();
    let svg_h = svg_size.height();
    let (render_w, render_h) = if target_w > 0 && target_h > 0 {
        (target_w, target_h)
    } else if target_w > 0 {
        let scale = target_w as f32 / svg_w;
        (target_w, (svg_h * scale).round().max(1.0) as u32)
    } else if target_h > 0 {
        let scale = target_h as f32 / svg_h;
        ((svg_w * scale).round().max(1.0) as u32, target_h)
    } else {
        (svg_w.round().max(1.0) as u32, svg_h.round().max(1.0) as u32)
    };

    let mut pixmap = resvg::tiny_skia::Pixmap::new(render_w, render_h)
        .ok_or_else(|| OverlayPipelineError::SvgParse("failed to allocate pixmap".into()))?;

    let scale_x = render_w as f32 / svg_w;
    let scale_y = render_h as f32 / svg_h;
    let transform = resvg::tiny_skia::Transform::from_scale(scale_x, scale_y);
    resvg::render(&tree, transform, &mut pixmap.as_mut());

    Ok(pixmap_to_rgba(&pixmap))
}

fn pixmap_to_rgba(pixmap: &resvg::tiny_skia::Pixmap) -> RgbaImage {
    let w = pixmap.width();
    let h = pixmap.height();
    let data = pixmap.data();
    let mut buf = ImageBuffer::new(w, h);
    for y in 0..h {
        for x in 0..w {
            let idx = ((y * w + x) * 4) as usize;
            let (r, g, b, a) = if idx + 3 < data.len() {
                let pa = data[idx + 3];
                if pa == 0 {
                    (0u8, 0u8, 0u8, 0u8)
                } else {
                    let pf = pa as u32;
                    let r = (data[idx] as u32 * 255 / pf).min(255) as u8;
                    let g = (data[idx + 1] as u32 * 255 / pf).min(255) as u8;
                    let b = (data[idx + 2] as u32 * 255 / pf).min(255) as u8;
                    (r, g, b, pa)
                }
            } else {
                (0, 0, 0, 0)
            };
            buf.put_pixel(x, y, Rgba([r, g, b, a]));
        }
    }
    buf
}

// ---------------------------------------------------------------------------
// Positioning
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
fn compute_position(
    h_align: HorizontalAlignment,
    h_offset: i32,
    v_align: VerticalAlignment,
    v_offset: i32,
    overlay_w: u32,
    overlay_h: u32,
    canvas_w: u32,
    canvas_h: u32,
) -> (i32, i32) {
    let x = match h_align {
        HorizontalAlignment::Left => h_offset,
        HorizontalAlignment::Center => canvas_w as i32 / 2 - overlay_w as i32 / 2 + h_offset,
        HorizontalAlignment::Right => canvas_w as i32 - overlay_w as i32 - h_offset,
    };
    let y = match v_align {
        VerticalAlignment::Top => v_offset,
        VerticalAlignment::Center => canvas_h as i32 / 2 - overlay_h as i32 / 2 + v_offset,
        VerticalAlignment::Bottom => canvas_h as i32 - overlay_h as i32 - v_offset,
    };
    (x, y)
}

// ---------------------------------------------------------------------------
// Pixel blending
// ---------------------------------------------------------------------------

const TRANSPARENT: Rgba<u8> = Rgba([0, 0, 0, 0]);

fn blend_pixel(buf: &mut RgbaImage, x: u32, y: u32, src: Rgba<u8>) {
    if src.0[3] == 0 {
        return;
    }
    if src.0[3] == 255 {
        buf.put_pixel(x, y, src);
        return;
    }
    let dst = buf.get_pixel(x, y);
    let src_a = src.0[3] as u32;
    let dst_a = dst.0[3] as u32;
    let out_a = src_a + dst_a * (255 - src_a) / 255;
    if out_a == 0 {
        return;
    }
    let blend = |s: u8, d: u8| -> u8 {
        ((s as u32 * src_a + d as u32 * dst_a * (255 - src_a) / 255) / out_a).min(255) as u8
    };
    buf.put_pixel(
        x,
        y,
        Rgba([
            blend(src.0[0], dst.0[0]),
            blend(src.0[1], dst.0[1]),
            blend(src.0[2], dst.0[2]),
            out_a as u8,
        ]),
    );
}

// ---------------------------------------------------------------------------
// Color parsing
// ---------------------------------------------------------------------------

pub fn parse_hex_color(s: &str) -> Result<Rgba<u8>, OverlayPipelineError> {
    let hex = s.strip_prefix('#').unwrap_or(s);
    let (r, g, b, a) = match hex.len() {
        3 => {
            let bytes = hex.as_bytes();
            let expand = |c: u8| {
                let d = char::from(c).to_digit(16).unwrap_or(0) as u8;
                d * 17
            };
            (expand(bytes[0]), expand(bytes[1]), expand(bytes[2]), 255)
        }
        4 => {
            let bytes = hex.as_bytes();
            let expand = |c: u8| {
                let d = char::from(c).to_digit(16).unwrap_or(0) as u8;
                d * 17
            };
            (
                expand(bytes[0]),
                expand(bytes[1]),
                expand(bytes[2]),
                expand(bytes[3]),
            )
        }
        6 => {
            let r = u32::from_str_radix(&hex[0..2], 16)
                .map_err(|_| OverlayPipelineError::InvalidColor(s.into()))?;
            let g = u32::from_str_radix(&hex[2..4], 16)
                .map_err(|_| OverlayPipelineError::InvalidColor(s.into()))?;
            let b = u32::from_str_radix(&hex[4..6], 16)
                .map_err(|_| OverlayPipelineError::InvalidColor(s.into()))?;
            (r as u8, g as u8, b as u8, 255)
        }
        8 => {
            let r = u32::from_str_radix(&hex[0..2], 16)
                .map_err(|_| OverlayPipelineError::InvalidColor(s.into()))?;
            let g = u32::from_str_radix(&hex[2..4], 16)
                .map_err(|_| OverlayPipelineError::InvalidColor(s.into()))?;
            let b = u32::from_str_radix(&hex[4..6], 16)
                .map_err(|_| OverlayPipelineError::InvalidColor(s.into()))?;
            let a = u32::from_str_radix(&hex[6..8], 16)
                .map_err(|_| OverlayPipelineError::InvalidColor(s.into()))?;
            (r as u8, g as u8, b as u8, a as u8)
        }
        _ => return Err(OverlayPipelineError::InvalidColor(s.into())),
    };
    Ok(Rgba([r, g, b, a]))
}

// ---------------------------------------------------------------------------
// Resolution helpers (pure functions for the domain layer)
// ---------------------------------------------------------------------------

pub fn resolve_groups(overlays: Vec<ResolvedOverlay>) -> Vec<ResolvedOverlay> {
    let mut by_group: HashMap<String, Vec<ResolvedOverlay>> = HashMap::new();
    let mut standalone = Vec::new();

    for o in overlays {
        match &o.group_name {
            Some(g) if !g.is_empty() => {
                by_group.entry(g.clone()).or_default().push(o);
            }
            _ => standalone.push(o),
        }
    }

    let mut result = standalone;
    for (_, mut group) in by_group {
        group.sort_by_key(|o| std::cmp::Reverse(o.weight));
        if let Some(winner) = group.into_iter().next() {
            result.push(winner);
        }
    }
    result
}

pub fn apply_suppress_rules(overlays: Vec<ResolvedOverlay>) -> Vec<ResolvedOverlay> {
    let suppressed: Vec<String> = overlays
        .iter()
        .flat_map(|o| o.suppresses.iter().cloned())
        .collect();
    if suppressed.is_empty() {
        return overlays;
    }
    overlays
        .into_iter()
        .filter(|o| !suppressed.iter().any(|s| s == &o.slug))
        .collect()
}

pub fn resolve_queue_positions(overlays: &mut [ResolvedOverlay], spacing: u32) {
    let mut queues: HashMap<String, Vec<usize>> = HashMap::new();
    for (i, o) in overlays.iter().enumerate() {
        if let Some(qn) = &o.queue_name
            && !qn.is_empty()
        {
            queues.entry(qn.clone()).or_default().push(i);
        }
    }

    for (_, mut indices) in queues {
        indices.sort_by_key(|&i| std::cmp::Reverse(overlays[i].weight));
        let mut accumulated_v = 0i32;
        let mut accumulated_h = 0i32;
        for (pos, &idx) in indices.iter().enumerate() {
            if pos == 0 {
                continue;
            }
            let prev = indices[pos - 1];
            let prev_h = estimate_overlay_height(&overlays[prev]);
            let prev_w = estimate_overlay_width(&overlays[prev]);
            accumulated_v += prev_h as i32 + spacing as i32;
            accumulated_h += prev_w as i32 + spacing as i32;
            overlays[idx].vertical_offset += accumulated_v;
            overlays[idx].horizontal_offset += accumulated_h;
        }
    }
}

fn estimate_overlay_height(o: &ResolvedOverlay) -> u32 {
    match o.overlay_type {
        OverlayType::Backdrop => o.back_height.unwrap_or(50),
        OverlayType::Image => o.scale_height.unwrap_or(50),
        OverlayType::Text => (o.font_size * 1.3) as u32,
    }
}

fn estimate_overlay_width(o: &ResolvedOverlay) -> u32 {
    match o.overlay_type {
        OverlayType::Backdrop => o.back_width.unwrap_or(100),
        OverlayType::Image => o.scale_width.unwrap_or(100),
        OverlayType::Text => {
            let chars = o.text.as_ref().map(|t| t.len()).unwrap_or(0) as u32;
            (o.font_size as u32 * chars) / 2
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_blank(w: u32, h: u32, color: Rgba<u8>) -> RgbaImage {
        ImageBuffer::from_pixel(w, h, color)
    }

    fn read_pixel(img: &RgbaImage, x: u32, y: u32) -> Rgba<u8> {
        *img.get_pixel(x.min(img.width() - 1), y.min(img.height() - 1))
    }

    fn has_nontransparent_pixel(img: &RgbaImage) -> bool {
        img.pixels().any(|p| p.0[3] > 0)
    }

    fn count_opaque_pixels(img: &RgbaImage) -> usize {
        img.pixels().filter(|p| p.0[3] > 0).count()
    }

    // ---- CanvasPreset ----

    #[test]
    fn poster_canvas_dimensions() {
        assert_eq!(CanvasPreset::Poster.dimensions(), (1000, 1500));
    }

    #[test]
    fn backdrop_canvas_dimensions() {
        assert_eq!(CanvasPreset::Backdrop.dimensions(), (1920, 1080));
    }

    #[test]
    fn canvas_from_artwork_type() {
        assert_eq!(
            CanvasPreset::from_artwork_type("poster"),
            Some(CanvasPreset::Poster)
        );
        assert_eq!(
            CanvasPreset::from_artwork_type("season_poster"),
            Some(CanvasPreset::Poster)
        );
        assert_eq!(
            CanvasPreset::from_artwork_type("backdrop"),
            Some(CanvasPreset::Backdrop)
        );
        assert_eq!(
            CanvasPreset::from_artwork_type("episode_thumb"),
            Some(CanvasPreset::Backdrop)
        );
        assert_eq!(CanvasPreset::from_artwork_type("unknown"), None);
    }

    // ---- OverlayType ----

    #[test]
    fn overlay_type_from_db_str() {
        assert_eq!(OverlayType::from_db_str("image"), Some(OverlayType::Image));
        assert_eq!(OverlayType::from_db_str("text"), Some(OverlayType::Text));
        assert_eq!(
            OverlayType::from_db_str("backdrop"),
            Some(OverlayType::Backdrop)
        );
        assert_eq!(OverlayType::from_db_str("foo"), None);
    }

    #[test]
    fn backdrop_has_lowest_layer_order() {
        assert!(OverlayType::Backdrop.layer_order() < OverlayType::Image.layer_order());
        assert!(OverlayType::Image.layer_order() < OverlayType::Text.layer_order());
    }

    // ---- parse_hex_color ----

    #[test]
    fn parse_hex_3_digit() {
        assert_eq!(parse_hex_color("#F00").unwrap(), Rgba([255, 0, 0, 255]));
        assert_eq!(parse_hex_color("#0F0").unwrap(), Rgba([0, 255, 0, 255]));
    }

    #[test]
    fn parse_hex_4_digit_with_alpha() {
        assert_eq!(parse_hex_color("#F00F").unwrap(), Rgba([255, 0, 0, 255]));
        assert_eq!(parse_hex_color("#F008").unwrap(), Rgba([255, 0, 0, 136]));
    }

    #[test]
    fn parse_hex_6_digit() {
        assert_eq!(
            parse_hex_color("#FF8800").unwrap(),
            Rgba([255, 136, 0, 255])
        );
        assert_eq!(parse_hex_color("#000000").unwrap(), Rgba([0, 0, 0, 255]));
    }

    #[test]
    fn parse_hex_8_digit_with_alpha() {
        assert_eq!(parse_hex_color("#00000099").unwrap(), Rgba([0, 0, 0, 153]));
        assert_eq!(
            parse_hex_color("#FFFFFF80").unwrap(),
            Rgba([255, 255, 255, 128])
        );
    }

    #[test]
    fn parse_hex_without_hash_prefix() {
        assert_eq!(parse_hex_color("FF0000").unwrap(), Rgba([255, 0, 0, 255]));
    }

    #[test]
    fn parse_hex_invalid_length_rejected() {
        assert!(parse_hex_color("#FF").is_err());
        assert!(parse_hex_color("#FFFFF").is_err());
        assert!(parse_hex_color("#GGGGGG").is_err());
    }

    // ---- compute_position ----

    #[test]
    fn position_top_left() {
        let (x, y) = compute_position(
            HorizontalAlignment::Left,
            0,
            VerticalAlignment::Top,
            0,
            100,
            50,
            1000,
            1500,
        );
        assert_eq!((x, y), (0, 0));
    }

    #[test]
    fn position_top_right_with_offset() {
        let (x, y) = compute_position(
            HorizontalAlignment::Right,
            25,
            VerticalAlignment::Top,
            0,
            100,
            50,
            1000,
            1500,
        );
        assert_eq!((x, y), (875, 0));
    }

    #[test]
    fn position_center() {
        let (x, y) = compute_position(
            HorizontalAlignment::Center,
            0,
            VerticalAlignment::Center,
            0,
            100,
            50,
            1000,
            1500,
        );
        assert_eq!(x, 450);
        assert_eq!(y, 725);
    }

    #[test]
    fn position_bottom_right() {
        let (x, y) = compute_position(
            HorizontalAlignment::Right,
            0,
            VerticalAlignment::Bottom,
            0,
            100,
            50,
            1000,
            1500,
        );
        assert_eq!(x, 900);
        assert_eq!(y, 1450);
    }

    // ---- blend_pixel ----

    #[test]
    fn blend_fully_opaque_replaces() {
        let mut img = make_blank(2, 2, Rgba([0, 0, 0, 0]));
        blend_pixel(&mut img, 0, 0, Rgba([255, 0, 0, 255]));
        assert_eq!(read_pixel(&img, 0, 0), Rgba([255, 0, 0, 255]));
    }

    #[test]
    fn blend_transparent_is_noop() {
        let mut img = make_blank(2, 2, Rgba([10, 20, 30, 100]));
        blend_pixel(&mut img, 0, 0, Rgba([255, 255, 255, 0]));
        assert_eq!(read_pixel(&img, 0, 0), Rgba([10, 20, 30, 100]));
    }

    #[test]
    fn blend_semi_transparent_combines() {
        let mut img = make_blank(2, 2, Rgba([0, 0, 0, 255]));
        blend_pixel(&mut img, 0, 0, Rgba([255, 255, 255, 128]));
        let px = read_pixel(&img, 0, 0);
        assert_eq!(px.0[3], 255);
        assert!(px.0[0] > 100);
    }

    // ---- fill_rounded_rect_buffer ----

    #[test]
    fn rounded_rect_corners_transparent() {
        let buf = fill_rounded_rect_buffer(100, 100, 20, Rgba([255, 0, 0, 255]));
        let corner = read_pixel(&buf, 0, 0);
        assert_eq!(corner.0[3], 0);
        let center = read_pixel(&buf, 50, 50);
        assert_eq!(center, Rgba([255, 0, 0, 255]));
    }

    #[test]
    fn square_rect_no_rounding() {
        let buf = fill_rounded_rect_buffer(10, 10, 0, Rgba([0, 255, 0, 255]));
        assert_eq!(read_pixel(&buf, 0, 0), Rgba([0, 255, 0, 255]));
        assert_eq!(read_pixel(&buf, 9, 9), Rgba([0, 255, 0, 255]));
    }

    #[test]
    fn rounded_rect_full_pixel_count_less_than_square() {
        let square = count_opaque_pixels(&fill_rounded_rect_buffer(
            100,
            100,
            0,
            Rgba([255, 0, 0, 255]),
        ));
        let rounded = count_opaque_pixels(&fill_rounded_rect_buffer(
            100,
            100,
            20,
            Rgba([255, 0, 0, 255]),
        ));
        assert!(rounded < square);
        assert!(rounded > 0);
    }

    // ---- is_in_corner_region ----

    #[test]
    fn corner_region_detection() {
        assert!(is_in_corner_region(0, 0, 100, 100, 10));
        assert!(is_in_corner_region(99, 99, 100, 100, 10));
        assert!(!is_in_corner_region(50, 0, 100, 100, 10));
        assert!(!is_in_corner_region(0, 50, 100, 100, 10));
        assert!(!is_in_corner_region(50, 50, 100, 100, 10));
    }

    // ---- resolve_groups ----

    fn make_overlay(slug: &str, group: Option<&str>, weight: i32) -> ResolvedOverlay {
        ResolvedOverlay {
            id: Uuid::nil(),
            slug: slug.into(),
            overlay_type: OverlayType::Image,
            horizontal_align: HorizontalAlignment::Left,
            horizontal_offset: 0,
            vertical_align: VerticalAlignment::Top,
            vertical_offset: 0,
            group_name: group.map(String::from),
            weight,
            queue_name: None,
            suppresses: vec![],
            image_bytes: None,
            image_is_svg: false,
            scale_width: None,
            scale_height: None,
            text: None,
            font_family: String::new(),
            font_size: 0.0,
            font_color: Rgba([0, 0, 0, 0]),
            stroke_color: None,
            stroke_width: 0,
            back_color: None,
            back_width: None,
            back_height: None,
            back_radius: 0,
            back_padding: 0,
        }
    }

    #[test]
    fn resolve_groups_picks_highest_weight() {
        let overlays = vec![
            make_overlay("4k", Some("resolution"), 30),
            make_overlay("4k_hdr", Some("resolution"), 40),
            make_overlay("1080p", Some("resolution"), 10),
            make_overlay("standalone", None, 0),
        ];
        let result = resolve_groups(overlays);
        let slugs: Vec<_> = result.iter().map(|o| o.slug.as_str()).collect();
        assert!(slugs.contains(&"4k_hdr"));
        assert!(slugs.contains(&"standalone"));
        assert!(!slugs.contains(&"4k"));
        assert!(!slugs.contains(&"1080p"));
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn resolve_groups_empty_group_name_is_standalone() {
        let overlays = vec![make_overlay("a", Some(""), 10), make_overlay("b", None, 5)];
        let result = resolve_groups(overlays);
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn resolve_groups_single_overlay_in_group() {
        let overlays = vec![make_overlay("solo", Some("lone"), 5)];
        let result = resolve_groups(overlays);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].slug, "solo");
    }

    // ---- apply_suppress_rules ----

    #[test]
    fn suppress_removes_listed_slugs() {
        let overlays = vec![
            {
                let mut o = make_overlay("4k_hdr", None, 0);
                o.suppresses = vec!["4k".into(), "hdr".into()];
                o
            },
            make_overlay("4k", None, 0),
            make_overlay("hdr", None, 0),
            make_overlay("rating", None, 0),
        ];
        let result = apply_suppress_rules(overlays);
        let slugs: Vec<_> = result.iter().map(|o| o.slug.as_str()).collect();
        assert!(slugs.contains(&"4k_hdr"));
        assert!(slugs.contains(&"rating"));
        assert!(!slugs.contains(&"4k"));
        assert!(!slugs.contains(&"hdr"));
    }

    #[test]
    fn suppress_empty_list_returns_all() {
        let overlays = vec![make_overlay("a", None, 0), make_overlay("b", None, 0)];
        let result = apply_suppress_rules(overlays);
        assert_eq!(result.len(), 2);
    }

    // ---- resolve_queue_positions ----

    #[test]
    fn queue_offsets_stack_vertically() {
        let mut overlays = vec![
            {
                let mut o = make_overlay("top", None, 40);
                o.queue_name = Some("br".into());
                o.overlay_type = OverlayType::Text;
                o.font_size = 20.0;
                o
            },
            {
                let mut o = make_overlay("mid", None, 30);
                o.queue_name = Some("br".into());
                o.overlay_type = OverlayType::Text;
                o.font_size = 20.0;
                o
            },
            {
                let mut o = make_overlay("bot", None, 20);
                o.queue_name = Some("br".into());
                o.overlay_type = OverlayType::Text;
                o.font_size = 20.0;
                o
            },
        ];
        resolve_queue_positions(&mut overlays, 8);
        assert_eq!(overlays[0].vertical_offset, 0);
        assert!(overlays[1].vertical_offset > 0);
        assert!(overlays[2].vertical_offset > overlays[1].vertical_offset);
    }

    #[test]
    fn queue_does_not_affect_non_queued() {
        let mut overlays = vec![make_overlay("loner", None, 0), {
            let mut o = make_overlay("queued", None, 10);
            o.queue_name = Some("q1".into());
            o
        }];
        resolve_queue_positions(&mut overlays, 8);
        assert_eq!(overlays[0].vertical_offset, 0);
    }

    // ---- FontRegistry ----

    #[test]
    fn font_registry_empty_has_no_fonts() {
        let reg = FontRegistry::empty();
        assert!(reg.is_empty());
        assert!(reg.resolve("Inter").is_none());
    }

    #[test]
    fn font_registry_resolve_case_insensitive() {
        let font_bytes = include_test_font();
        let font = FontArc::try_from_vec(font_bytes).unwrap();
        let mut reg = FontRegistry::empty();
        reg.add("Inter".into(), font);
        assert!(reg.resolve("inter").is_some());
        assert!(reg.resolve("INTER").is_some());
        assert!(reg.resolve("Inter").is_some());
        assert!(reg.resolve("Arial").is_none());
    }

    #[test]
    fn font_registry_first_returns_added_font() {
        let font_bytes = include_test_font();
        let font = FontArc::try_from_vec(font_bytes).unwrap();
        let mut reg = FontRegistry::empty();
        reg.add("MyFont".into(), font);
        assert!(reg.first().is_some());
        assert!(reg.resolve("nonexistent").is_none());
    }

    #[test]
    fn font_registry_resolve_or_first_falls_back() {
        let font_bytes = include_test_font();
        let font = FontArc::try_from_vec(font_bytes).unwrap();
        let mut reg = FontRegistry::empty();
        reg.add("DejaVu".into(), font);
        let resolved = reg.resolve_or_first("Nonexistent");
        assert!(resolved.is_ok());
    }

    #[test]
    fn font_registry_resolve_or_first_no_fonts_errors() {
        let reg = FontRegistry::empty();
        assert!(matches!(
            reg.resolve_or_first("anything"),
            Err(OverlayPipelineError::NoFontAvailable)
        ));
    }

    // ---- resize_to_canvas ----

    #[test]
    fn resize_downscales_to_poster() {
        let src = make_blank(2000, 3000, Rgba([100, 100, 100, 255]));
        let result = resize_to_canvas(&src, CanvasPreset::Poster);
        assert_eq!(result.dimensions(), (1000, 1500));
    }

    #[test]
    fn resize_pads_smaller_image() {
        let src = make_blank(500, 750, Rgba([100, 100, 100, 255]));
        let result = resize_to_canvas(&src, CanvasPreset::Poster);
        assert_eq!(result.dimensions(), (1000, 1500));
        let center = read_pixel(&result, 500, 750);
        assert_eq!(center.0[3], 255);
    }

    #[test]
    fn resize_noop_when_exact_match() {
        let (w, h) = CanvasPreset::Poster.dimensions();
        let src = make_blank(w, h, Rgba([50, 50, 50, 255]));
        let result = resize_to_canvas(&src, CanvasPreset::Poster);
        assert_eq!(result.dimensions(), (w, h));
        assert_eq!(read_pixel(&result, 0, 0), Rgba([50, 50, 50, 255]));
    }

    // ---- composite (integration) ----

    #[test]
    fn composite_with_no_overlays_returns_canvas() {
        let src = make_blank(1000, 1500, Rgba([100, 150, 200, 255]));
        let fonts = FontRegistry::empty();
        let result = composite(&src, CanvasPreset::Poster, &[], &fonts).unwrap();
        assert_eq!(result.dimensions(), (1000, 1500));
        assert_eq!(read_pixel(&result, 500, 750), Rgba([100, 150, 200, 255]));
    }

    #[test]
    fn composite_backdrop_overlay_fills_region() {
        let src = make_blank(1000, 1500, Rgba([0, 0, 0, 0]));
        let overlay_def = ResolvedOverlay {
            id: Uuid::nil(),
            slug: "test_backdrop".into(),
            overlay_type: OverlayType::Backdrop,
            horizontal_align: HorizontalAlignment::Left,
            horizontal_offset: 0,
            vertical_align: VerticalAlignment::Top,
            vertical_offset: 0,
            group_name: None,
            weight: 0,
            queue_name: None,
            suppresses: vec![],
            image_bytes: None,
            image_is_svg: false,
            scale_width: None,
            scale_height: None,
            text: None,
            font_family: String::new(),
            font_size: 0.0,
            font_color: Rgba([0, 0, 0, 0]),
            stroke_color: None,
            stroke_width: 0,
            back_color: Some(Rgba([255, 0, 0, 255])),
            back_width: Some(200),
            back_height: Some(100),
            back_radius: 0,
            back_padding: 0,
        };
        let fonts = FontRegistry::empty();
        let result = composite(&src, CanvasPreset::Poster, &[overlay_def], &fonts).unwrap();
        assert_eq!(read_pixel(&result, 50, 50), Rgba([255, 0, 0, 255]));
        assert_eq!(read_pixel(&result, 250, 150), Rgba([0, 0, 0, 0]));
    }

    #[test]
    fn composite_image_overlay_blends_png() {
        let src = make_blank(100, 100, Rgba([0, 0, 0, 0]));
        let overlay_img = make_blank(10, 10, Rgba([0, 255, 0, 255]));
        let mut png_bytes = Vec::new();
        let dyn_img = image::DynamicImage::ImageRgba8(overlay_img);
        let mut cursor = std::io::Cursor::new(&mut png_bytes);
        dyn_img
            .write_to(&mut cursor, image::ImageFormat::Png)
            .unwrap();

        let overlay_def = ResolvedOverlay {
            id: Uuid::nil(),
            slug: "badge".into(),
            overlay_type: OverlayType::Image,
            horizontal_align: HorizontalAlignment::Left,
            horizontal_offset: 5,
            vertical_align: VerticalAlignment::Top,
            vertical_offset: 5,
            group_name: None,
            weight: 0,
            queue_name: None,
            suppresses: vec![],
            image_bytes: Some(png_bytes),
            image_is_svg: false,
            scale_width: None,
            scale_height: None,
            text: None,
            font_family: String::new(),
            font_size: 0.0,
            font_color: Rgba([0, 0, 0, 0]),
            stroke_color: None,
            stroke_width: 0,
            back_color: None,
            back_width: None,
            back_height: None,
            back_radius: 0,
            back_padding: 0,
        };
        let fonts = FontRegistry::empty();
        let result = composite(&src, CanvasPreset::Poster, &[overlay_def], &fonts).unwrap();
        assert_eq!(read_pixel(&result, 10, 10), Rgba([0, 255, 0, 255]));
    }

    #[test]
    fn composite_text_overlay_without_font_errors() {
        let src = make_blank(100, 100, Rgba([0, 0, 0, 0]));
        let overlay_def = ResolvedOverlay {
            id: Uuid::nil(),
            slug: "rating".into(),
            overlay_type: OverlayType::Text,
            horizontal_align: HorizontalAlignment::Left,
            horizontal_offset: 0,
            vertical_align: VerticalAlignment::Top,
            vertical_offset: 0,
            group_name: None,
            weight: 0,
            queue_name: None,
            suppresses: vec![],
            image_bytes: None,
            image_is_svg: false,
            scale_width: None,
            scale_height: None,
            text: Some("8.5".into()),
            font_family: "Inter".into(),
            font_size: 40.0,
            font_color: Rgba([255, 255, 255, 255]),
            stroke_color: None,
            stroke_width: 0,
            back_color: None,
            back_width: None,
            back_height: None,
            back_radius: 0,
            back_padding: 0,
        };
        let fonts = FontRegistry::empty();
        let result = composite(&src, CanvasPreset::Poster, &[overlay_def], &fonts);
        assert!(matches!(result, Err(OverlayPipelineError::NoFontAvailable)));
    }

    #[test]
    fn composite_text_overlay_renders_pixels() {
        let src = make_blank(200, 200, Rgba([0, 0, 0, 0]));
        let font_bytes = include_test_font();
        let font = FontArc::try_from_vec(font_bytes).unwrap();
        let mut fonts = FontRegistry::empty();
        fonts.add("TestFont".into(), font);

        let overlay_def = ResolvedOverlay {
            id: Uuid::nil(),
            slug: "rating".into(),
            overlay_type: OverlayType::Text,
            horizontal_align: HorizontalAlignment::Left,
            horizontal_offset: 10,
            vertical_align: VerticalAlignment::Top,
            vertical_offset: 10,
            group_name: None,
            weight: 0,
            queue_name: None,
            suppresses: vec![],
            image_bytes: None,
            image_is_svg: false,
            scale_width: None,
            scale_height: None,
            text: Some("Hello".into()),
            font_family: "TestFont".into(),
            font_size: 30.0,
            font_color: Rgba([255, 255, 255, 255]),
            stroke_color: None,
            stroke_width: 0,
            back_color: None,
            back_width: None,
            back_height: None,
            back_radius: 0,
            back_padding: 0,
        };
        let result = composite(&src, CanvasPreset::Backdrop, &[overlay_def], &fonts).unwrap();
        assert!(has_nontransparent_pixel(&result));
    }

    #[test]
    fn composite_text_overlay_with_backdrop() {
        let src = make_blank(200, 200, Rgba([0, 0, 0, 0]));
        let font_bytes = include_test_font();
        let font = FontArc::try_from_vec(font_bytes).unwrap();
        let mut fonts = FontRegistry::empty();
        fonts.add("TestFont".into(), font);

        let overlay_def = ResolvedOverlay {
            id: Uuid::nil(),
            slug: "rating".into(),
            overlay_type: OverlayType::Text,
            horizontal_align: HorizontalAlignment::Left,
            horizontal_offset: 0,
            vertical_align: VerticalAlignment::Top,
            vertical_offset: 0,
            group_name: None,
            weight: 0,
            queue_name: None,
            suppresses: vec![],
            image_bytes: None,
            image_is_svg: false,
            scale_width: None,
            scale_height: None,
            text: Some("8.5".into()),
            font_family: "TestFont".into(),
            font_size: 24.0,
            font_color: Rgba([255, 255, 255, 255]),
            stroke_color: None,
            stroke_width: 0,
            back_color: Some(Rgba([0, 0, 0, 200])),
            back_width: None,
            back_height: None,
            back_radius: 5,
            back_padding: 6,
        };
        let result = composite(&src, CanvasPreset::Backdrop, &[overlay_def], &fonts).unwrap();
        let black_count = result
            .pixels()
            .filter(|p| p.0[0] < 50 && p.0[1] < 50 && p.0[2] < 50 && p.0[3] > 100)
            .count();
        assert!(black_count > 0, "backdrop should produce dark pixels");
    }

    #[test]
    fn composite_layer_order_backdrop_before_text() {
        let src = make_blank(100, 100, Rgba([0, 0, 0, 0]));
        let backdrop_def = ResolvedOverlay {
            id: Uuid::nil(),
            slug: "bg".into(),
            overlay_type: OverlayType::Backdrop,
            horizontal_align: HorizontalAlignment::Left,
            horizontal_offset: 0,
            vertical_align: VerticalAlignment::Top,
            vertical_offset: 0,
            group_name: None,
            weight: 0,
            queue_name: None,
            suppresses: vec![],
            image_bytes: None,
            image_is_svg: false,
            scale_width: None,
            scale_height: None,
            text: None,
            font_family: String::new(),
            font_size: 0.0,
            font_color: Rgba([0, 0, 0, 0]),
            stroke_color: None,
            stroke_width: 0,
            back_color: Some(Rgba([255, 0, 0, 255])),
            back_width: Some(50),
            back_height: Some(50),
            back_radius: 0,
            back_padding: 0,
        };
        let image_overlay_img = make_blank(50, 50, Rgba([0, 0, 255, 255]));
        let mut png_bytes = Vec::new();
        let mut cursor = std::io::Cursor::new(&mut png_bytes);
        image::DynamicImage::ImageRgba8(image_overlay_img)
            .write_to(&mut cursor, image::ImageFormat::Png)
            .unwrap();
        let image_def = ResolvedOverlay {
            id: Uuid::nil(),
            slug: "badge".into(),
            overlay_type: OverlayType::Image,
            horizontal_align: HorizontalAlignment::Left,
            horizontal_offset: 0,
            vertical_align: VerticalAlignment::Top,
            vertical_offset: 0,
            group_name: None,
            weight: 0,
            queue_name: None,
            suppresses: vec![],
            image_bytes: Some(png_bytes),
            image_is_svg: false,
            scale_width: None,
            scale_height: None,
            text: None,
            font_family: String::new(),
            font_size: 0.0,
            font_color: Rgba([0, 0, 0, 0]),
            stroke_color: None,
            stroke_width: 0,
            back_color: None,
            back_width: None,
            back_height: None,
            back_radius: 0,
            back_padding: 0,
        };
        let fonts = FontRegistry::empty();
        let result = composite(
            &src,
            CanvasPreset::Backdrop,
            &[image_def, backdrop_def],
            &fonts,
        )
        .unwrap();
        assert_eq!(read_pixel(&result, 10, 10), Rgba([0, 0, 255, 255]));
    }

    // ---- pixmap_to_rgba ----

    #[test]
    fn pixmap_conversion_preserves_opaque_pixels() {
        let mut pixmap = resvg::tiny_skia::Pixmap::new(10, 10).unwrap();
        pixmap.fill(resvg::tiny_skia::Color::from_rgba8(255, 128, 64, 255));
        let rgba = pixmap_to_rgba(&pixmap);
        let px = read_pixel(&rgba, 5, 5);
        assert_eq!(px, Rgba([255, 128, 64, 255]));
    }

    #[test]
    fn pixmap_conversion_handles_transparent() {
        let mut pixmap = resvg::tiny_skia::Pixmap::new(5, 5).unwrap();
        pixmap.fill(resvg::tiny_skia::Color::from_rgba8(0, 0, 0, 0));
        let rgba = pixmap_to_rgba(&pixmap);
        assert_eq!(read_pixel(&rgba, 0, 0), Rgba([0, 0, 0, 0]));
    }

    #[test]
    fn pixmap_conversion_unpremultiplies() {
        let mut pixmap = resvg::tiny_skia::Pixmap::new(2, 2).unwrap();
        pixmap.fill(resvg::tiny_skia::Color::from_rgba8(0, 0, 0, 0));
        let data = pixmap.data_mut();
        let idx = 0;
        let premul_a = 128u8;
        data[idx] = 128;
        data[idx + 1] = 64;
        data[idx + 2] = 32;
        data[idx + 3] = premul_a;
        let rgba = pixmap_to_rgba(&pixmap);
        let px = read_pixel(&rgba, 0, 0);
        assert_eq!(px.0[3], 128);
        assert!(px.0[0] >= 250);
    }

    // ---- render_svg ----

    #[test]
    fn render_svg_basic() {
        let svg = r##"<?xml version="1.0" encoding="UTF-8"?>
<svg xmlns="http://www.w3.org/2000/svg" width="100" height="100">
  <rect width="100" height="100" fill="#FF0000"/>
</svg>"##;
        let result = render_svg(svg.as_bytes(), 50, 50).unwrap();
        assert_eq!(result.dimensions(), (50, 50));
        assert_eq!(read_pixel(&result, 25, 25), Rgba([255, 0, 0, 255]));
    }

    #[test]
    fn render_svg_invalid_returns_error() {
        let result = render_svg(b"not valid svg", 50, 50);
        assert!(matches!(result, Err(OverlayPipelineError::SvgParse(_))));
    }

    #[test]
    fn render_svg_auto_dimensions() {
        let svg = r##"<?xml version="1.0" encoding="UTF-8"?>
<svg xmlns="http://www.w3.org/2000/svg" width="200" height="100">
  <rect width="200" height="100" fill="#00FF00"/>
</svg>"##;
        let result = render_svg(svg.as_bytes(), 0, 0).unwrap();
        assert_eq!(result.dimensions(), (200, 100));
    }

    // ---- dilate_mask ----

    #[test]
    fn dilate_mask_expands_single_pixel() {
        let mut mask = ImageBuffer::from_pixel(10, 10, TRANSPARENT);
        mask.put_pixel(5, 5, Rgba([255, 255, 255, 255]));
        let dilated = dilate_mask(&mask, 2);
        let opaque_count = count_opaque_pixels(&dilated);
        assert!(opaque_count > 1);
        assert_eq!(read_pixel(&dilated, 5, 5).0[3], 255);
    }

    #[test]
    fn dilate_mask_zero_radius_preserves() {
        let mut mask = ImageBuffer::from_pixel(5, 5, TRANSPARENT);
        mask.put_pixel(2, 2, Rgba([255, 255, 255, 200]));
        let dilated = dilate_mask(&mask, 0);
        assert_eq!(read_pixel(&dilated, 2, 2).0[3], 200);
    }

    // ---- render_text_to_buffer ----

    #[test]
    fn render_text_empty_string_returns_tiny_buffer() {
        let font_bytes = include_test_font();
        let font = FontArc::try_from_vec(font_bytes).unwrap();
        let buf =
            render_text_to_buffer("", &font, 30.0, Rgba([255, 255, 255, 255]), None, 0).unwrap();
        assert!(buf.width() <= 1);
    }

    #[test]
    fn render_text_produces_visible_pixels() {
        let font_bytes = include_test_font();
        let font = FontArc::try_from_vec(font_bytes).unwrap();
        let buf =
            render_text_to_buffer("AB", &font, 40.0, Rgba([255, 255, 255, 255]), None, 0).unwrap();
        assert!(has_nontransparent_pixel(&buf));
    }

    #[test]
    fn render_text_with_stroke_produces_more_pixels() {
        let font_bytes = include_test_font();
        let font = FontArc::try_from_vec(font_bytes).unwrap();
        let without_stroke =
            render_text_to_buffer("X", &font, 40.0, Rgba([255, 255, 255, 255]), None, 0).unwrap();
        let with_stroke = render_text_to_buffer(
            "X",
            &font,
            40.0,
            Rgba([255, 255, 255, 255]),
            Some(Rgba([0, 0, 0, 255])),
            3,
        )
        .unwrap();
        let no_stroke_count = count_opaque_pixels(&without_stroke);
        let stroke_count = count_opaque_pixels(&with_stroke);
        assert!(
            stroke_count > no_stroke_count,
            "stroke should add pixels around the glyph"
        );
    }

    // ---- layout_glyphs ----

    #[test]
    fn layout_glyphs_for_ascii() {
        let font_bytes = include_test_font();
        let font = FontArc::try_from_vec(font_bytes).unwrap();
        let glyphs = layout_glyphs(&font, 24.0, "Hello");
        assert!(!glyphs.is_empty());
    }

    #[test]
    fn layout_glyphs_skips_control_chars() {
        let font_bytes = include_test_font();
        let font = FontArc::try_from_vec(font_bytes).unwrap();
        let glyphs = layout_glyphs(&font, 24.0, "A\nB\tC");
        assert_eq!(glyphs.len(), 3);
    }

    // ---- FontRegistry::scan_dir ----

    #[test]
    fn scan_dir_loads_ttf_files() {
        let dir = std::env::temp_dir().join("duskcue_overlay_font_test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let font_bytes = include_test_font();
        std::fs::write(dir.join("TestFont.ttf"), font_bytes).unwrap();
        let registry = FontRegistry::scan_dir(&dir);
        assert_eq!(registry.len(), 1);
        assert!(registry.resolve("testfont").is_some());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn scan_dir_ignores_non_font_files() {
        let dir = std::env::temp_dir().join("duskcue_overlay_font_ignore_test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("readme.txt"), b"not a font").unwrap();
        std::fs::write(dir.join("image.png"), b"not a font either").unwrap();
        let registry = FontRegistry::scan_dir(&dir);
        assert!(registry.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn scan_dir_missing_directory_returns_empty() {
        let registry = FontRegistry::scan_dir(Path::new("/nonexistent/path/that/does/not/exist"));
        assert!(registry.is_empty());
    }

    // ---- Test font ----

    fn include_test_font() -> Vec<u8> {
        minimal_ttf_bytes()
    }

    fn minimal_ttf_bytes() -> Vec<u8> {
        let font_data = include_bytes!("../../assets/fonts/Inter-Regular.ttf");
        font_data.to_vec()
    }
}

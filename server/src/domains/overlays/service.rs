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

use image::Rgba;
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::services::image_pipeline::{self, EncodeConfig};
use crate::services::overlays as overlay_svc;
use crate::services::overlays::{
    CanvasPreset, HorizontalAlignment, OverlayType, ResolvedOverlay, VerticalAlignment,
};
use crate::state::AppState;

use super::error::OverlayError;
use super::types::*;

pub fn validate_overlay_type(value: &str) -> Result<(), OverlayError> {
    if VALID_OVERLAY_TYPES.contains(&value) {
        Ok(())
    } else {
        Err(OverlayError::InvalidConditions(format!(
            "invalid overlay_type: {value}"
        )))
    }
}

pub fn validate_applies_to(value: &str) -> Result<(), OverlayError> {
    if VALID_APPLIES_TO.contains(&value) {
        Ok(())
    } else {
        Err(OverlayError::InvalidConditions(format!(
            "invalid applies_to: {value}"
        )))
    }
}

pub fn validate_horizontal_align(value: &str) -> Result<(), OverlayError> {
    if VALID_HORIZONTAL_ALIGN.contains(&value) {
        Ok(())
    } else {
        Err(OverlayError::InvalidConditions(format!(
            "invalid horizontal_align: {value}"
        )))
    }
}

pub fn validate_vertical_align(value: &str) -> Result<(), OverlayError> {
    if VALID_VERTICAL_ALIGN.contains(&value) {
        Ok(())
    } else {
        Err(OverlayError::InvalidConditions(format!(
            "invalid vertical_align: {value}"
        )))
    }
}

pub fn generate_slug(name: &str) -> String {
    name.to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

pub async fn list_overlays(
    _pool: &PgPool,
    _library_id: Option<Uuid>,
    _enabled_only: bool,
    _page: u32,
    _page_size: u32,
) -> Result<OverlayListResponse, OverlayError> {
    todo!("Phase 12 — overlay definition listing (CRUD)")
}

pub async fn get_overlay(_pool: &PgPool, _overlay_id: Uuid) -> Result<OverlayDefinitionResponse, OverlayError> {
    todo!("Phase 12 — overlay definition fetch (CRUD)")
}

pub async fn create_overlay(_pool: &PgPool, _req: CreateOverlayRequest) -> Result<OverlayDefinitionResponse, OverlayError> {
    todo!("Phase 12 — overlay definition creation (CRUD)")
}

pub async fn update_overlay(
    _pool: &PgPool,
    _overlay_id: Uuid,
    _req: UpdateOverlayRequest,
) -> Result<OverlayDefinitionResponse, OverlayError> {
    todo!("Phase 12 — overlay definition update (CRUD)")
}

pub async fn delete_overlay(_pool: &PgPool, _overlay_id: Uuid) -> Result<(), OverlayError> {
    todo!("Phase 12 — overlay definition deletion (CRUD)")
}

pub async fn apply_overlays(
    _pool: &PgPool,
    _req: ApplyOverlaysRequest,
) -> Result<ApplyOverlaysResponse, OverlayError> {
    todo!("Phase 12 Task 8 — overlay application worker integration")
}

pub async fn preview_overlay(
    state: &AppState,
    req: PreviewOverlayRequest,
) -> Result<PreviewOverlayResponse, OverlayError> {
    let pool = &state.pool;
    let media_item_id = req.media_item_id.unwrap();
    let artwork_type = req.artwork_type.as_deref().unwrap_or("poster");

    let canvas = CanvasPreset::from_artwork_type(artwork_type)
        .ok_or_else(|| OverlayError::InvalidConditions(format!("unsupported artwork_type: {artwork_type}")))?;

    let artwork_row = sqlx::query(
        r#"SELECT id, local_path FROM artwork
           WHERE media_item_id = $1 AND artwork_type = $2 AND "order" = 0
           LIMIT 1"#,
    )
    .bind(media_item_id)
    .bind(artwork_type)
    .fetch_optional(pool)
    .await?
    .ok_or(OverlayError::ImageFileNotFound(
        "no primary artwork found for media item".into(),
    ))?;

    let local_path: Option<String> = artwork_row.try_get("local_path")?;
    let source_path = local_path
        .as_ref()
        .ok_or_else(|| OverlayError::ImageFileNotFound("artwork has no local_path".into()))?;

    let source_bytes = std::fs::read(source_path).map_err(|e| {
        OverlayError::ImageFileNotFound(format!("failed to read source artwork: {e}"))
    })?;

    let source_img = image::load_from_memory(&source_bytes)
        .map_err(|e| OverlayError::CompositingFailed(format!("failed to decode source artwork: {e}")))?
        .to_rgba8();

    let definitions = load_overlay_definitions_for_preview(pool, req.overlay_ids.as_deref(), artwork_type).await?;

    let media_ctx = load_media_context(pool, media_item_id).await?;

    let mut resolved: Vec<ResolvedOverlay> = definitions
        .into_iter()
        .map(|d| row_to_resolved(&d, &media_ctx))
        .collect::<Result<Vec<_>, _>>()?;

    resolved = overlay_svc::resolve_groups(resolved);
    resolved = overlay_svc::apply_suppress_rules(resolved);
    overlay_svc::resolve_queue_positions(&mut resolved, overlay_svc::DEFAULT_QUEUE_SPACING);

    let fonts_dir = state.bootstrap.data_dir.join("fonts");
    let font_registry = overlay_svc::FontRegistry::scan_dir(&fonts_dir);

    let composite_result = overlay_svc::composite(&source_img, canvas, &resolved, &font_registry)
        .map_err(|e| OverlayError::CompositingFailed(e.to_string()))?;

    let encode_config = EncodeConfig::default();
    let (webp_bytes, _) = image_pipeline::encode_webp(&image::DynamicImage::ImageRgba8(composite_result), &encode_config)
        .map_err(|e| OverlayError::CompositingFailed(format!("failed to encode WebP: {e}")))?;

    let preview_dir = state.bootstrap.data_dir.join("cache").join("images").join("overlays").join("previews");
    std::fs::create_dir_all(&preview_dir)
        .map_err(|e| OverlayError::CompositingFailed(format!("failed to create preview dir: {e}")))?;

    let preview_filename = format!("preview_{}.webp", media_item_id);
    let preview_path = preview_dir.join(&preview_filename);
    std::fs::write(&preview_path, &webp_bytes)
        .map_err(|e| OverlayError::CompositingFailed(format!("failed to write preview: {e}")))?;

    let applied_ids: Vec<Uuid> = resolved.iter().map(|o| o.id).collect();
    let preview_url = format!("/cache/images/overlays/previews/{preview_filename}");

    Ok(PreviewOverlayResponse {
        media_item_id,
        artwork_type: artwork_type.to_string(),
        applied_overlay_ids: applied_ids,
        preview_url,
    })
}

pub async fn list_templates(_pool: &PgPool) -> Result<Vec<OverlayTemplateSummary>, OverlayError> {
    todo!("Phase 12 — community template listing")
}

pub async fn import_template(
    _pool: &PgPool,
    _import: OverlayTemplateImport,
) -> Result<OverlayTemplateResponse, OverlayError> {
    todo!("Phase 12 — community template import")
}

struct MediaContext {
    title: String,
    year: Option<i32>,
    resolution: Option<String>,
    video_codec: Option<String>,
    audio_codec: Option<String>,
    critic_rating: Option<f64>,
}

async fn load_media_context(pool: &PgPool, media_item_id: Uuid) -> Result<MediaContext, OverlayError> {
    let row = sqlx::query(
        r#"SELECT mi.title, mi.year, mi.rating_average,
                  (SELECT video_resolution FROM media_files WHERE media_item_id = mi.id AND is_healthy = true ORDER BY file_size DESC LIMIT 1) AS resolution,
                  (SELECT video_codec FROM media_files WHERE media_item_id = mi.id AND is_healthy = true ORDER BY file_size DESC LIMIT 1) AS video_codec,
                  (SELECT audio_codec FROM media_files WHERE media_item_id = mi.id AND is_healthy = true ORDER BY file_size DESC LIMIT 1) AS audio_codec
           FROM media_items mi WHERE mi.id = $1"#,
    )
    .bind(media_item_id)
    .fetch_optional(pool)
    .await?
    .ok_or(OverlayError::NotFound)?;

    Ok(MediaContext {
        title: row.try_get("title").unwrap_or_default(),
        year: row.try_get("year").ok().flatten(),
        resolution: row.try_get("resolution").ok().flatten(),
        video_codec: row.try_get("video_codec").ok().flatten(),
        audio_codec: row.try_get("audio_codec").ok().flatten(),
        critic_rating: row.try_get("rating_average").ok().flatten(),
    })
}

fn resolve_text_variables(template: &str, ctx: &MediaContext) -> String {
    let mut result = template.to_string();
    result = result.replace("<<title>>", &ctx.title);
    if let Some(year) = ctx.year {
        result = result.replace("<<year>>", &year.to_string());
    }
    if let Some(res) = &ctx.resolution {
        result = result.replace("<<resolution>>", res);
    }
    if let Some(codec) = &ctx.video_codec {
        result = result.replace("<<video_codec>>", codec);
    }
    if let Some(codec) = &ctx.audio_codec {
        result = result.replace("<<audio_codec>>", codec);
    }
    if let Some(rating) = ctx.critic_rating {
        result = result.replace("<<critic_rating>>", &format!("{:.1}", rating));
    }
    result
}

async fn load_overlay_definitions_for_preview(
    pool: &PgPool,
    overlay_ids: Option<&[Uuid]>,
    artwork_type: &str,
) -> Result<Vec<OverlayDefinitionRow>, OverlayError> {
    let rows = if let Some(ids) = overlay_ids
        && !ids.is_empty()
    {
        sqlx::query(
            r#"SELECT id, name, slug, library_id, overlay_type, image_path, text_template,
                      font_family, font_size, font_color, stroke_color, stroke_width,
                      back_color, back_width, back_height, back_radius, back_padding,
                      horizontal_offset, horizontal_align, vertical_offset, vertical_align,
                      scale_width, scale_height, group_name, weight, queue_name,
                      conditions, suppresses, applies_to, is_enabled, is_system, metadata
               FROM overlay_definitions
               WHERE id = ANY($1) AND is_enabled = true"#,
        )
        .bind(ids)
        .fetch_all(pool)
        .await?
    } else {
        sqlx::query(
            r#"SELECT id, name, slug, library_id, overlay_type, image_path, text_template,
                      font_family, font_size, font_color, stroke_color, stroke_width,
                      back_color, back_width, back_height, back_radius, back_padding,
                      horizontal_offset, horizontal_align, vertical_offset, vertical_align,
                      scale_width, scale_height, group_name, weight, queue_name,
                      conditions, suppresses, applies_to, is_enabled, is_system, metadata
               FROM overlay_definitions
               WHERE applies_to = $1 AND is_enabled = true"#,
        )
        .bind(artwork_type)
        .fetch_all(pool)
        .await?
    };

    rows.iter().map(row_to_definition_row).collect()
}

#[allow(clippy::too_many_lines)]
fn row_to_definition_row(row: &sqlx::postgres::PgRow) -> Result<OverlayDefinitionRow, OverlayError> {
    Ok(OverlayDefinitionRow {
        id: row.try_get("id")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
        name: row.try_get("name")?,
        slug: row.try_get("slug")?,
        library_id: row.try_get("library_id").ok().flatten(),
        overlay_type: row.try_get("overlay_type")?,
        image_path: row.try_get("image_path").ok().flatten(),
        text_template: row.try_get("text_template").ok().flatten(),
        font_family: row.try_get("font_family")?,
        font_size: row.try_get("font_size")?,
        font_color: row.try_get("font_color")?,
        stroke_color: row.try_get("stroke_color").ok().flatten(),
        stroke_width: row.try_get("stroke_width").ok().flatten(),
        back_color: row.try_get("back_color").ok().flatten(),
        back_width: row.try_get("back_width").ok().flatten(),
        back_height: row.try_get("back_height").ok().flatten(),
        back_radius: row.try_get("back_radius").ok().flatten(),
        back_padding: row.try_get("back_padding").ok().flatten(),
        horizontal_offset: row.try_get("horizontal_offset")?,
        horizontal_align: row.try_get("horizontal_align")?,
        vertical_offset: row.try_get("vertical_offset")?,
        vertical_align: row.try_get("vertical_align")?,
        scale_width: row.try_get("scale_width").ok().flatten(),
        scale_height: row.try_get("scale_height").ok().flatten(),
        group_name: row.try_get("group_name").ok().flatten(),
        weight: row.try_get("weight")?,
        queue_name: row.try_get("queue_name").ok().flatten(),
        conditions: row.try_get("conditions")?,
        suppresses: row
            .try_get::<Vec<String>, _>("suppresses")
            .unwrap_or_default(),
        applies_to: row.try_get("applies_to")?,
        is_enabled: row.try_get("is_enabled")?,
        is_system: row.try_get("is_system")?,
        metadata: row.try_get("metadata")?,
    })
}

fn row_to_resolved(row: &OverlayDefinitionRow, ctx: &MediaContext) -> Result<ResolvedOverlay, OverlayError> {
    let overlay_type = OverlayType::from_db_str(&row.overlay_type)
        .ok_or_else(|| OverlayError::InvalidConditions(format!("invalid overlay_type: {}", row.overlay_type)))?;

    let h_align = HorizontalAlignment::from_db_str(&row.horizontal_align)
        .unwrap_or(HorizontalAlignment::Left);
    let v_align = VerticalAlignment::from_db_str(&row.vertical_align)
        .unwrap_or(VerticalAlignment::Top);

    let font_color = overlay_svc::parse_hex_color(&row.font_color).unwrap_or(Rgba([255, 255, 255, 255]));
    let stroke_color = row.stroke_color.as_deref().and_then(|s| overlay_svc::parse_hex_color(s).ok());
    let back_color = row.back_color.as_deref().and_then(|s| overlay_svc::parse_hex_color(s).ok());

    let text = row.text_template.as_ref().map(|t| resolve_text_variables(t, ctx));

    let (image_bytes, image_is_svg) = if overlay_type == OverlayType::Image {
        if let Some(ref path) = row.image_path {
            let is_svg = path.to_ascii_lowercase().ends_with(".svg");
            match std::fs::read(path) {
                Ok(bytes) => (Some(bytes), is_svg),
                Err(e) => {
                    tracing::warn!(slug = %row.slug, error = %e, "failed to load overlay image, skipping bytes");
                    (None, is_svg)
                }
            }
        } else {
            (None, false)
        }
    } else {
        (None, false)
    };

    Ok(ResolvedOverlay {
        id: row.id,
        slug: row.slug.clone(),
        overlay_type,
        horizontal_align: h_align,
        horizontal_offset: row.horizontal_offset,
        vertical_align: v_align,
        vertical_offset: row.vertical_offset,
        group_name: row.group_name.clone(),
        weight: row.weight,
        queue_name: row.queue_name.clone(),
        suppresses: row.suppresses.clone(),
        image_bytes,
        image_is_svg,
        scale_width: row.scale_width.and_then(|v| u32::try_from(v).ok()),
        scale_height: row.scale_height.and_then(|v| u32::try_from(v).ok()),
        text,
        font_family: row.font_family.clone(),
        font_size: row.font_size as f32,
        font_color,
        stroke_color,
        stroke_width: row.stroke_width.and_then(|v| u32::try_from(v).ok()).unwrap_or(0),
        back_color,
        back_width: row.back_width.and_then(|v| u32::try_from(v).ok()),
        back_height: row.back_height.and_then(|v| u32::try_from(v).ok()),
        back_radius: row.back_radius.and_then(|v| u32::try_from(v).ok()).unwrap_or(0),
        back_padding: row.back_padding.and_then(|v| u32::try_from(v).ok()).unwrap_or(0),
    })
}

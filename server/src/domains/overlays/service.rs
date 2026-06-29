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

use crate::services::clean_art::{self, OverlayHashInput};
use crate::services::conditions::{self, MediaFilterContext};
use crate::services::image_pipeline::{self, EncodeConfig};
use crate::services::overlays as overlay_svc;
use crate::services::overlays::{
    CanvasPreset, HorizontalAlignment, OverlayType, ResolvedOverlay, VerticalAlignment,
};
use crate::state::AppState;

use crate::error::AppError;

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

const SELECT_CLAUSE: &str = "SELECT id, created_at, updated_at, name, slug, library_id, overlay_type, image_path, text_template, font_family, font_size, font_color, stroke_color, stroke_width, back_color, back_width, back_height, back_radius, back_padding, horizontal_offset, horizontal_align, vertical_offset, vertical_align, scale_width, scale_height, group_name, weight, queue_name, conditions, suppresses, applies_to, is_enabled, is_system, metadata FROM overlay_definitions";

const RETURNING_COLUMNS: &str = "id, created_at, updated_at, name, slug, library_id, overlay_type, image_path, text_template, font_family, font_size, font_color, stroke_color, stroke_width, back_color, back_width, back_height, back_radius, back_padding, horizontal_offset, horizontal_align, vertical_offset, vertical_align, scale_width, scale_height, group_name, weight, queue_name, conditions, suppresses, applies_to, is_enabled, is_system, metadata";

fn row_to_response(row: OverlayDefinitionRow) -> OverlayDefinitionResponse {
    OverlayDefinitionResponse {
        id: row.id,
        name: row.name,
        slug: row.slug,
        library_id: row.library_id,
        overlay_type: row.overlay_type,
        image_path: row.image_path,
        text_template: row.text_template,
        font_family: row.font_family,
        font_size: row.font_size,
        font_color: row.font_color,
        stroke_color: row.stroke_color,
        stroke_width: row.stroke_width,
        back_color: row.back_color,
        back_width: row.back_width,
        back_height: row.back_height,
        back_radius: row.back_radius,
        back_padding: row.back_padding,
        horizontal_offset: row.horizontal_offset,
        horizontal_align: row.horizontal_align,
        vertical_offset: row.vertical_offset,
        vertical_align: row.vertical_align,
        scale_width: row.scale_width,
        scale_height: row.scale_height,
        group_name: row.group_name,
        weight: row.weight,
        queue_name: row.queue_name,
        conditions: row.conditions,
        suppresses: row.suppresses,
        applies_to: row.applies_to,
        is_enabled: row.is_enabled,
        is_system: row.is_system,
        metadata: row.metadata,
        created_at: row.created_at,
        updated_at: row.updated_at,
    }
}

pub async fn list_overlays(
    pool: &PgPool,
    library_id: Option<Uuid>,
    enabled_only: bool,
    page: u32,
    page_size: u32,
) -> Result<OverlayListResponse, OverlayError> {
    let mut builder = sqlx::QueryBuilder::new(SELECT_CLAUSE);
    let mut where_started = false;
    if let Some(lib) = library_id {
        builder.push(" WHERE library_id = ").push_bind(lib);
        where_started = true;
    }
    if enabled_only {
        builder.push(if where_started { " AND" } else { " WHERE" });
        builder.push(" is_enabled = true");
    }

    let mut count_builder = sqlx::QueryBuilder::new("SELECT COUNT(*) FROM overlay_definitions");
    if let Some(lib) = library_id {
        count_builder.push(" WHERE library_id = ").push_bind(lib);
        where_started = true;
    } else {
        where_started = false;
    }
    if enabled_only {
        count_builder.push(if where_started { " AND" } else { " WHERE" });
        count_builder.push(" is_enabled = true");
    }

    builder.push(" ORDER BY applies_to, weight DESC, name");
    let limit: i64 = page_size.max(1) as i64;
    let offset: i64 = (page.saturating_sub(1) as i64) * limit;
    builder.push(" LIMIT ").push_bind(limit);
    builder.push(" OFFSET ").push_bind(offset);

    let rows = builder.build().fetch_all(pool).await?;
    let items: Vec<OverlayDefinitionResponse> = rows
        .iter()
        .map(row_to_definition_row)
        .filter_map(|r| r.ok())
        .map(row_to_response)
        .collect();

    let total: i64 = count_builder
        .build()
        .fetch_one(pool)
        .await?
        .try_get("count")
        .unwrap_or(0);

    Ok(OverlayListResponse { items, total })
}

pub async fn get_overlay(
    pool: &PgPool,
    overlay_id: Uuid,
) -> Result<OverlayDefinitionResponse, OverlayError> {
    let mut builder = sqlx::QueryBuilder::new(SELECT_CLAUSE);
    builder.push(" WHERE id = ").push_bind(overlay_id);
    let row = builder.build().fetch_optional(pool).await?;
    match row {
        Some(row) => Ok(row_to_response(row_to_definition_row(&row)?)),
        None => Err(OverlayError::NotFound),
    }
}

pub async fn create_overlay(
    pool: &PgPool,
    req: CreateOverlayRequest,
) -> Result<OverlayDefinitionResponse, OverlayError> {
    let slug = generate_slug(&req.name);
    let applies_to = req.applies_to.unwrap_or_else(|| "poster".to_string());
    let conditions = req.conditions.unwrap_or(serde_json::json!({}));
    let suppresses = req.suppresses.unwrap_or_default();
    let metadata = req.metadata.unwrap_or(serde_json::json!({}));

    let row = sqlx::query(
        r#"INSERT INTO overlay_definitions
           (name, slug, library_id, overlay_type, image_path, text_template,
            font_family, font_size, font_color, stroke_color, stroke_width,
            back_color, back_width, back_height, back_radius, back_padding,
            horizontal_offset, horizontal_align, vertical_offset, vertical_align,
            scale_width, scale_height, group_name, weight, queue_name,
            conditions, suppresses, applies_to, is_enabled, metadata)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16,
                   $17, $18, $19, $20, $21, $22, $23, $24, $25, $26, $27, $28, $29, $30)
           RETURNING id, created_at, updated_at, name, slug, library_id, overlay_type, image_path, text_template,
                     font_family, font_size, font_color, stroke_color, stroke_width,
                     back_color, back_width, back_height, back_radius, back_padding,
                     horizontal_offset, horizontal_align, vertical_offset, vertical_align,
                     scale_width, scale_height, group_name, weight, queue_name,
                     conditions, suppresses, applies_to, is_enabled, is_system, metadata"#,
    )
    .bind(req.name)
    .bind(slug)
    .bind(req.library_id)
    .bind(req.overlay_type)
    .bind(req.image_path)
    .bind(req.text_template)
    .bind(req.font_family.unwrap_or_else(|| "Inter".to_string()))
    .bind(req.font_size.unwrap_or(63))
    .bind(req.font_color.unwrap_or_else(|| "#FFFFFF".to_string()))
    .bind(req.stroke_color)
    .bind(req.stroke_width)
    .bind(req.back_color)
    .bind(req.back_width)
    .bind(req.back_height)
    .bind(req.back_radius)
    .bind(req.back_padding)
    .bind(req.horizontal_offset.unwrap_or(0))
    .bind(req.horizontal_align.unwrap_or_else(|| "left".to_string()))
    .bind(req.vertical_offset.unwrap_or(0))
    .bind(req.vertical_align.unwrap_or_else(|| "top".to_string()))
    .bind(req.scale_width)
    .bind(req.scale_height)
    .bind(req.group_name)
    .bind(req.weight.unwrap_or(0))
    .bind(req.queue_name)
    .bind(conditions)
    .bind(&suppresses)
    .bind(applies_to)
    .bind(req.is_enabled.unwrap_or(true))
    .bind(metadata)
    .fetch_one(pool)
    .await?;

    Ok(row_to_response(row_to_definition_row(&row)?))
}

pub async fn update_overlay(
    pool: &PgPool,
    overlay_id: Uuid,
    req: UpdateOverlayRequest,
) -> Result<OverlayDefinitionResponse, OverlayError> {
    let mut builder = sqlx::QueryBuilder::new("UPDATE overlay_definitions SET updated_at = now()");
    if let Some(name) = &req.name {
        builder.push(", name = ").push_bind(name.clone());
        builder.push(", slug = ").push_bind(generate_slug(name));
    }
    if let Some(library_id) = req.library_id {
        builder.push(", library_id = ").push_bind(library_id);
    }
    if let Some(image_path) = &req.image_path {
        builder
            .push(", image_path = ")
            .push_bind(image_path.clone());
    }
    if let Some(text_template) = &req.text_template {
        builder
            .push(", text_template = ")
            .push_bind(text_template.clone());
    }
    if let Some(font_family) = &req.font_family {
        builder
            .push(", font_family = ")
            .push_bind(font_family.clone());
    }
    if let Some(font_size) = req.font_size {
        builder.push(", font_size = ").push_bind(font_size);
    }
    if let Some(font_color) = &req.font_color {
        builder
            .push(", font_color = ")
            .push_bind(font_color.clone());
    }
    if let Some(stroke_color) = &req.stroke_color {
        builder
            .push(", stroke_color = ")
            .push_bind(stroke_color.clone());
    }
    if let Some(stroke_width) = req.stroke_width {
        builder.push(", stroke_width = ").push_bind(stroke_width);
    }
    if let Some(back_color) = &req.back_color {
        builder
            .push(", back_color = ")
            .push_bind(back_color.clone());
    }
    if let Some(back_width) = req.back_width {
        builder.push(", back_width = ").push_bind(back_width);
    }
    if let Some(back_height) = req.back_height {
        builder.push(", back_height = ").push_bind(back_height);
    }
    if let Some(back_radius) = req.back_radius {
        builder.push(", back_radius = ").push_bind(back_radius);
    }
    if let Some(back_padding) = req.back_padding {
        builder.push(", back_padding = ").push_bind(back_padding);
    }
    if let Some(horizontal_offset) = req.horizontal_offset {
        builder
            .push(", horizontal_offset = ")
            .push_bind(horizontal_offset);
    }
    if let Some(horizontal_align) = &req.horizontal_align {
        builder
            .push(", horizontal_align = ")
            .push_bind(horizontal_align.clone());
    }
    if let Some(vertical_offset) = req.vertical_offset {
        builder
            .push(", vertical_offset = ")
            .push_bind(vertical_offset);
    }
    if let Some(vertical_align) = &req.vertical_align {
        builder
            .push(", vertical_align = ")
            .push_bind(vertical_align.clone());
    }
    if let Some(scale_width) = req.scale_width {
        builder.push(", scale_width = ").push_bind(scale_width);
    }
    if let Some(scale_height) = req.scale_height {
        builder.push(", scale_height = ").push_bind(scale_height);
    }
    if let Some(group_name) = &req.group_name {
        builder
            .push(", group_name = ")
            .push_bind(group_name.clone());
    }
    if let Some(weight) = req.weight {
        builder.push(", weight = ").push_bind(weight);
    }
    if let Some(queue_name) = &req.queue_name {
        builder
            .push(", queue_name = ")
            .push_bind(queue_name.clone());
    }
    if let Some(conditions) = req.conditions {
        builder.push(", conditions = ").push_bind(conditions);
    }
    if let Some(suppresses) = &req.suppresses {
        builder
            .push(", suppresses = ")
            .push_bind(suppresses.clone());
    }
    if let Some(applies_to) = &req.applies_to {
        builder
            .push(", applies_to = ")
            .push_bind(applies_to.clone());
    }
    if let Some(is_enabled) = req.is_enabled {
        builder.push(", is_enabled = ").push_bind(is_enabled);
    }
    if let Some(metadata) = req.metadata {
        builder.push(", metadata = ").push_bind(metadata);
    }

    builder.push(" WHERE id = ").push_bind(overlay_id);
    builder.push(" RETURNING ");
    builder.push(RETURNING_COLUMNS);

    let row = builder
        .build()
        .fetch_optional(pool)
        .await?
        .ok_or(OverlayError::NotFound)?;
    Ok(row_to_response(row_to_definition_row(&row)?))
}

pub async fn delete_overlay(pool: &PgPool, overlay_id: Uuid) -> Result<(), AppError> {
    let row = sqlx::query("SELECT is_system FROM overlay_definitions WHERE id = $1")
        .bind(overlay_id)
        .fetch_optional(pool)
        .await
        .map_err(OverlayError::from)?;
    match row {
        None => Err(AppError::from(OverlayError::NotFound)),
        Some(row) => {
            let is_system: bool = row
                .try_get("is_system")
                .map_err(|e| AppError::from(OverlayError::Database(e)))?;
            if is_system {
                return Err(AppError::Conflict(
                    "system overlay definitions cannot be deleted; disable them instead".into(),
                ));
            }
            sqlx::query("DELETE FROM overlay_definitions WHERE id = $1")
                .bind(overlay_id)
                .execute(pool)
                .await
                .map_err(|e| AppError::from(OverlayError::Database(e)))?;
            Ok(())
        }
    }
}

pub async fn apply_overlays(
    state: &AppState,
    req: ApplyOverlaysRequest,
) -> Result<ApplyOverlaysResponse, OverlayError> {
    let result = crate::workers::overlay_compositor::apply_overlays_now(
        state,
        req.library_id,
        req.reapply_all.unwrap_or(false),
        req.max_concurrent,
    )
    .await?;

    Ok(ApplyOverlaysResponse {
        status: "completed".to_string(),
        queued_items: result.candidates as i64,
    })
}

/// The outcome of a single-item compositing pass.
#[derive(Debug, Clone)]
pub struct CompositeResult {
    pub media_item_id: Uuid,
    pub artwork_type: String,
    pub composited: bool,
    pub applied_count: usize,
}

/// Composite overlays for a single media item + artwork type, using the clean
/// art preservation pipeline.
///
/// This is the single-item entry point called by the `overlay_compositor`
/// worker (Task 8) and by the admin-triggered apply endpoint. It:
///
/// 1. Loads enabled overlay definitions for the artwork type.
/// 2. Evaluates conditions against the media item context.
/// 3. Computes a config hash from the matching definitions + source artwork ID.
/// 4. Compares against stored state — if the hash matches and `reapply_all` is
///    false, skips re-compositing (returns `composited: false`).
/// 5. Otherwise: ensures a clean backup exists, composites from it, saves the
///    overlaid result to cache, and upserts `artwork_overlay_state`.
///
/// Source artwork is never modified — only the clean backup (derived) and the
/// overlaid result (derived) are written, both under `/cache/images/`.
pub async fn composite_and_persist(
    state: &AppState,
    media_item_id: Uuid,
    artwork_type: &str,
    reapply_all: bool,
) -> Result<CompositeResult, OverlayError> {
    let pool = &state.pool;
    let data_dir = &state.bootstrap.data_dir;

    let canvas = CanvasPreset::from_artwork_type(artwork_type).ok_or_else(|| {
        OverlayError::InvalidConditions(format!("unsupported artwork_type: {artwork_type}"))
    })?;

    let config = state.runtime_config.load();
    let encode_config = EncodeConfig {
        lossy_quality: config.metadata.overlay_image_quality as f32,
    };
    drop(config);

    let media_ctx = load_media_context(pool, media_item_id).await?;
    let filter_ctx = media_ctx.to_filter_context();

    let definitions = load_overlay_definitions_for_preview(pool, None, artwork_type).await?;

    let matching: Vec<OverlayDefinitionRow> = definitions
        .into_iter()
        .filter(|d| {
            (d.library_id.is_none() || d.library_id == media_ctx.library_id)
                && conditions::evaluate(&d.conditions, &filter_ctx)
        })
        .collect();

    if matching.is_empty() {
        let removed = clean_art::delete_overlay_state(pool, media_item_id, artwork_type).await?;
        if removed {
            tracing::debug!(%media_item_id, %artwork_type, "no matching overlays — removed existing overlay state");
        }
        return Ok(CompositeResult {
            media_item_id,
            artwork_type: artwork_type.to_string(),
            composited: false,
            applied_count: 0,
        });
    }

    let clean =
        clean_art::ensure_clean_backup(pool, data_dir, media_item_id, artwork_type, &encode_config)
            .await?;

    let hash_inputs: Vec<OverlayHashInput> = matching
        .iter()
        .map(|d| OverlayHashInput {
            id: d.id,
            updated_at: d.updated_at,
        })
        .collect();
    let config_hash = clean_art::compute_config_hash(&hash_inputs, clean.source_artwork_id);

    if !reapply_all
        && let Some(ref existing) =
            clean_art::get_overlay_state(pool, media_item_id, artwork_type).await?
        && existing.overlay_config_hash == config_hash
        && existing.overlaid_art_path.is_some()
    {
        return Ok(CompositeResult {
            media_item_id,
            artwork_type: artwork_type.to_string(),
            composited: false,
            applied_count: matching.len(),
        });
    }

    let mut resolved: Vec<ResolvedOverlay> = matching
        .iter()
        .map(|d| row_to_resolved(d, &media_ctx))
        .collect::<Result<Vec<_>, _>>()?;

    resolved = overlay_svc::resolve_groups(resolved);
    resolved = overlay_svc::apply_suppress_rules(resolved);
    overlay_svc::resolve_queue_positions(&mut resolved, overlay_svc::DEFAULT_QUEUE_SPACING);

    let fonts_dir = data_dir.join("fonts");
    let font_registry = overlay_svc::FontRegistry::scan_dir(&fonts_dir);

    let composited = overlay_svc::composite(&clean.image, canvas, &resolved, &font_registry)
        .map_err(|e| OverlayError::CompositingFailed(e.to_string()))?;

    let (webp_bytes, _) =
        image_pipeline::encode_webp(&image::DynamicImage::ImageRgba8(composited), &encode_config)
            .map_err(|e| {
            OverlayError::CompositingFailed(format!("failed to encode composited WebP: {e}"))
        })?;

    let overlaid_path =
        clean_art::save_overlaid_result(data_dir, media_item_id, artwork_type, &webp_bytes)?;

    let applied_ids: Vec<Uuid> = resolved.iter().map(|o| o.id).collect();
    clean_art::upsert_overlay_state(
        pool,
        media_item_id,
        artwork_type,
        &applied_ids,
        &config_hash,
        &clean.path.to_string_lossy(),
        Some(&overlaid_path.to_string_lossy()),
    )
    .await?;

    tracing::info!(
        %media_item_id, %artwork_type,
        applied = applied_ids.len(),
        "overlays composited and persisted"
    );

    Ok(CompositeResult {
        media_item_id,
        artwork_type: artwork_type.to_string(),
        composited: true,
        applied_count: applied_ids.len(),
    })
}

pub async fn preview_overlay(
    state: &AppState,
    req: PreviewOverlayRequest,
) -> Result<PreviewOverlayResponse, OverlayError> {
    let pool = &state.pool;
    let media_item_id = req.media_item_id.unwrap();
    let artwork_type = req.artwork_type.as_deref().unwrap_or("poster");

    let canvas = CanvasPreset::from_artwork_type(artwork_type).ok_or_else(|| {
        OverlayError::InvalidConditions(format!("unsupported artwork_type: {artwork_type}"))
    })?;

    let config = state.runtime_config.load();
    let encode_config = EncodeConfig {
        lossy_quality: config.metadata.overlay_image_quality as f32,
    };
    drop(config);

    let clean = clean_art::ensure_clean_backup(
        pool,
        &state.bootstrap.data_dir,
        media_item_id,
        artwork_type,
        &encode_config,
    )
    .await?;

    let definitions =
        load_overlay_definitions_for_preview(pool, req.overlay_ids.as_deref(), artwork_type)
            .await?;

    let media_ctx = load_media_context(pool, media_item_id).await?;
    let filter_ctx = media_ctx.to_filter_context();

    let matching_definitions: Vec<_> = definitions
        .into_iter()
        .filter(|d| {
            (d.library_id.is_none() || d.library_id == media_ctx.library_id)
                && conditions::evaluate(&d.conditions, &filter_ctx)
        })
        .collect();

    let mut resolved: Vec<ResolvedOverlay> = matching_definitions
        .into_iter()
        .map(|d| row_to_resolved(&d, &media_ctx))
        .collect::<Result<Vec<_>, _>>()?;

    resolved = overlay_svc::resolve_groups(resolved);
    resolved = overlay_svc::apply_suppress_rules(resolved);
    overlay_svc::resolve_queue_positions(&mut resolved, overlay_svc::DEFAULT_QUEUE_SPACING);

    let fonts_dir = state.bootstrap.data_dir.join("fonts");
    let font_registry = overlay_svc::FontRegistry::scan_dir(&fonts_dir);

    let composite_result = overlay_svc::composite(&clean.image, canvas, &resolved, &font_registry)
        .map_err(|e| OverlayError::CompositingFailed(e.to_string()))?;

    let (webp_bytes, _) = image_pipeline::encode_webp(
        &image::DynamicImage::ImageRgba8(composite_result),
        &encode_config,
    )
    .map_err(|e| OverlayError::CompositingFailed(format!("failed to encode WebP: {e}")))?;

    let preview_dir = state
        .bootstrap
        .data_dir
        .join("cache")
        .join("images")
        .join("overlays")
        .join("previews");
    std::fs::create_dir_all(&preview_dir).map_err(|e| {
        OverlayError::CompositingFailed(format!("failed to create preview dir: {e}"))
    })?;

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

pub async fn list_templates(pool: &PgPool) -> Result<Vec<OverlayTemplateSummary>, OverlayError> {
    let rows = sqlx::query(
        r#"SELECT metadata->>'template_name' AS name,
                  COALESCE((metadata->>'template_version')::int, 1) AS version,
                  COUNT(*)::bigint AS overlay_count
           FROM overlay_definitions
           WHERE metadata ? 'template_name'
           GROUP BY metadata->>'template_name',
                    COALESCE((metadata->>'template_version')::int, 1)
           ORDER BY metadata->>'template_name'"#,
    )
    .fetch_all(pool)
    .await?;

    let summaries: Vec<OverlayTemplateSummary> = rows
        .iter()
        .map(|row| OverlayTemplateSummary {
            name: row.try_get("name").unwrap_or_default(),
            version: row.try_get("version").unwrap_or(1),
            overlay_count: row.try_get::<i64, _>("overlay_count").unwrap_or(0) as usize,
        })
        .collect();

    Ok(summaries)
}

pub async fn import_template(
    pool: &PgPool,
    import: OverlayTemplateImport,
) -> Result<OverlayTemplateResponse, OverlayError> {
    let mut overlay_ids = Vec::with_capacity(import.overlays.len());
    let template_name = import.name.clone();
    let template_version = import.version.unwrap_or(1);

    for entry in import.overlays {
        let slug = generate_slug(&entry.name);
        let applies_to = entry.applies_to.as_deref().unwrap_or("poster");
        let conditions = entry.conditions.unwrap_or(serde_json::json!({}));
        let suppresses = entry.suppresses.unwrap_or_default();
        let metadata = serde_json::json!({
            "template_name": template_name,
            "template_version": template_version,
            "library_id": import.library_id,
        });

        let row = sqlx::query(
            r#"INSERT INTO overlay_definitions
               (name, slug, library_id, overlay_type, image_path, text_template,
                font_family, font_size, font_color,
                back_color, back_radius,
                horizontal_offset, horizontal_align, vertical_offset, vertical_align,
                group_name, weight, queue_name, conditions, suppresses, applies_to, metadata)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15,
                       $16, $17, $18, $19, $20, $21, $22)
               RETURNING id"#,
        )
        .bind(entry.name)
        .bind(slug)
        .bind(import.library_id)
        .bind(entry.overlay_type)
        .bind(entry.image_path)
        .bind(entry.text_template)
        .bind(entry.font_family.unwrap_or_else(|| "Inter".to_string()))
        .bind(entry.font_size.unwrap_or(63))
        .bind(entry.font_color.unwrap_or_else(|| "#FFFFFF".to_string()))
        .bind(entry.back_color)
        .bind(entry.back_radius.unwrap_or(0))
        .bind(entry.horizontal_offset.unwrap_or(0))
        .bind(entry.horizontal_align.unwrap_or_else(|| "left".to_string()))
        .bind(entry.vertical_offset.unwrap_or(0))
        .bind(entry.vertical_align.unwrap_or_else(|| "top".to_string()))
        .bind(entry.group_name)
        .bind(entry.weight.unwrap_or(0))
        .bind(entry.queue_name)
        .bind(conditions)
        .bind(&suppresses)
        .bind(applies_to)
        .bind(metadata)
        .fetch_one(pool)
        .await?;

        let id: Uuid = row.try_get("id")?;
        overlay_ids.push(id);
    }

    let imported_count = overlay_ids.len();
    Ok(OverlayTemplateResponse {
        imported_count,
        overlay_ids,
    })
}

struct OverlayMediaContext {
    title: String,
    year: Option<i32>,
    runtime_seconds: Option<i32>,
    content_rating: Option<String>,
    critic_rating: Option<f64>,
    audience_rating: Option<f64>,
    rating_vote_count: Option<i32>,
    media_type: String,
    library_id: Option<Uuid>,
    genres: Vec<String>,
    video_resolution: Option<String>,
    video_codec: Option<String>,
    video_dynamic_range: Option<String>,
    audio_codec: Option<String>,
    audio_channels: Option<i32>,
    container_format: Option<String>,
    has_dolby_vision: bool,
    has_multiple_versions: bool,
    edition: Option<String>,
    original_language: Option<String>,
    streaming_on: Vec<String>,
}

impl OverlayMediaContext {
    fn to_filter_context(&self) -> MediaFilterContext {
        MediaFilterContext {
            media_type: self.media_type.clone(),
            library_id: self.library_id,
            content_rating: self.content_rating.clone(),
            critic_rating: self.critic_rating,
            genres: self.genres.clone(),
            video_resolution: self.video_resolution.clone(),
            video_codec: self.video_codec.clone(),
            video_dynamic_range: self.video_dynamic_range.clone(),
            audio_codec: self.audio_codec.clone(),
            audio_channels: self.audio_channels,
            container_format: self.container_format.clone(),
            has_dolby_vision: self.has_dolby_vision,
            has_multiple_versions: self.has_multiple_versions,
            edition: self.edition.clone(),
            original_language: self.original_language.clone(),
            streaming_on: self.streaming_on.clone(),
        }
    }
}

async fn load_media_context(
    pool: &PgPool,
    media_item_id: Uuid,
) -> Result<OverlayMediaContext, OverlayError> {
    let row = sqlx::query(
        r#"SELECT mi.title,
                  mi.year,
                  mi.runtime_seconds,
                  mi.content_rating,
                  mi.rating_average,
                  mi.rating_vote_count,
                  mi.type,
                  mi.library_id,
                  mi.metadata,
                  mf.video_resolution,
                  mf.video_codec,
                  mf.video_dynamic_range,
                  mf.audio_codec,
                  mf.audio_channels,
                  mf.container_format,
                  fc.cnt AS file_count,
                  COALESCE(gl.genres, '{}'::text[]) AS genres
           FROM media_items mi
           LEFT JOIN LATERAL (
               SELECT video_resolution, video_codec, video_dynamic_range,
                      audio_codec, audio_channels, container_format
               FROM media_files
               WHERE media_item_id = mi.id AND is_healthy = true
               ORDER BY file_size DESC
               LIMIT 1
           ) mf ON true
           LEFT JOIN LATERAL (
               SELECT COUNT(*)::int AS cnt
               FROM media_files
               WHERE media_item_id = mi.id AND is_healthy = true
           ) fc ON true
           LEFT JOIN LATERAL (
               SELECT array_agg(g.name) AS genres
               FROM media_genres mg
               JOIN genres g ON g.id = mg.genre_id
               WHERE mg.media_item_id = mi.id
           ) gl ON true
           WHERE mi.id = $1"#,
    )
    .bind(media_item_id)
    .fetch_optional(pool)
    .await?
    .ok_or(OverlayError::NotFound)?;

    let metadata: serde_json::Value = row.try_get("metadata").unwrap_or(serde_json::Value::Null);

    let video_dynamic_range: Option<String> = row.try_get("video_dynamic_range").ok().flatten();
    let has_dolby_vision = video_dynamic_range
        .as_deref()
        .map(|v| v.to_ascii_lowercase().starts_with("dolby_vision"))
        .unwrap_or(false);

    let file_count: i32 = row.try_get("file_count").unwrap_or(1);
    let has_multiple_versions = file_count > 1;

    let genres: Vec<String> = row.try_get::<Vec<String>, _>("genres").unwrap_or_default();

    let original_language = metadata
        .get("original_language")
        .and_then(|v| v.as_str())
        .map(String::from);

    let streaming_on = extract_streaming_services(&metadata);

    let edition = metadata
        .get("edition")
        .and_then(|v| v.as_str())
        .map(String::from);

    Ok(OverlayMediaContext {
        title: row.try_get("title").unwrap_or_default(),
        year: row.try_get("year").ok().flatten(),
        runtime_seconds: row.try_get("runtime_seconds").ok().flatten(),
        content_rating: row.try_get("content_rating").ok().flatten(),
        critic_rating: row.try_get("rating_average").ok().flatten(),
        audience_rating: metadata.get("audience_rating").and_then(|v| v.as_f64()),
        rating_vote_count: row.try_get("rating_vote_count").ok().flatten(),
        media_type: row.try_get("type").unwrap_or_default(),
        library_id: row.try_get("library_id").ok().flatten(),
        genres,
        video_resolution: row.try_get("video_resolution").ok().flatten(),
        video_codec: row.try_get("video_codec").ok().flatten(),
        video_dynamic_range,
        audio_codec: row.try_get("audio_codec").ok().flatten(),
        audio_channels: row.try_get("audio_channels").ok().flatten(),
        container_format: row.try_get("container_format").ok().flatten(),
        has_dolby_vision,
        has_multiple_versions,
        edition,
        original_language,
        streaming_on,
    })
}

fn extract_streaming_services(metadata: &serde_json::Value) -> Vec<String> {
    if let Some(arr) = metadata.get("streaming_on").and_then(|v| v.as_array()) {
        return arr
            .iter()
            .filter_map(|v| v.as_str().map(String::from))
            .collect();
    }
    if let Some(arr) = metadata.get("watch_providers").and_then(|v| v.as_array()) {
        return arr
            .iter()
            .filter_map(|v| v.as_str().map(String::from))
            .collect();
    }
    if let Some(s) = metadata.get("streaming_provider").and_then(|v| v.as_str()) {
        return vec![s.to_string()];
    }
    Vec::new()
}

fn resolve_text_variables(template: &str, ctx: &OverlayMediaContext) -> String {
    let mut result = template.to_string();
    result = result.replace("<<title>>", &ctx.title);
    if let Some(year) = ctx.year {
        result = result.replace("<<year>>", &year.to_string());
    }
    if let Some(res) = &ctx.video_resolution {
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
        result = result.replace("<<critic_rating/>>", &format!("{:.1}", rating / 2.0));
    }
    if let Some(rating) = ctx.audience_rating {
        result = result.replace("<<audience_rating>>", &format!("{:.1}", rating));
    }
    if let Some(votes) = ctx.rating_vote_count {
        result = result.replace("<<rating_vote_count>>", &votes.to_string());
    }
    if let Some(dr) = &ctx.video_dynamic_range {
        result = result.replace("<<video_dynamic_range>>", dr);
    }
    if let Some(container) = &ctx.container_format {
        result = result.replace("<<container>>", container);
    }
    if let Some(channels) = ctx.audio_channels {
        result = result.replace("<<audio_channels>>", &format_audio_channels(channels));
    }
    if let Some(content_rating) = &ctx.content_rating {
        result = result.replace("<<content_rating>>", content_rating);
    }
    if let Some(runtime) = ctx.runtime_seconds {
        let minutes = runtime / 60;
        result = result.replace("<<runtime>>", &minutes.to_string());
        result = result.replace("<<runtimeH>>", &(minutes / 60).to_string());
        result = result.replace("<<runtimeM>>", &(minutes % 60).to_string());
    }
    if let Some(edition) = &ctx.edition {
        result = result.replace("<<edition>>", edition);
    }
    result
}

fn format_audio_channels(channels: i32) -> String {
    match channels {
        1 => "1.0".into(),
        2 => "2.0".into(),
        6 => "5.1".into(),
        7 => "6.1".into(),
        8 => "7.1".into(),
        n => format!("{}.0", n),
    }
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
            r#"SELECT id, created_at, updated_at, name, slug, library_id, overlay_type, image_path, text_template,
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
            r#"SELECT id, created_at, updated_at, name, slug, library_id, overlay_type, image_path, text_template,
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
fn row_to_definition_row(
    row: &sqlx::postgres::PgRow,
) -> Result<OverlayDefinitionRow, OverlayError> {
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

fn row_to_resolved(
    row: &OverlayDefinitionRow,
    ctx: &OverlayMediaContext,
) -> Result<ResolvedOverlay, OverlayError> {
    let overlay_type = OverlayType::from_db_str(&row.overlay_type).ok_or_else(|| {
        OverlayError::InvalidConditions(format!("invalid overlay_type: {}", row.overlay_type))
    })?;

    let h_align = HorizontalAlignment::from_db_str(&row.horizontal_align)
        .unwrap_or(HorizontalAlignment::Left);
    let v_align =
        VerticalAlignment::from_db_str(&row.vertical_align).unwrap_or(VerticalAlignment::Top);

    let font_color =
        overlay_svc::parse_hex_color(&row.font_color).unwrap_or(Rgba([255, 255, 255, 255]));
    let stroke_color = row
        .stroke_color
        .as_deref()
        .and_then(|s| overlay_svc::parse_hex_color(s).ok());
    let back_color = row
        .back_color
        .as_deref()
        .and_then(|s| overlay_svc::parse_hex_color(s).ok());

    let text = row
        .text_template
        .as_ref()
        .map(|t| resolve_text_variables(t, ctx));

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
        stroke_width: row
            .stroke_width
            .and_then(|v| u32::try_from(v).ok())
            .unwrap_or(0),
        back_color,
        back_width: row.back_width.and_then(|v| u32::try_from(v).ok()),
        back_height: row.back_height.and_then(|v| u32::try_from(v).ok()),
        back_radius: row
            .back_radius
            .and_then(|v| u32::try_from(v).ok())
            .unwrap_or(0),
        back_padding: row
            .back_padding
            .and_then(|v| u32::try_from(v).ok())
            .unwrap_or(0),
    })
}

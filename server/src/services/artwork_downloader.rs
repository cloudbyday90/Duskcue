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

use std::path::{Path, PathBuf};

use reqwest::Client;
use sqlx::PgPool;
use uuid::Uuid;

use super::metadata::{ImageEntry, ImagesData, TmdbConfig};

const MAX_POSTERS: usize = 5;
const MAX_BACKDROPS: usize = 3;
const MAX_LOGOS: usize = 2;

pub struct ArtworkDownloadResult {
    pub downloaded: u32,
    pub skipped_existing: u32,
    pub failed: u32,
}

pub struct ArtworkDownloadContext<'a> {
    pub pool: &'a PgPool,
    pub http: &'a Client,
    pub tmdb_config: &'a TmdbConfig,
    pub data_dir: &'a Path,
}

pub struct NewArtworkRow<'a> {
    pub media_item_id: Uuid,
    pub artwork_type: &'a str,
    pub source_url: &'a str,
    pub local_path: &'a Path,
    pub width: u32,
    pub height: u32,
    pub language: Option<&'a str>,
    pub order: i32,
}

pub async fn download_and_store_artwork(
    ctx: &ArtworkDownloadContext<'_>,
    media_item_id: Uuid,
    tmdb_id: u64,
    images: &ImagesData,
    artwork_auto_download: bool,
) -> ArtworkDownloadResult {
    if !artwork_auto_download {
        return ArtworkDownloadResult {
            downloaded: 0,
            skipped_existing: 0,
            failed: 0,
        };
    }

    let mut result = ArtworkDownloadResult {
        downloaded: 0,
        skipped_existing: 0,
        failed: 0,
    };

    let sorted_posters = sort_by_votes(&images.posters);
    let sorted_backdrops = sort_by_votes(&images.backdrops);
    let sorted_logos = sort_by_votes(&images.logos);

    for (order, image) in sorted_posters.iter().take(MAX_POSTERS).enumerate() {
        match download_single_artwork(ctx, media_item_id, tmdb_id, image, "poster", order as i32)
            .await
        {
            DownloadOutcome::Downloaded => result.downloaded += 1,
            DownloadOutcome::SkippedExisting => result.skipped_existing += 1,
            DownloadOutcome::Failed => result.failed += 1,
        }
    }

    for (order, image) in sorted_backdrops.iter().take(MAX_BACKDROPS).enumerate() {
        match download_single_artwork(ctx, media_item_id, tmdb_id, image, "backdrop", order as i32)
            .await
        {
            DownloadOutcome::Downloaded => result.downloaded += 1,
            DownloadOutcome::SkippedExisting => result.skipped_existing += 1,
            DownloadOutcome::Failed => result.failed += 1,
        }
    }

    for (order, image) in sorted_logos.iter().take(MAX_LOGOS).enumerate() {
        match download_single_artwork(ctx, media_item_id, tmdb_id, image, "logo", order as i32)
            .await
        {
            DownloadOutcome::Downloaded => result.downloaded += 1,
            DownloadOutcome::SkippedExisting => result.skipped_existing += 1,
            DownloadOutcome::Failed => result.failed += 1,
        }
    }

    if result.downloaded > 0 || result.skipped_existing > 0 {
        tracing::info!(
            media_item_id = %media_item_id,
            tmdb_id = tmdb_id,
            downloaded = result.downloaded,
            skipped = result.skipped_existing,
            failed = result.failed,
            "Artwork download complete"
        );
    }

    result
}

enum DownloadOutcome {
    Downloaded,
    SkippedExisting,
    Failed,
}

async fn download_single_artwork(
    ctx: &ArtworkDownloadContext<'_>,
    media_item_id: Uuid,
    tmdb_id: u64,
    image: &ImageEntry,
    artwork_type: &str,
    order: i32,
) -> DownloadOutcome {
    let source_url = format!(
        "{}original{}",
        ctx.tmdb_config.secure_image_base_url, image.file_path
    );

    let existing = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM artwork WHERE media_item_id = $1 AND source_url = $2",
    )
    .bind(media_item_id)
    .bind(&source_url)
    .fetch_one(ctx.pool)
    .await
    .unwrap_or(0);

    if existing > 0 {
        return DownloadOutcome::SkippedExisting;
    }

    let local_path = build_local_path(ctx.data_dir, tmdb_id, artwork_type, &image.file_path);

    match download_image(ctx.http, &source_url, &local_path).await {
        Ok(()) => {
            match insert_artwork_row(
                ctx.pool,
                &NewArtworkRow {
                    media_item_id,
                    artwork_type,
                    source_url: &source_url,
                    local_path: &local_path,
                    width: image.width,
                    height: image.height,
                    language: image.language.as_deref(),
                    order,
                },
            )
            .await
            {
                Ok(()) => DownloadOutcome::Downloaded,
                Err(e) => {
                    tracing::warn!(
                        media_item_id = %media_item_id,
                        artwork_type = artwork_type,
                        error = %e,
                        "Failed to insert artwork row"
                    );
                    let _ = tokio::fs::remove_file(&local_path).await;
                    DownloadOutcome::Failed
                }
            }
        }
        Err(e) => {
            tracing::warn!(
                source_url = %source_url,
                error = %e,
                "Failed to download artwork image"
            );
            DownloadOutcome::Failed
        }
    }
}

fn sort_by_votes(images: &[ImageEntry]) -> Vec<&ImageEntry> {
    let mut sorted: Vec<&ImageEntry> = images.iter().collect();
    sorted.sort_by(|a, b| {
        b.vote_count.cmp(&a.vote_count).then_with(|| {
            b.vote_average
                .partial_cmp(&a.vote_average)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
    });
    sorted
}

fn build_local_path(
    data_dir: &Path,
    tmdb_id: u64,
    artwork_type: &str,
    tmdb_file_path: &str,
) -> PathBuf {
    let subdir = match artwork_type {
        "backdrop" => "backdrops",
        "logo" => "logos",
        _ => "posters",
    };

    let filename = Path::new(tmdb_file_path)
        .file_name()
        .map(|f| f.to_string_lossy().to_string())
        .unwrap_or_else(|| format!("{tmdb_id}.jpg"));

    let prefixed_filename = format!("{tmdb_id}_{filename}");

    data_dir
        .join("metadata")
        .join("artwork")
        .join("tmdb")
        .join(subdir)
        .join(prefixed_filename)
}

async fn download_image(
    http: &Client,
    url: &str,
    local_path: &Path,
) -> Result<(), ArtworkDownloadError> {
    if let Some(parent) = local_path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| ArtworkDownloadError::Io(e.to_string()))?;
    }

    let response = http
        .get(url)
        .send()
        .await
        .map_err(|e| ArtworkDownloadError::Network(e.to_string()))?;

    if !response.status().is_success() {
        return Err(ArtworkDownloadError::Network(format!(
            "HTTP {} for {}",
            response.status(),
            url
        )));
    }

    let bytes = response
        .bytes()
        .await
        .map_err(|e| ArtworkDownloadError::Network(e.to_string()))?;

    tokio::fs::write(local_path, &bytes)
        .await
        .map_err(|e| ArtworkDownloadError::Io(e.to_string()))?;

    Ok(())
}

async fn insert_artwork_row(pool: &PgPool, row: &NewArtworkRow<'_>) -> Result<(), sqlx::Error> {
    let local_path_str = row.local_path.to_string_lossy().to_string();

    sqlx::query(
        r#"INSERT INTO artwork (media_item_id, artwork_type, source_url, local_path, width, height, language, provider, "order", source_type)
           VALUES ($1, $2, $3, $4, $5, $6, $7, 'tmdb', $8, 'tmdb')
           ON CONFLICT (media_item_id, artwork_type, "order") DO NOTHING"#,
    )
    .bind(row.media_item_id)
    .bind(row.artwork_type)
    .bind(row.source_url)
    .bind(&local_path_str)
    .bind(row.width as i32)
    .bind(row.height as i32)
    .bind(row.language)
    .bind(row.order)
    .execute(pool)
    .await?;

    Ok(())
}

#[derive(Debug, thiserror::Error)]
enum ArtworkDownloadError {
    #[error("network error: {0}")]
    Network(String),
    #[error("I/O error: {0}")]
    Io(String),
}

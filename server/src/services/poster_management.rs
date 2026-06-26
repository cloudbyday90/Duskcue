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

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use ignore::{WalkBuilder, WalkState};
use image::GenericImageView;
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Postgres, Row, Transaction};
use uuid::Uuid;

const IMAGE_EXTENSIONS: &[&str] = &["jpg", "jpeg", "png", "webp"];

#[derive(Debug, Clone, Default, Serialize)]
pub struct PosterImportResult {
    pub discovered: u64,
    pub matched: u64,
    pub imported: u64,
    pub skipped: u64,
    pub failed: u64,
    pub locked: u64,
}

impl PosterImportResult {
    fn record_import(&mut self, locked: bool) {
        self.imported += 1;
        if locked {
            self.locked += 1;
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CommunityPackImport {
    pub name: String,
    pub version: Option<i32>,
    pub author: Option<String>,
    pub pack_root: Option<PathBuf>,
    #[serde(default)]
    pub lock_imported: bool,
    #[serde(default)]
    pub artwork: Vec<CommunityArtworkEntry>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CommunityArtworkEntry {
    pub tmdb_id: Option<i64>,
    pub title: Option<String>,
    pub year: Option<i32>,
    pub media_type: Option<String>,
    pub poster: Option<String>,
    pub backdrop: Option<String>,
    pub season_number: Option<i32>,
    pub source: Option<String>,
}

#[derive(Debug, Clone)]
pub struct AssetScanConfig {
    pub path: PathBuf,
    pub lock_imported: bool,
}

#[derive(Debug, Clone)]
struct MatchedArtwork {
    target: ArtworkTarget,
    artwork_type: String,
    source_path: PathBuf,
    source_url: String,
    source_type: &'static str,
    provider: &'static str,
    lock: bool,
}

#[derive(Debug, Clone, Copy)]
enum ArtworkTarget {
    MediaItem(Uuid),
    Collection(Uuid),
}

#[derive(Debug, Clone)]
struct ImageInfo {
    width: i32,
    height: i32,
    extension: String,
}

#[derive(Debug, thiserror::Error)]
pub enum PosterManagementError {
    #[error("asset directory is not configured")]
    AssetDirectoryNotConfigured,
    #[error("path is outside the configured root: {0}")]
    UnsafePath(String),
    #[error("path does not exist: {0}")]
    PathNotFound(String),
    #[error("artwork not found: {0}")]
    ArtworkNotFound(Uuid),
    #[error("community pack is empty")]
    EmptyCommunityPack,
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("image decode error: {0}")]
    Image(#[from] image::ImageError),
}

pub async fn scan_asset_directory(
    pool: &PgPool,
    data_dir: &Path,
    config: AssetScanConfig,
) -> Result<PosterImportResult, PosterManagementError> {
    let root = canonical_existing_dir(&config.path)?;
    let files = discover_image_files(&root);
    let mut result = PosterImportResult {
        discovered: files.len() as u64,
        ..Default::default()
    };

    for file in files {
        let canonical = match canonical_child(&root, &file) {
            Ok(path) => path,
            Err(e) => {
                tracing::warn!(path = %file.display(), error = %e, "Asset image skipped");
                result.failed += 1;
                continue;
            }
        };

        match match_asset_path(pool, &root, &canonical).await {
            Ok(Some(mut matched)) => {
                result.matched += 1;
                matched.lock = config.lock_imported;
                match import_matched_artwork(pool, data_dir, &matched).await {
                    Ok(imported) => {
                        if imported {
                            result.record_import(matched.lock);
                        } else {
                            result.skipped += 1;
                        }
                    }
                    Err(e) => {
                        tracing::warn!(path = %canonical.display(), error = %e, "Asset artwork import failed");
                        result.failed += 1;
                    }
                }
            }
            Ok(None) => result.skipped += 1,
            Err(e) => {
                tracing::warn!(path = %canonical.display(), error = %e, "Asset artwork matching failed");
                result.failed += 1;
            }
        }
    }

    Ok(result)
}

pub async fn import_community_pack(
    pool: &PgPool,
    data_dir: &Path,
    pack: CommunityPackImport,
) -> Result<PosterImportResult, PosterManagementError> {
    if pack.artwork.is_empty() {
        return Err(PosterManagementError::EmptyCommunityPack);
    }

    let pack_root = pack
        .pack_root
        .as_deref()
        .map(canonical_existing_dir)
        .transpose()?;

    let mut result = PosterImportResult {
        discovered: pack.artwork.len() as u64,
        ..Default::default()
    };

    for entry in &pack.artwork {
        let Some(media_item_id) = match_community_entry(pool, entry).await? else {
            result.skipped += 1;
            continue;
        };

        result.matched += 1;
        let items = community_entry_artwork(&pack, entry, media_item_id, pack_root.as_deref())?;
        if items.is_empty() {
            result.skipped += 1;
            continue;
        }

        for matched in items {
            match import_matched_artwork(pool, data_dir, &matched).await {
                Ok(imported) => {
                    if imported {
                        result.record_import(matched.lock);
                    } else {
                        result.skipped += 1;
                    }
                }
                Err(e) => {
                    tracing::warn!(error = %e, "Community artwork import failed");
                    result.failed += 1;
                }
            }
        }
    }

    Ok(result)
}

pub async fn set_artwork_lock(
    pool: &PgPool,
    artwork_id: Uuid,
    locked: bool,
) -> Result<(), PosterManagementError> {
    let result = sqlx::query("UPDATE artwork SET is_locked = $2, updated_at = now() WHERE id = $1")
        .bind(artwork_id)
        .bind(locked)
        .execute(pool)
        .await?;

    if result.rows_affected() == 0 {
        return Err(PosterManagementError::ArtworkNotFound(artwork_id));
    }

    Ok(())
}

pub async fn select_artwork(
    pool: &PgPool,
    artwork_id: Uuid,
    lock: Option<bool>,
) -> Result<(), PosterManagementError> {
    let row = sqlx::query("SELECT media_item_id, artwork_type FROM artwork WHERE id = $1")
        .bind(artwork_id)
        .fetch_optional(pool)
        .await?
        .ok_or(PosterManagementError::ArtworkNotFound(artwork_id))?;

    let media_item_id: Option<Uuid> = row.try_get("media_item_id")?;
    let artwork_type: String = row.try_get("artwork_type")?;

    let mut tx = pool.begin().await?;
    promote_existing_artwork(&mut tx, artwork_id, media_item_id, &artwork_type, lock).await?;
    tx.commit().await?;

    Ok(())
}

async fn import_matched_artwork(
    pool: &PgPool,
    data_dir: &Path,
    matched: &MatchedArtwork,
) -> Result<bool, PosterManagementError> {
    let info = read_image_info(&matched.source_path)?;
    let stored_path = copy_artwork_to_store(data_dir, matched, &info).await?;
    let local_path = stored_path.to_string_lossy().to_string();

    let existing = find_existing_artwork(pool, matched, &local_path).await?;
    let mut tx = pool.begin().await?;

    match (matched.target, existing) {
        (ArtworkTarget::MediaItem(media_item_id), Some(id)) => {
            sqlx::query(
                r#"UPDATE artwork
                   SET local_path = $2, width = $3, height = $4, source_url = $5,
                       provider = $6, is_locked = $7, source_type = $8, updated_at = now()
                   WHERE id = $1"#,
            )
            .bind(id)
            .bind(&local_path)
            .bind(info.width)
            .bind(info.height)
            .bind(&matched.source_url)
            .bind(matched.provider)
            .bind(matched.lock)
            .bind(matched.source_type)
            .execute(&mut *tx)
            .await?;
            promote_existing_artwork(
                &mut tx,
                id,
                Some(media_item_id),
                &matched.artwork_type,
                Some(matched.lock),
            )
            .await?;
            delete_overlay_state(&mut tx, media_item_id, &matched.artwork_type).await?;
        }
        (ArtworkTarget::MediaItem(media_item_id), None) => {
            demote_media_artwork(&mut tx, media_item_id, &matched.artwork_type).await?;
            let id = Uuid::now_v7();
            sqlx::query(
                r#"INSERT INTO artwork
                   (id, media_item_id, artwork_type, source_url, local_path, width, height,
                    provider, "order", is_locked, source_type)
                   VALUES ($1, $2, $3, $4, $5, $6, $7, $8, 0, $9, $10)"#,
            )
            .bind(id)
            .bind(media_item_id)
            .bind(&matched.artwork_type)
            .bind(&matched.source_url)
            .bind(&local_path)
            .bind(info.width)
            .bind(info.height)
            .bind(matched.provider)
            .bind(matched.lock)
            .bind(matched.source_type)
            .execute(&mut *tx)
            .await?;
            restore_demoted_media_artwork(&mut tx, media_item_id, &matched.artwork_type).await?;
            delete_overlay_state(&mut tx, media_item_id, &matched.artwork_type).await?;
        }
        (ArtworkTarget::Collection(collection_id), Some(id)) => {
            sqlx::query(
                r#"UPDATE artwork
                   SET local_path = $2, width = $3, height = $4, source_url = $5,
                       provider = $6, is_locked = $7, source_type = $8, updated_at = now()
                   WHERE id = $1"#,
            )
            .bind(id)
            .bind(&local_path)
            .bind(info.width)
            .bind(info.height)
            .bind(&matched.source_url)
            .bind(matched.provider)
            .bind(matched.lock)
            .bind(matched.source_type)
            .execute(&mut *tx)
            .await?;
            update_collection_artwork(&mut tx, collection_id, &matched.artwork_type, id).await?;
        }
        (ArtworkTarget::Collection(collection_id), None) => {
            let id = Uuid::now_v7();
            sqlx::query(
                r#"INSERT INTO artwork
                   (id, media_item_id, artwork_type, source_url, local_path, width, height,
                    provider, "order", is_locked, source_type)
                   VALUES ($1, NULL, $2, $3, $4, $5, $6, $7, 0, $8, $9)"#,
            )
            .bind(id)
            .bind(&matched.artwork_type)
            .bind(&matched.source_url)
            .bind(&local_path)
            .bind(info.width)
            .bind(info.height)
            .bind(matched.provider)
            .bind(matched.lock)
            .bind(matched.source_type)
            .execute(&mut *tx)
            .await?;
            update_collection_artwork(&mut tx, collection_id, &matched.artwork_type, id).await?;
        }
    }

    tx.commit().await?;
    Ok(true)
}

async fn find_existing_artwork(
    pool: &PgPool,
    matched: &MatchedArtwork,
    local_path: &str,
) -> Result<Option<Uuid>, sqlx::Error> {
    match matched.target {
        ArtworkTarget::MediaItem(media_item_id) => {
            sqlx::query_scalar(
                r#"SELECT id FROM artwork
                   WHERE media_item_id = $1
                     AND artwork_type = $2
                     AND (source_url = $3 OR local_path = $4)
                   LIMIT 1"#,
            )
            .bind(media_item_id)
            .bind(&matched.artwork_type)
            .bind(&matched.source_url)
            .bind(local_path)
            .fetch_optional(pool)
            .await
        }
        ArtworkTarget::Collection(collection_id) => {
            sqlx::query_scalar(
                r#"SELECT a.id
                   FROM artwork a
                   JOIN collections c ON c.poster_artwork_id = a.id OR c.backdrop_artwork_id = a.id
                   WHERE c.id = $1 AND (a.source_url = $2 OR a.local_path = $3)
                   LIMIT 1"#,
            )
            .bind(collection_id)
            .bind(&matched.source_url)
            .bind(local_path)
            .fetch_optional(pool)
            .await
        }
    }
}

async fn promote_existing_artwork(
    tx: &mut Transaction<'_, Postgres>,
    artwork_id: Uuid,
    media_item_id: Option<Uuid>,
    artwork_type: &str,
    lock: Option<bool>,
) -> Result<(), sqlx::Error> {
    let Some(media_item_id) = media_item_id else {
        if let Some(locked) = lock {
            sqlx::query("UPDATE artwork SET is_locked = $2, updated_at = now() WHERE id = $1")
                .bind(artwork_id)
                .bind(locked)
                .execute(&mut **tx)
                .await?;
        }
        return Ok(());
    };

    demote_media_artwork(tx, media_item_id, artwork_type).await?;
    sqlx::query(
        r#"UPDATE artwork
           SET "order" = 0,
               is_locked = COALESCE($4, is_locked),
               updated_at = now()
           WHERE id = $1 AND media_item_id = $2 AND artwork_type = $3"#,
    )
    .bind(artwork_id)
    .bind(media_item_id)
    .bind(artwork_type)
    .bind(lock)
    .execute(&mut **tx)
    .await?;
    restore_demoted_media_artwork(tx, media_item_id, artwork_type).await?;
    delete_overlay_state(tx, media_item_id, artwork_type).await?;

    Ok(())
}

async fn demote_media_artwork(
    tx: &mut Transaction<'_, Postgres>,
    media_item_id: Uuid,
    artwork_type: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"UPDATE artwork
           SET "order" = "order" + 1000, updated_at = now()
           WHERE media_item_id = $1 AND artwork_type = $2"#,
    )
    .bind(media_item_id)
    .bind(artwork_type)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn restore_demoted_media_artwork(
    tx: &mut Transaction<'_, Postgres>,
    media_item_id: Uuid,
    artwork_type: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"UPDATE artwork
           SET "order" = "order" - 999, updated_at = now()
           WHERE media_item_id = $1 AND artwork_type = $2 AND "order" >= 1000"#,
    )
    .bind(media_item_id)
    .bind(artwork_type)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn delete_overlay_state(
    tx: &mut Transaction<'_, Postgres>,
    media_item_id: Uuid,
    artwork_type: &str,
) -> Result<(), sqlx::Error> {
    let overlay_type = match artwork_type {
        "thumbnail" => "episode_thumb",
        other => other,
    };
    sqlx::query("DELETE FROM artwork_overlay_state WHERE media_item_id = $1 AND artwork_type = $2")
        .bind(media_item_id)
        .bind(overlay_type)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

async fn update_collection_artwork(
    tx: &mut Transaction<'_, Postgres>,
    collection_id: Uuid,
    artwork_type: &str,
    artwork_id: Uuid,
) -> Result<(), sqlx::Error> {
    if artwork_type == "backdrop" {
        sqlx::query(
            "UPDATE collections SET backdrop_artwork_id = $2, updated_at = now() WHERE id = $1",
        )
        .bind(collection_id)
        .bind(artwork_id)
        .execute(&mut **tx)
        .await?;
    } else {
        sqlx::query(
            "UPDATE collections SET poster_artwork_id = $2, updated_at = now() WHERE id = $1",
        )
        .bind(collection_id)
        .bind(artwork_id)
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}

async fn match_asset_path(
    pool: &PgPool,
    root: &Path,
    path: &Path,
) -> Result<Option<MatchedArtwork>, PosterManagementError> {
    let Ok(relative) = path.strip_prefix(root) else {
        return Err(PosterManagementError::UnsafePath(
            path.display().to_string(),
        ));
    };
    let parts = relative
        .components()
        .map(|c| c.as_os_str().to_string_lossy().to_string())
        .collect::<Vec<_>>();

    if parts.len() < 2 {
        return Ok(None);
    }

    let section = parts[0].to_ascii_lowercase();
    let filename = Path::new(parts.last().unwrap_or(&String::new()))
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();

    match section.as_str() {
        "movies" | "movie" => {
            let folder = parts.get(1).map(String::as_str).unwrap_or_default();
            let Some((artwork_type, _)) = classify_item_artwork(&filename) else {
                return Ok(None);
            };
            let Some(media_item_id) = match_media_item(pool, "movie", folder, path).await? else {
                return Ok(None);
            };
            Ok(Some(MatchedArtwork {
                target: ArtworkTarget::MediaItem(media_item_id),
                artwork_type,
                source_path: path.to_path_buf(),
                source_url: format!("asset://{}", path.display()),
                source_type: "asset_directory",
                provider: "asset_directory",
                lock: true,
            }))
        }
        "tv" | "series" | "shows" => {
            let folder = parts.get(1).map(String::as_str).unwrap_or_default();
            if let Some(season_number) = parse_season_artwork_name(&filename) {
                let Some(series_id) = match_media_item(pool, "series", folder, path).await? else {
                    return Ok(None);
                };
                let season_id = sqlx::query_scalar(
                    "SELECT id FROM seasons WHERE series_id = $1 AND season_number = $2",
                )
                .bind(series_id)
                .bind(season_number)
                .fetch_optional(pool)
                .await?;
                return Ok(season_id.map(|id| MatchedArtwork {
                    target: ArtworkTarget::MediaItem(id),
                    artwork_type: "season_poster".to_string(),
                    source_path: path.to_path_buf(),
                    source_url: format!("asset://{}", path.display()),
                    source_type: "asset_directory",
                    provider: "asset_directory",
                    lock: true,
                }));
            }

            let Some((artwork_type, _)) = classify_item_artwork(&filename) else {
                return Ok(None);
            };
            let Some(media_item_id) = match_media_item(pool, "series", folder, path).await? else {
                return Ok(None);
            };
            Ok(Some(MatchedArtwork {
                target: ArtworkTarget::MediaItem(media_item_id),
                artwork_type,
                source_path: path.to_path_buf(),
                source_url: format!("asset://{}", path.display()),
                source_type: "asset_directory",
                provider: "asset_directory",
                lock: true,
            }))
        }
        "collections" => {
            let Some(collection_id) = match_collection(pool, &filename).await? else {
                return Ok(None);
            };
            Ok(Some(MatchedArtwork {
                target: ArtworkTarget::Collection(collection_id),
                artwork_type: "poster".to_string(),
                source_path: path.to_path_buf(),
                source_url: format!("asset://{}", path.display()),
                source_type: "asset_directory",
                provider: "asset_directory",
                lock: true,
            }))
        }
        _ => Ok(None),
    }
}

async fn match_media_item(
    pool: &PgPool,
    media_type: &str,
    folder_name: &str,
    path: &Path,
) -> Result<Option<Uuid>, sqlx::Error> {
    if let Some(tmdb_id) = parse_tmdb_id(folder_name).or_else(|| {
        path.file_stem()
            .and_then(|s| s.to_str())
            .and_then(parse_tmdb_id)
    }) {
        let id = sqlx::query_scalar(
            "SELECT id FROM media_items WHERE type = $1 AND tmdb_id = $2 LIMIT 1",
        )
        .bind(media_type)
        .bind(tmdb_id)
        .fetch_optional(pool)
        .await?;
        if id.is_some() {
            return Ok(id);
        }
    }

    let (title, year) = split_title_year(folder_name);
    sqlx::query_scalar(
        r#"SELECT id FROM media_items
           WHERE type = $1
             AND (lower(title) = lower($2) OR lower(sort_title) = lower($2))
             AND ($3::int IS NULL OR EXTRACT(YEAR FROM premiere_date)::int = $3)
           ORDER BY CASE WHEN match_state IN ('confirmed', 'manual') THEN 0 ELSE 1 END, created_at DESC
           LIMIT 1"#,
    )
    .bind(media_type)
    .bind(title)
    .bind(year)
    .fetch_optional(pool)
    .await
}

async fn match_collection(pool: &PgPool, name: &str) -> Result<Option<Uuid>, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT id FROM collections WHERE lower(name) = lower($1) OR lower(slug) = lower($2) LIMIT 1",
    )
    .bind(name)
    .bind(slugify(name))
    .fetch_optional(pool)
    .await
}

async fn match_community_entry(
    pool: &PgPool,
    entry: &CommunityArtworkEntry,
) -> Result<Option<Uuid>, sqlx::Error> {
    let media_type = entry.media_type.as_deref().unwrap_or("movie");
    if let Some(tmdb_id) = entry.tmdb_id {
        let id = sqlx::query_scalar(
            "SELECT id FROM media_items WHERE type = $1 AND tmdb_id = $2 LIMIT 1",
        )
        .bind(media_type)
        .bind(tmdb_id)
        .fetch_optional(pool)
        .await?;
        if id.is_some() {
            return Ok(id);
        }
    }

    let Some(title) = entry.title.as_deref() else {
        return Ok(None);
    };

    sqlx::query_scalar(
        r#"SELECT id FROM media_items
           WHERE type = $1
             AND lower(title) = lower($2)
             AND ($3::int IS NULL OR EXTRACT(YEAR FROM premiere_date)::int = $3)
           LIMIT 1"#,
    )
    .bind(media_type)
    .bind(title)
    .bind(entry.year)
    .fetch_optional(pool)
    .await
}

fn community_entry_artwork(
    pack: &CommunityPackImport,
    entry: &CommunityArtworkEntry,
    media_item_id: Uuid,
    pack_root: Option<&Path>,
) -> Result<Vec<MatchedArtwork>, PosterManagementError> {
    let mut out = Vec::new();
    if let Some(poster) = entry.poster.as_deref() {
        out.push(community_artwork_match(
            pack,
            poster,
            pack_root,
            media_item_id,
            "poster",
        )?);
    }
    if let Some(backdrop) = entry.backdrop.as_deref() {
        out.push(community_artwork_match(
            pack,
            backdrop,
            pack_root,
            media_item_id,
            "backdrop",
        )?);
    }
    Ok(out)
}

fn community_artwork_match(
    pack: &CommunityPackImport,
    relative: &str,
    pack_root: Option<&Path>,
    media_item_id: Uuid,
    artwork_type: &str,
) -> Result<MatchedArtwork, PosterManagementError> {
    let Some(root) = pack_root else {
        return Err(PosterManagementError::PathNotFound(relative.to_string()));
    };
    let source_path = canonical_child(root, &root.join(relative))?;
    Ok(MatchedArtwork {
        target: ArtworkTarget::MediaItem(media_item_id),
        artwork_type: artwork_type.to_string(),
        source_path,
        source_url: format!(
            "community://{}/{}/{}",
            pack.name,
            pack.version.unwrap_or(1),
            relative.replace('\\', "/")
        ),
        source_type: "community",
        provider: "community",
        lock: pack.lock_imported,
    })
}

fn discover_image_files(root: &Path) -> Vec<PathBuf> {
    let mut builder = WalkBuilder::new(root);
    builder
        .hidden(false)
        .git_ignore(false)
        .git_exclude(false)
        .git_global(false)
        .follow_links(false);

    let seen = std::sync::Mutex::new(Vec::new());
    builder.build_parallel().run(|| {
        let seen = &seen;
        Box::new(move |entry| {
            let Ok(entry) = entry else {
                return WalkState::Continue;
            };
            if !entry.file_type().is_some_and(|ft| ft.is_file()) {
                return WalkState::Continue;
            }
            if is_supported_image_path(entry.path())
                && let Ok(mut guard) = seen.lock()
            {
                guard.push(entry.path().to_path_buf());
            }
            WalkState::Continue
        })
    });

    seen.into_inner().unwrap_or_default()
}

fn read_image_info(path: &Path) -> Result<ImageInfo, PosterManagementError> {
    let bytes = std::fs::read(path)?;
    let image = image::load_from_memory(&bytes)?;
    let (width, height) = image.dimensions();
    Ok(ImageInfo {
        width: width as i32,
        height: height as i32,
        extension: normalized_extension(path).unwrap_or_else(|| "jpg".to_string()),
    })
}

async fn copy_artwork_to_store(
    data_dir: &Path,
    matched: &MatchedArtwork,
    info: &ImageInfo,
) -> Result<PathBuf, PosterManagementError> {
    let bytes = tokio::fs::read(&matched.source_path).await?;
    let hash = blake3::hash(&bytes).to_hex().to_string();
    let prefix = match matched.target {
        ArtworkTarget::MediaItem(id) | ArtworkTarget::Collection(id) => id.to_string(),
    };
    let filename = format!("{}_{}.{}", prefix, &hash[..16], info.extension);
    let subdir = artwork_subdir(&matched.artwork_type);
    let path = data_dir
        .join("metadata")
        .join("artwork")
        .join(matched.source_type)
        .join(subdir)
        .join(filename);

    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    tokio::fs::write(&path, bytes).await?;
    Ok(path)
}

fn canonical_existing_dir(path: &Path) -> Result<PathBuf, PosterManagementError> {
    let canonical = path
        .canonicalize()
        .map_err(|_| PosterManagementError::PathNotFound(path.display().to_string()))?;
    if !canonical.is_dir() {
        return Err(PosterManagementError::PathNotFound(
            path.display().to_string(),
        ));
    }
    Ok(canonical)
}

fn canonical_child(root: &Path, path: &Path) -> Result<PathBuf, PosterManagementError> {
    let canonical = path
        .canonicalize()
        .map_err(|_| PosterManagementError::PathNotFound(path.display().to_string()))?;
    if !canonical.starts_with(root) {
        return Err(PosterManagementError::UnsafePath(
            canonical.display().to_string(),
        ));
    }
    Ok(canonical)
}

fn classify_item_artwork(name: &str) -> Option<(String, bool)> {
    let normalized = name.to_ascii_lowercase().replace([' ', '-', '_'], "");
    match normalized.as_str() {
        "poster" | "folder" | "cover" => Some(("poster".to_string(), true)),
        "background" | "backdrop" | "fanart" => Some(("backdrop".to_string(), true)),
        _ => None,
    }
}

fn parse_season_artwork_name(name: &str) -> Option<i32> {
    let normalized = name.to_ascii_lowercase().replace([' ', '_', '-'], "");
    let rest = normalized.strip_prefix("season")?;
    rest.parse::<i32>().ok().filter(|n| *n >= 0)
}

fn parse_tmdb_id(value: &str) -> Option<i64> {
    let lower = value.to_ascii_lowercase();
    for marker in ["tmdb-", "tmdb_", "tmdb "] {
        if let Some(pos) = lower.find(marker) {
            let start = pos + marker.len();
            let digits = lower[start..]
                .chars()
                .take_while(|c| c.is_ascii_digit())
                .collect::<String>();
            if let Ok(id) = digits.parse::<i64>() {
                return Some(id);
            }
        }
    }
    let stem = lower.trim();
    if stem.chars().all(|c| c.is_ascii_digit()) {
        return stem.parse::<i64>().ok();
    }
    None
}

fn split_title_year(value: &str) -> (&str, Option<i32>) {
    let trimmed = value.trim();
    if let Some(open) = trimmed.rfind('(')
        && trimmed.ends_with(')')
    {
        let year_str = &trimmed[open + 1..trimmed.len() - 1];
        if year_str.len() == 4
            && year_str.chars().all(|c| c.is_ascii_digit())
            && let Ok(year) = year_str.parse::<i32>()
        {
            return (trimmed[..open].trim(), Some(year));
        }
    }
    (trimmed, None)
}

fn slugify(value: &str) -> String {
    let mut out = String::new();
    let mut last_dash = false;
    for ch in value.chars().flat_map(|c| c.to_lowercase()) {
        if ch.is_ascii_alphanumeric() {
            out.push(ch);
            last_dash = false;
        } else if !last_dash && !out.is_empty() {
            out.push('-');
            last_dash = true;
        }
    }
    out.trim_matches('-').to_string()
}

fn is_supported_image_path(path: &Path) -> bool {
    normalized_extension(path)
        .as_deref()
        .is_some_and(|ext| IMAGE_EXTENSIONS.contains(&ext))
}

fn normalized_extension(path: &Path) -> Option<String> {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
}

fn artwork_subdir(artwork_type: &str) -> &'static str {
    match artwork_type {
        "backdrop" => "backdrops",
        "logo" => "logos",
        "banner" => "banners",
        "season_poster" => "season_posters",
        "thumbnail" | "episode_thumb" => "thumbnails",
        _ => "posters",
    }
}

pub fn dedupe_paths(paths: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut seen = HashSet::new();
    paths
        .into_iter()
        .filter(|p| seen.insert(p.to_string_lossy().to_ascii_lowercase()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_title_year_extracts_suffix_year() {
        assert_eq!(
            split_title_year("The Matrix (1999)"),
            ("The Matrix", Some(1999))
        );
        assert_eq!(split_title_year("The Matrix"), ("The Matrix", None));
    }

    #[test]
    fn parse_tmdb_id_supports_markers_and_plain_stems() {
        assert_eq!(parse_tmdb_id("The Matrix {tmdb-603}"), Some(603));
        assert_eq!(parse_tmdb_id("tmdb_603_poster"), Some(603));
        assert_eq!(parse_tmdb_id("603"), Some(603));
        assert_eq!(parse_tmdb_id("poster"), None);
    }

    #[test]
    fn classify_item_artwork_matches_common_names() {
        assert_eq!(classify_item_artwork("poster").unwrap().0, "poster");
        assert_eq!(classify_item_artwork("folder").unwrap().0, "poster");
        assert_eq!(classify_item_artwork("background").unwrap().0, "backdrop");
        assert!(classify_item_artwork("Season 01").is_none());
    }

    #[test]
    fn parse_season_artwork_names() {
        assert_eq!(parse_season_artwork_name("Season 01"), Some(1));
        assert_eq!(parse_season_artwork_name("Season_02"), Some(2));
        assert_eq!(parse_season_artwork_name("Season03"), Some(3));
        assert_eq!(parse_season_artwork_name("poster"), None);
    }

    #[test]
    fn slugify_normalizes_collection_names() {
        assert_eq!(
            slugify("Marvel Cinematic Universe"),
            "marvel-cinematic-universe"
        );
        assert_eq!(slugify(" Studio: Ghibli "), "studio-ghibli");
    }

    #[test]
    fn supported_image_path_is_case_insensitive() {
        assert!(is_supported_image_path(Path::new("poster.JPG")));
        assert!(is_supported_image_path(Path::new("poster.webp")));
        assert!(!is_supported_image_path(Path::new("poster.gif")));
    }

    #[test]
    fn artwork_subdirs_match_storage_policy() {
        assert_eq!(artwork_subdir("poster"), "posters");
        assert_eq!(artwork_subdir("backdrop"), "backdrops");
        assert_eq!(artwork_subdir("season_poster"), "season_posters");
        assert_eq!(artwork_subdir("episode_thumb"), "thumbnails");
    }

    #[test]
    fn dedupe_paths_is_case_insensitive() {
        let items = vec![PathBuf::from("A.jpg"), PathBuf::from("a.JPG")];
        assert_eq!(dedupe_paths(items).len(), 1);
    }
}

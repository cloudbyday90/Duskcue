use sqlx::PgPool;
use sqlx::Row;
use uuid::Uuid;

use super::metadata::{
    CastEntry, CreditsData, CrewEntry, EnrichmentOrchestrator, EnrichmentResult, MetadataError,
};

pub async fn enrich_items_for_library(
    pool: &PgPool,
    orchestrator: &EnrichmentOrchestrator,
    library_id: Uuid,
    errors: &mut Vec<String>,
) {
    let items = match fetch_enrichable_items(pool, library_id).await {
        Ok(items) => items,
        Err(e) => {
            tracing::error!(
                library_id = %library_id,
                error = %e,
                "Failed to fetch enrichable items"
            );
            errors.push(format!("Failed to fetch enrichable items: {e}"));
            return;
        }
    };

    if items.is_empty() {
        tracing::info!(
            library_id = %library_id,
            "Phase 5 (Enrich) — no items to enrich"
        );
        return;
    }

    tracing::info!(
        library_id = %library_id,
        item_count = items.len(),
        "Phase 5 (Enrich) — starting metadata enrichment"
    );

    let mut enriched = 0u64;
    let mut failed = 0u64;

    for item in &items {
        match enrich_single_item(orchestrator, pool, item).await {
            Ok(()) => enriched += 1,
            Err(e) => {
                failed += 1;
                tracing::warn!(
                    media_item_id = %item.id,
                    title = %item.title,
                    item_type = %item.item_type,
                    error = %e,
                    "Enrichment failed for item"
                );
            }
        }
    }

    tracing::info!(
        library_id = %library_id,
        enriched,
        failed,
        "Phase 5 (Enrich) — complete"
    );
}

struct EnrichableItem {
    id: Uuid,
    item_type: String,
    title: String,
    year: Option<i32>,
    tmdb_id: Option<i64>,
    imdb_id: Option<String>,
}

async fn fetch_enrichable_items(
    pool: &PgPool,
    library_id: Uuid,
) -> Result<Vec<EnrichableItem>, sqlx::Error> {
    let rows = sqlx::query(
        r#"SELECT mi.id, mi.type, mi.title,
                  EXTRACT(YEAR FROM mi.premiere_date)::INT AS year,
                  mi.tmdb_id, mi.imdb_id
           FROM media_items mi
           WHERE mi.library_id = $1
             AND mi.type IN ('movie', 'series')
             AND mi.match_state IN ('auto_matched', 'unmatched')
           ORDER BY mi.created_at ASC"#,
    )
    .bind(library_id)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .iter()
        .map(|r| EnrichableItem {
            id: r.get("id"),
            item_type: r.get("type"),
            title: r.get("title"),
            year: r.try_get("year").ok(),
            tmdb_id: r.try_get("tmdb_id").ok(),
            imdb_id: r.try_get("imdb_id").ok(),
        })
        .collect())
}

async fn enrich_single_item(
    orchestrator: &EnrichmentOrchestrator,
    pool: &PgPool,
    item: &EnrichableItem,
) -> Result<(), MetadataError> {
    let result = match item.item_type.as_str() {
        "movie" => {
            orchestrator
                .enrich_movie(
                    item.tmdb_id.map(|v| v as u64),
                    item.imdb_id.as_deref(),
                    &item.title,
                    item.year.map(|y| y as u32),
                    Some(item.id),
                )
                .await?
        }
        "series" => {
            orchestrator
                .enrich_tv(
                    item.tmdb_id.map(|v| v as u64),
                    item.imdb_id.as_deref(),
                    &item.title,
                    item.year.map(|y| y as u32),
                    Some(item.id),
                )
                .await?
        }
        _ => return Ok(()),
    };

    persist_enrichment_result(pool, item.id, &item.item_type, &result).await?;

    Ok(())
}

async fn persist_enrichment_result(
    pool: &PgPool,
    media_item_id: Uuid,
    item_type: &str,
    result: &EnrichmentResult,
) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;

    update_media_item(&mut tx, media_item_id, result).await?;

    if item_type == "movie" {
        update_movie_extension(&mut tx, media_item_id, result).await?;
    } else if item_type == "series" {
        update_series_extension(&mut tx, media_item_id, result).await?;
    }

    if !result.genres.is_empty() {
        upsert_genres(&mut tx, media_item_id, &result.genres).await?;
    }

    if let Some(ref credits) = result.credits {
        upsert_credits(&mut tx, media_item_id, credits).await?;
    }

    let metadata = build_metadata_json(result);
    sqlx::query(
        "UPDATE media_items SET metadata = COALESCE(metadata, '{}') || $2::jsonb WHERE id = $1",
    )
    .bind(media_item_id)
    .bind(&metadata)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    Ok(())
}

async fn update_media_item(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    media_item_id: Uuid,
    result: &EnrichmentResult,
) -> Result<(), sqlx::Error> {
    let year: Option<i32> = result
        .release_date
        .as_ref()
        .and_then(|d| d.get(..4))
        .and_then(|y| y.parse().ok());

    let runtime_seconds: Option<i32> = result.runtime.map(|r| (r * 60) as i32);

    let mut query_builder = sqlx::QueryBuilder::new(
        "UPDATE media_items SET updated_at = now(), match_state = 'confirmed'",
    );

    query_builder.push(", overview = ");
    if let Some(ref overview) = result.overview {
        query_builder.push_bind(overview.clone());
    } else {
        query_builder.push("NULL");
    }

    if let Some(ref title) = result.title {
        query_builder.push(", original_title = ");
        query_builder.push_bind(title.clone());
    }

    if year.is_some() {
        query_builder.push(", premiere_date = TO_DATE(");
        query_builder.push_bind(result.release_date.clone().unwrap_or_default());
        query_builder.push(", 'YYYY-MM-DD')");
    }

    if let Some(runtime) = runtime_seconds {
        query_builder.push(", runtime_seconds = ");
        query_builder.push_bind(runtime);
    }

    if let Some(vote) = result.vote_average {
        query_builder.push(", rating_average = ");
        query_builder.push_bind(vote as f32);
    }

    if let Some(tmdb) = result.tmdb_id {
        query_builder.push(", tmdb_id = ");
        query_builder.push_bind(tmdb as i64);
    }

    if let Some(ref imdb) = result.imdb_id {
        query_builder.push(", imdb_id = ");
        query_builder.push_bind(imdb.clone());
    }

    if let Some(tvdb) = result.tvdb_id {
        query_builder.push(", tvdb_id = ");
        query_builder.push_bind(tvdb as i64);
    }

    query_builder.push(" WHERE id = ");
    query_builder.push_bind(media_item_id);

    query_builder.build().execute(&mut **tx).await?;

    Ok(())
}

async fn update_movie_extension(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    media_item_id: Uuid,
    result: &EnrichmentResult,
) -> Result<(), sqlx::Error> {
    let mut query_builder =
        sqlx::QueryBuilder::new("UPDATE movies SET updated_at = now()");

    if let Some(ref tagline) = result.tagline {
        query_builder.push(", metadata = COALESCE(metadata, '{}') || ");
        query_builder.push_bind(serde_json::json!({ "tagline": tagline }));
    }

    if let Some(rated) = result
        .ratings
        .as_ref()
        .and_then(|r| r.rated.clone())
    {
        query_builder.push(", metadata = COALESCE(metadata, '{}') || ");
        query_builder.push_bind(serde_json::json!({ "certification": rated }));
    }

    if !result.production_companies.is_empty() {
        let studios: Vec<String> = result.production_companies.iter().map(|c| c.name.clone()).collect();
        query_builder.push(", metadata = COALESCE(metadata, '{}') || ");
        query_builder.push_bind(serde_json::json!({ "studios": studios }));
    }

    query_builder.push(" WHERE id = ");
    query_builder.push_bind(media_item_id);

    query_builder.build().execute(&mut **tx).await?;

    Ok(())
}

async fn update_series_extension(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    media_item_id: Uuid,
    result: &EnrichmentResult,
) -> Result<(), sqlx::Error> {
    let mut query_builder =
        sqlx::QueryBuilder::new("UPDATE series SET updated_at = now()");

    if !result.networks.is_empty() {
        let network_names: Vec<String> = result.networks.iter().map(|n| n.name.clone()).collect();
        query_builder.push(", metadata = COALESCE(metadata, '{}') || ");
        query_builder.push_bind(serde_json::json!({ "networks": network_names }));
    }

    query_builder.push(" WHERE id = ");
    query_builder.push_bind(media_item_id);

    query_builder.build().execute(&mut **tx).await?;

    Ok(())
}

async fn upsert_genres(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    media_item_id: Uuid,
    genres: &[super::metadata::GenreEntry],
) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM media_genres WHERE media_item_id = $1")
        .bind(media_item_id)
        .execute(&mut **tx)
        .await?;

    for genre in genres {
        let slug = genre
            .name
            .to_lowercase()
            .replace(' ', "-")
            .replace(|c: char| !c.is_alphanumeric() && c != '-', "");

        let genre_id: Option<Uuid> = sqlx::query_scalar(
            "INSERT INTO genres (name, slug) VALUES ($1, $2) ON CONFLICT (name) DO UPDATE SET name = EXCLUDED.name RETURNING id",
        )
        .bind(&genre.name)
        .bind(&slug)
        .fetch_optional(&mut **tx)
        .await?;

        let genre_id = match genre_id {
            Some(id) => id,
            None => continue,
        };

        sqlx::query(
            "INSERT INTO media_genres (media_item_id, genre_id) VALUES ($1, $2) ON CONFLICT DO NOTHING",
        )
        .bind(media_item_id)
        .bind(genre_id)
        .execute(&mut **tx)
        .await?;
    }

    Ok(())
}

async fn upsert_credits(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    media_item_id: Uuid,
    credits: &CreditsData,
) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM media_credits WHERE media_item_id = $1")
        .bind(media_item_id)
        .execute(&mut **tx)
        .await?;

    let top_cast: Vec<&CastEntry> = credits.cast.iter().take(20).collect();
    for entry in top_cast {
        let person_id = upsert_person(
            &mut *tx,
            entry.id,
            &entry.name,
            entry.profile_path.as_deref(),
        )
        .await?;

        sqlx::query(
            r#"INSERT INTO media_credits (media_item_id, person_id, credit_type, role, "order")
               VALUES ($1, $2, 'cast', $3, $4)
               ON CONFLICT DO NOTHING"#,
        )
        .bind(media_item_id)
        .bind(person_id)
        .bind(entry.character.as_deref().unwrap_or("Unknown"))
        .bind(entry.order.unwrap_or(0) as i32)
        .execute(&mut **tx)
        .await?;
    }

    let key_crew: Vec<&CrewEntry> = credits
        .crew
        .iter()
        .filter(|c| {
            matches!(
                c.job.as_deref(),
                Some("Director") | Some("Writer") | Some("Creator") | Some("Executive Producer")
            )
        })
        .take(10)
        .collect();

    for entry in key_crew {
        let person_id = upsert_person(
            &mut *tx,
            entry.id,
            &entry.name,
            entry.profile_path.as_deref(),
        )
        .await?;

        sqlx::query(
            r#"INSERT INTO media_credits (media_item_id, person_id, credit_type, role, department, "order")
               VALUES ($1, $2, 'crew', $3, $4, 0)
               ON CONFLICT DO NOTHING"#,
        )
        .bind(media_item_id)
        .bind(person_id)
        .bind(entry.job.as_deref().unwrap_or("Unknown"))
        .bind(entry.department.as_deref().unwrap_or("Unknown"))
        .execute(&mut **tx)
        .await?;
    }

    Ok(())
}

async fn upsert_person(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    tmdb_person_id: u64,
    name: &str,
    profile_path: Option<&str>,
) -> Result<Uuid, sqlx::Error> {
    let sort_name = name.to_lowercase();
    let image_url = profile_path.map(|p| format!("https://image.tmdb.org/t/p/original{p}"));

    let id: Uuid = sqlx::query_scalar(
        r#"INSERT INTO people (name, sort_name, tmdb_person_id, image_url)
           VALUES ($1, $2, $3, $4)
           ON CONFLICT (tmdb_person_id) WHERE tmdb_person_id IS NOT NULL
           DO UPDATE SET name = EXCLUDED.name, sort_name = EXCLUDED.sort_name,
                         image_url = COALESCE(EXCLUDED.image_url, people.image_url),
                         updated_at = now()
           RETURNING id"#,
    )
    .bind(name)
    .bind(&sort_name)
    .bind(tmdb_person_id as i64)
    .bind(&image_url)
    .fetch_one(&mut **tx)
    .await?;

    Ok(id)
}

fn build_metadata_json(result: &EnrichmentResult) -> serde_json::Value {
    let mut obj = serde_json::Map::new();

    if let Some(ref tagline) = result.tagline {
        obj.insert("tagline".to_string(), serde_json::Value::String(tagline.clone()));
    }

    if !result.videos.is_empty() {
        let videos: Vec<serde_json::Value> = result
            .videos
            .iter()
            .map(|v| {
                serde_json::json!({
                    "key": v.key,
                    "name": v.name,
                    "site": v.site,
                    "type": v.video_type,
                    "official": v.official,
                })
            })
            .collect();
        obj.insert("videos".to_string(), serde_json::Value::Array(videos));
    }

    if let Some(ref ratings) = result.ratings {
        if let Some(ref rt) = ratings.rotten_tomatoes {
            obj.insert(
                "rotten_tomatoes".to_string(),
                serde_json::Value::String(rt.clone()),
            );
        }
        if let Some(ref mc) = ratings.metacritic {
            obj.insert(
                "metacritic".to_string(),
                serde_json::Value::String(mc.clone()),
            );
        }
        if let Some(imdb) = ratings.imdb_rating {
            obj.insert("imdb_rating".to_string(), serde_json::json!(imdb));
        }
    }

    serde_json::Value::Object(obj)
}

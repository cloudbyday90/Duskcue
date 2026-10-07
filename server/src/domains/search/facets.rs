use sqlx::Row;

use crate::domains::profiles::types::ProfileScope;

use super::query::matching_query;
use super::types::{SearchFacetCount, SearchFacets, SearchParams};

const FACETS_SQL: &str = r#"
SELECT 'types' AS facet, type AS value, type AS label, count(*)::bigint AS count
FROM matched GROUP BY type
UNION ALL
SELECT 'genres', g.slug, g.name, count(*)::bigint
FROM matched mi JOIN media_genres mg ON mg.media_item_id = mi.id JOIN genres g ON g.id = mg.genre_id
GROUP BY g.slug, g.name
UNION ALL
SELECT 'years', search_year::text, search_year::text, count(*)::bigint
FROM matched WHERE search_year IS NOT NULL GROUP BY search_year
UNION ALL
SELECT 'ratings', bucket::text, bucket::text || '+', count(*)::bigint
FROM matched CROSS JOIN unnest(ARRAY[9, 8, 7, 6]) bucket WHERE rating_average >= bucket GROUP BY bucket
ORDER BY facet, count DESC, value ASC
"#;

pub(super) async fn load(
    pool: &sqlx::PgPool,
    scope: &ProfileScope,
    params: &SearchParams,
) -> Result<SearchFacets, sqlx::Error> {
    let mut sql = matching_query(scope, params);
    let rows = sql.push(FACETS_SQL).build().fetch_all(pool).await?;
    let mut facets = SearchFacets::default();
    for row in rows {
        let item = SearchFacetCount {
            value: row.get("value"),
            label: row.get("label"),
            count: row.get("count"),
        };
        match row.get::<&str, _>("facet") {
            "types" => facets.types.push(item),
            "genres" => facets.genres.push(item),
            "years" => facets.years.push(item),
            "ratings" => facets.ratings.push(item),
            _ => unreachable!(),
        }
    }
    facets.years.sort_by(|a, b| b.value.cmp(&a.value));
    facets.ratings.sort_by(|a, b| b.value.cmp(&a.value));
    Ok(facets)
}

use super::service::row_to_response;
use super::types::{MediaAvailabilityResponse, MediaBrowseItemResponse, MediaWatchStateResponse};
use crate::domains::profiles::types::ProfileScope;
use sqlx::{Postgres, QueryBuilder, Row};

pub(crate) const BROWSE_SELECT_SQL: &str = r#"SELECT mi.*, series.status AS series_status,
       COALESCE(ep.series_id, sn.series_id) AS series_id, sn.id AS season_id, sn.season_number,
       ep.episode_number, ep.absolute_episode_number, series_mi.title AS series_title,
       files.file_count, files.healthy_file_count,
       COALESCE(files.healthy_runtime_seconds, NULLIF(GREATEST(mi.runtime_seconds, 0), 0)) AS browse_runtime_seconds,
       COALESCE(uid.is_watched, false) AS is_watched, COALESCE(uid.is_favorite, false) AS is_favorite,
       COALESCE(uid.resume_position_ms, 0) AS resume_position_ms, COALESCE(uid.play_count, 0) AS play_count,
       uid.last_played_at, children.episode_count, children.available_episode_count, children.watched_episode_count
FROM allowed_media mi
LEFT JOIN episodes ep ON ep.id = mi.id
LEFT JOIN seasons sn ON sn.id = CASE WHEN mi.type = 'season' THEN mi.id ELSE ep.season_id END
LEFT JOIN allowed_media series_mi ON series_mi.id = COALESCE(ep.series_id, sn.series_id)
LEFT JOIN series ON series.id = CASE WHEN mi.type = 'series' THEN mi.id ELSE series_mi.id END
LEFT JOIN LATERAL (
    SELECT count(*) AS file_count, count(*) FILTER (WHERE mf.is_healthy) AS healthy_file_count,
           max(mf.runtime_seconds) FILTER (WHERE mf.is_healthy AND mf.runtime_seconds > 0) AS healthy_runtime_seconds
    FROM media_files mf WHERE mf.media_item_id = mi.id
) files ON true
LEFT JOIN user_item_data uid ON uid.media_item_id = mi.id AND uid.profile_id = "#;

pub(crate) fn push_browse_children(sql: &mut QueryBuilder<Postgres>, scope: &ProfileScope) {
    sql.push(r#" LEFT JOIN LATERAL (
        SELECT count(*) AS episode_count,
               count(*) FILTER (WHERE EXISTS (SELECT 1 FROM media_files child_file WHERE child_file.media_item_id = child.id AND child_file.is_healthy)) AS available_episode_count,
               count(*) FILTER (WHERE child_watch.is_watched) AS watched_episode_count
        FROM allowed_media child JOIN episodes child_episode ON child_episode.id = child.id
        LEFT JOIN user_item_data child_watch ON child_watch.media_item_id = child.id AND child_watch.profile_id = "#)
        .push_bind(scope.profile_id).push(" AND child_watch.user_id = ").push_bind(scope.owner_user_id)
        .push(" WHERE mi.type = 'season' AND child_episode.season_id = mi.id) children ON true WHERE true");
}

pub(crate) fn browse_row_to_response(row: &sqlx::postgres::PgRow) -> MediaBrowseItemResponse {
    let media = row_to_response(row);
    let healthy_file_count: i64 = row.get("healthy_file_count");
    let is_season = media.r#type == "season";
    MediaBrowseItemResponse {
        duration_ms: row
            .get::<Option<i32>, _>("browse_runtime_seconds")
            .map(|seconds| i64::from(seconds) * 1000),
        availability: MediaAvailabilityResponse {
            can_play: matches!(media.r#type.as_str(), "movie" | "episode")
                && healthy_file_count > 0,
            healthy_file_count,
        },
        watch_state: MediaWatchStateResponse {
            is_watched: row.get("is_watched"),
            is_favorite: row.get("is_favorite"),
            resume_position_ms: i64::from(row.get::<i32, _>("resume_position_ms")).max(0),
            play_count: row.get("play_count"),
            last_played_at: row.get("last_played_at"),
        },
        series_title: row.get("series_title"),
        episode_count: is_season.then(|| row.get("episode_count")),
        available_episode_count: is_season.then(|| row.get("available_episode_count")),
        watched_episode_count: is_season.then(|| row.get("watched_episode_count")),
        media,
    }
}

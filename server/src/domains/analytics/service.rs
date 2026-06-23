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

#![allow(unused_variables)]

use base64::Engine;
use chrono::{DateTime, Duration, Utc};
use sqlx::{Row, PgPool};
use uuid::Uuid;

use crate::domains::analytics::error::AnalyticsError;
use crate::domains::analytics::types::*;

const OVERVIEW_SQL: &str = r#"SELECT
            COUNT(*) AS total_plays,
            COUNT(DISTINCT user_id) AS unique_users,
            COALESCE(SUM(duration_seconds), 0)::bigint AS total_watch_time_seconds,
            COUNT(*) FILTER (WHERE stream_decision = 'direct_play') AS direct_play_count,
            COUNT(*) FILTER (WHERE stream_decision = 'direct_stream') AS direct_stream_count,
            COUNT(*) FILTER (WHERE stream_decision = 'transcode') AS transcode_count
        FROM play_sessions
        WHERE ($1::timestamptz IS NULL OR started_at >= $1)
          AND started_at <= $2
          AND ($3::uuid IS NULL OR user_id = $3)
          AND ($4::uuid IS NULL OR library_id = $4)"#;

const CONCURRENT_COUNT_SQL: &str = r#"SELECT COUNT(*)::bigint AS concurrent
        FROM play_sessions
        WHERE stopped_at IS NULL
          AND started_at > now() - interval '24 hours'"#;

const PLAY_HISTORY_SQL: &str = r#"SELECT
            ps.id, ps.user_id, ps.media_item_id, ps.library_id,
            ps.started_at, ps.stopped_at, ps.duration_seconds,
            ps.location_type, ps.geo_city, ps.geo_country,
            ps.client_name, ps.client_device,
            ps.stream_decision, ps.percent_complete::float8 AS percent_complete,
            ps.bandwidth_bps,
            COALESCE(u.display_name, 'Unknown') AS user_display_name,
            COALESCE(mi.title, 'Unknown') AS media_title,
            COALESCE(mi.type, 'movie') AS media_type
        FROM play_sessions ps
        LEFT JOIN users u ON u.id = ps.user_id
        LEFT JOIN media_items mi ON mi.id = ps.media_item_id
        WHERE ($1::timestamptz IS NULL OR ps.started_at >= $1)
          AND ps.started_at <= $2
          AND ($3::uuid IS NULL OR ps.user_id = $3)
          AND ($4::uuid IS NULL OR ps.library_id = $4)
          AND ($5::text IS NULL OR ps.stream_decision = $5)
          AND ($6::uuid IS NULL OR ps.id < $6)
        ORDER BY ps.id DESC
        LIMIT $7"#;

const TOP_MEDIA_BY_PLAY_COUNT_SQL: &str = r#"SELECT
            mi.id AS media_item_id,
            mi.title,
            mi.type AS media_type,
            mi.library_id,
            COUNT(*)::bigint AS play_count,
            COALESCE(SUM(ps.duration_seconds), 0)::bigint AS total_watch_time_seconds,
            COUNT(DISTINCT ps.user_id)::bigint AS unique_users
        FROM play_sessions ps
        JOIN media_items mi ON mi.id = ps.media_item_id
        WHERE ($1::timestamptz IS NULL OR ps.started_at >= $1)
          AND ps.started_at <= $2
          AND ($3::uuid IS NULL OR ps.library_id = $3)
        GROUP BY mi.id
        ORDER BY play_count DESC
        LIMIT $4"#;

const TOP_MEDIA_BY_WATCH_TIME_SQL: &str = r#"SELECT
            mi.id AS media_item_id,
            mi.title,
            mi.type AS media_type,
            mi.library_id,
            COUNT(*)::bigint AS play_count,
            COALESCE(SUM(ps.duration_seconds), 0)::bigint AS total_watch_time_seconds,
            COUNT(DISTINCT ps.user_id)::bigint AS unique_users
        FROM play_sessions ps
        JOIN media_items mi ON mi.id = ps.media_item_id
        WHERE ($1::timestamptz IS NULL OR ps.started_at >= $1)
          AND ps.started_at <= $2
          AND ($3::uuid IS NULL OR ps.library_id = $3)
        GROUP BY mi.id
        ORDER BY total_watch_time_seconds DESC
        LIMIT $4"#;

const BANDWIDTH_SQL: &str = r#"WITH buckets AS (
            SELECT generate_series($2, $3, $1::interval) AS bucket_start
        ),
        agg AS (
            SELECT date_bin($1::interval, started_at, $2) AS bucket_start,
                   COALESCE(SUM(bandwidth_bps), 0)::bigint AS bandwidth_bps,
                   COUNT(*)::bigint AS session_count
            FROM play_sessions
            WHERE started_at >= $2 AND started_at <= $3
              AND ($4::uuid IS NULL OR user_id = $4)
              AND ($5::uuid IS NULL OR library_id = $5)
            GROUP BY 1
        )
        SELECT
            b.bucket_start AS "timestamp",
            COALESCE(a.bandwidth_bps, 0)::bigint AS bandwidth_bps,
            COALESCE(a.session_count, 0)::bigint AS session_count
        FROM buckets b
        LEFT JOIN agg a ON a.bucket_start = b.bucket_start
        ORDER BY b.bucket_start"#;

const CONCURRENT_STREAMS_SQL: &str = r#"SELECT
            ps.id, ps.user_id, ps.media_item_id, ps.started_at,
            ps.stream_decision, ps.client_name, ps.client_device, ps.bandwidth_bps,
            COALESCE(u.display_name, 'Unknown') AS user_display_name,
            COALESCE(mi.title, 'Unknown') AS media_title
        FROM play_sessions ps
        LEFT JOIN users u ON u.id = ps.user_id
        LEFT JOIN media_items mi ON mi.id = ps.media_item_id
        WHERE ps.stopped_at IS NULL
          AND ps.started_at > now() - interval '24 hours'
        ORDER BY ps.started_at DESC"#;

fn resolve_time_range(
    range: Option<&str>,
    from: Option<DateTime<Utc>>,
    to: Option<DateTime<Utc>>,
) -> Result<(Option<DateTime<Utc>>, DateTime<Utc>), AnalyticsError> {
    let to = to.unwrap_or_else(Utc::now);
    let from = match from {
        Some(f) => {
            if f > to {
                return Err(AnalyticsError::InvalidDateRange(
                    "'from' must not be later than 'to'".to_string(),
                ));
            }
            Some(f)
        }
        None => {
            let preset = range.unwrap_or("7d");
            match preset {
                "24h" => Some(to - Duration::hours(24)),
                "7d" => Some(to - Duration::days(7)),
                "30d" => Some(to - Duration::days(30)),
                "90d" => Some(to - Duration::days(90)),
                "all" => None,
                other => {
                    return Err(AnalyticsError::InvalidTimePreset(other.to_string()));
                }
            }
        }
    };
    Ok((from, to))
}

fn resolve_bucket_interval(from: DateTime<Utc>, to: DateTime<Utc>) -> &'static str {
    let span_seconds = (to - from).num_seconds();
    if span_seconds <= 86_400 {
        "1 hour"
    } else if span_seconds <= 7 * 86_400 {
        "6 hours"
    } else {
        "1 day"
    }
}

fn encode_cursor(id: Uuid) -> String {
    let json = serde_json::json!({ "id": id.to_string() });
    base64::engine::general_purpose::STANDARD
        .encode(serde_json::to_vec(&json).unwrap_or_default())
}

fn parse_cursor(cursor: Option<&str>) -> Option<Uuid> {
    cursor.and_then(|c| {
        let bytes = base64::engine::general_purpose::STANDARD.decode(c).ok()?;
        let json: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
        json.get("id")?.as_str().and_then(|s| s.parse::<Uuid>().ok())
    })
}

pub async fn get_analytics_overview(
    pool: &PgPool,
    query: &AnalyticsQuery,
) -> Result<AnalyticsOverviewResponse, AnalyticsError> {
    let (from, to) = resolve_time_range(query.range.as_deref(), query.from, query.to)?;

    let row = sqlx::query(OVERVIEW_SQL)
        .bind(from)
        .bind(to)
        .bind(query.user_id)
        .bind(query.library_id)
        .fetch_one(pool)
        .await?;

    let concurrent: i64 = sqlx::query_scalar(CONCURRENT_COUNT_SQL)
        .fetch_one(pool)
        .await
        .unwrap_or(0);

    Ok(AnalyticsOverviewResponse {
        total_plays: row.try_get("total_plays").unwrap_or(0),
        unique_users: row.try_get("unique_users").unwrap_or(0),
        total_watch_time_seconds: row.try_get("total_watch_time_seconds").unwrap_or(0),
        concurrent_streams: concurrent,
        direct_play_count: row.try_get("direct_play_count").unwrap_or(0),
        direct_stream_count: row.try_get("direct_stream_count").unwrap_or(0),
        transcode_count: row.try_get("transcode_count").unwrap_or(0),
        range_start: from,
        range_end: to,
    })
}

pub async fn list_play_history(
    pool: &PgPool,
    query: &PlayHistoryQuery,
) -> Result<PlayHistoryResponse, AnalyticsError> {
    let (from, to) = resolve_time_range(query.range.as_deref(), query.from, query.to)?;
    let cursor_id = parse_cursor(query.cursor.as_deref());
    let limit = query.limit.unwrap_or(20).clamp(1, 100);
    let fetch_limit = (limit + 1) as i64;

    let rows = sqlx::query(PLAY_HISTORY_SQL)
        .bind(from)
        .bind(to)
        .bind(query.user_id)
        .bind(query.library_id)
        .bind(query.stream_decision.as_deref())
        .bind(cursor_id)
        .bind(fetch_limit)
        .fetch_all(pool)
        .await?;

    let has_more = rows.len() > limit as usize;
    let rows = if has_more { &rows[..limit as usize] } else { &rows };

    let items: Vec<PlaySessionResponse> = rows.iter().map(row_to_play_session_response).collect();

    let next_cursor = if has_more {
        items.last().map(|i| encode_cursor(i.id))
    } else {
        None
    };

    Ok(PlayHistoryResponse {
        items,
        has_more,
        next_cursor,
    })
}

pub async fn get_top_media(
    pool: &PgPool,
    query: &TopMediaQuery,
) -> Result<TopMediaResponse, AnalyticsError> {
    let (from, to) = resolve_time_range(query.range.as_deref(), query.from, query.to)?;
    let sort_by = query.sort_by.as_deref().unwrap_or("play_count");
    let limit = query.limit.unwrap_or(10).clamp(1, 100) as i64;

    let sql = match sort_by {
        "watch_time" => TOP_MEDIA_BY_WATCH_TIME_SQL,
        _ => TOP_MEDIA_BY_PLAY_COUNT_SQL,
    };

    let rows = sqlx::query(sql)
        .bind(from)
        .bind(to)
        .bind(query.library_id)
        .bind(limit)
        .fetch_all(pool)
        .await?;

    let items: Vec<TopMediaItem> = rows
        .iter()
        .map(|r| TopMediaItem {
            media_item_id: r.get("media_item_id"),
            title: r.get("title"),
            media_type: r.get("media_type"),
            library_id: r.get("library_id"),
            play_count: r.try_get("play_count").unwrap_or(0),
            total_watch_time_seconds: r.try_get("total_watch_time_seconds").unwrap_or(0),
            unique_users: r.try_get("unique_users").unwrap_or(0),
        })
        .collect();

    let resolved_sort = match sort_by {
        "watch_time" => "watch_time".to_string(),
        _ => "play_count".to_string(),
    };

    Ok(TopMediaResponse {
        items,
        sort_by: resolved_sort,
    })
}

pub async fn get_bandwidth_usage(
    pool: &PgPool,
    query: &AnalyticsQuery,
) -> Result<BandwidthResponse, AnalyticsError> {
    let (from_opt, to) = resolve_time_range(query.range.as_deref(), query.from, query.to)?;
    let from = from_opt.unwrap_or(to - Duration::days(90));
    let bucket_interval = resolve_bucket_interval(from, to);

    let rows = sqlx::query(BANDWIDTH_SQL)
        .bind(bucket_interval)
        .bind(from)
        .bind(to)
        .bind(query.user_id)
        .bind(query.library_id)
        .fetch_all(pool)
        .await?;

    let points: Vec<BandwidthPoint> = rows
        .iter()
        .map(|r| BandwidthPoint {
            timestamp: r.get("timestamp"),
            bandwidth_bps: r.try_get("bandwidth_bps").unwrap_or(0),
            session_count: r.try_get("session_count").unwrap_or(0),
        })
        .collect();

    let peak_bandwidth_bps = points.iter().map(|p| p.bandwidth_bps).max().unwrap_or(0);
    let average_bandwidth_bps = if points.is_empty() {
        0
    } else {
        points.iter().map(|p| p.bandwidth_bps).sum::<i64>() / points.len() as i64
    };

    Ok(BandwidthResponse {
        points,
        peak_bandwidth_bps,
        average_bandwidth_bps,
    })
}

pub async fn get_concurrent_streams(
    pool: &PgPool,
) -> Result<ConcurrentStreamsResponse, AnalyticsError> {
    let rows = sqlx::query(CONCURRENT_STREAMS_SQL).fetch_all(pool).await?;

    let count = rows.len() as i64;
    let streams: Vec<ConcurrentStreamInfo> = rows
        .iter()
        .map(|r| ConcurrentStreamInfo {
            session_id: r.get("id"),
            user_id: r.get("user_id"),
            user_display_name: r.get("user_display_name"),
            media_item_id: r.get("media_item_id"),
            media_title: r.get("media_title"),
            started_at: r.get("started_at"),
            stream_decision: r.get("stream_decision"),
            client_name: r.get("client_name"),
            client_device: r.get("client_device"),
            bandwidth_bps: r.get("bandwidth_bps"),
        })
        .collect();

    Ok(ConcurrentStreamsResponse { count, streams })
}

fn row_to_play_session_response(row: &sqlx::postgres::PgRow) -> PlaySessionResponse {
    PlaySessionResponse {
        id: row.get("id"),
        user_id: row.get("user_id"),
        user_display_name: row.get("user_display_name"),
        media_item_id: row.get("media_item_id"),
        media_title: row.get("media_title"),
        media_type: row.get("media_type"),
        library_id: row.get("library_id"),
        started_at: row.get("started_at"),
        stopped_at: row.get("stopped_at"),
        duration_seconds: row.get("duration_seconds"),
        location_type: row.get("location_type"),
        geo_city: row.get("geo_city"),
        geo_country: row.get("geo_country"),
        client_name: row.get("client_name"),
        client_device: row.get("client_device"),
        stream_decision: row.get("stream_decision"),
        percent_complete: row.get("percent_complete"),
        bandwidth_bps: row.get("bandwidth_bps"),
    }
}

pub async fn list_trust_scores(pool: &PgPool) -> Result<TrustScoreListResponse, AnalyticsError> {
    todo!()
}

pub async fn list_trust_events(
    pool: &PgPool,
    query: &TrustEventQuery,
) -> Result<TrustEventListResponse, AnalyticsError> {
    todo!()
}

pub async fn acknowledge_trust_event(
    pool: &PgPool,
    event_id: uuid::Uuid,
    acknowledger_user_id: uuid::Uuid,
) -> Result<AcknowledgeEventResponse, AnalyticsError> {
    todo!()
}

pub async fn get_geoip_status(
    pool: &PgPool,
    data_dir: &std::path::Path,
) -> Result<GeoIpStatusResponse, AnalyticsError> {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_time_range_default_is_7d() {
        let (from, to) = resolve_time_range(None, None, None).unwrap();
        assert!(from.is_some());
        let span = to - from.unwrap();
        assert!((span.num_days() - 7).abs() <= 1);
    }

    #[test]
    fn resolve_time_range_24h() {
        let (from, to) = resolve_time_range(Some("24h"), None, None).unwrap();
        let span = to - from.unwrap();
        assert!((span.num_hours() - 24).abs() <= 1);
    }

    #[test]
    fn resolve_time_range_all_is_unbounded() {
        let (from, _to) = resolve_time_range(Some("all"), None, None).unwrap();
        assert!(from.is_none());
    }

    #[test]
    fn resolve_time_range_explicit_from_to() {
        let from = Utc::now() - Duration::days(3);
        let to = Utc::now();
        let (resolved_from, resolved_to) = resolve_time_range(None, Some(from), Some(to)).unwrap();
        assert_eq!(resolved_from, Some(from));
        assert_eq!(resolved_to, to);
    }

    #[test]
    fn resolve_time_range_rejects_from_after_to() {
        let from = Utc::now() + Duration::days(1);
        let to = Utc::now();
        let err = resolve_time_range(None, Some(from), Some(to)).unwrap_err();
        assert!(matches!(err, AnalyticsError::InvalidDateRange(_)));
    }

    #[test]
    fn resolve_time_range_rejects_bad_preset() {
        let err = resolve_time_range(Some("999d"), None, None).unwrap_err();
        assert!(matches!(err, AnalyticsError::InvalidTimePreset(_)));
    }

    #[test]
    fn resolve_time_range_from_takes_precedence_over_range() {
        let from = Utc::now() - Duration::days(40);
        let to = Utc::now();
        let (resolved_from, _) = resolve_time_range(Some("24h"), Some(from), Some(to)).unwrap();
        assert_eq!(resolved_from, Some(from));
    }

    #[test]
    fn resolve_bucket_interval_short_range_hourly() {
        let from = Utc::now() - Duration::hours(20);
        let to = Utc::now();
        assert_eq!(resolve_bucket_interval(from, to), "1 hour");
    }

    #[test]
    fn resolve_bucket_interval_week_range_six_hourly() {
        let from = Utc::now() - Duration::days(5);
        let to = Utc::now();
        assert_eq!(resolve_bucket_interval(from, to), "6 hours");
    }

    #[test]
    fn resolve_bucket_interval_long_range_daily() {
        let from = Utc::now() - Duration::days(30);
        let to = Utc::now();
        assert_eq!(resolve_bucket_interval(from, to), "1 day");
    }

    #[test]
    fn cursor_roundtrip() {
        let id = Uuid::new_v4();
        let encoded = encode_cursor(id);
        let decoded = parse_cursor(Some(&encoded));
        assert_eq!(decoded, Some(id));
    }

    #[test]
    fn cursor_rejects_garbage() {
        assert_eq!(parse_cursor(Some("not-base64!!")), None);
        assert_eq!(parse_cursor(None), None);
    }

    #[test]
    fn cursor_rejects_missing_id_field() {
        let json = serde_json::json!({ "other": "x" });
        let encoded = base64::engine::general_purpose::STANDARD
            .encode(serde_json::to_vec(&json).unwrap());
        assert_eq!(parse_cursor(Some(&encoded)), None);
    }
}

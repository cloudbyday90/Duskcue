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

use std::net::IpAddr;

use base64::Engine;
use chrono::{DateTime, Duration, Utc};
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::domains::analytics::error::AnalyticsError;
use crate::domains::analytics::types::*;
use crate::services::geoip::{self, GeoLocation, LocationType};
use crate::state::{AnalyticsConfig, AppState};

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
    let rows = sqlx::query(
        "SELECT ts.user_id, ts.score, ts.total_violations, ts.last_violation_at, \
                ts.last_good_session_at, ts.updated_at, \
                COALESCE(u.display_name, 'Unknown') AS user_display_name \
         FROM user_trust_scores ts \
         LEFT JOIN users u ON u.id = ts.user_id \
         ORDER BY ts.score ASC, ts.last_violation_at DESC NULLS LAST",
    )
    .fetch_all(pool)
    .await?;

    let items: Vec<TrustScoreResponse> = rows
        .iter()
        .map(|r| TrustScoreResponse {
            user_id: r.get("user_id"),
            user_display_name: r.get("user_display_name"),
            score: r.get("score"),
            total_violations: r.get("total_violations"),
            last_violation_at: r.get("last_violation_at"),
            last_good_session_at: r.get("last_good_session_at"),
            updated_at: r.get("updated_at"),
        })
        .collect();

    Ok(TrustScoreListResponse { items })
}

pub async fn list_trust_events(
    pool: &PgPool,
    query: &TrustEventQuery,
) -> Result<TrustEventListResponse, AnalyticsError> {
    let page = query.page.unwrap_or(1).max(1);
    let page_size = query.page_size.unwrap_or(25).clamp(1, 100);
    let offset = ((page - 1) * page_size) as i64;

    let rows = sqlx::query(
        "SELECT te.id, te.user_id, te.play_session_id, te.rule_type, te.severity, \
                te.score_impact, te.details, te.acknowledged, te.acknowledged_at, te.created_at, \
                COALESCE(u.display_name, 'Unknown') AS user_display_name \
         FROM user_trust_events te \
         LEFT JOIN users u ON u.id = te.user_id \
         WHERE ($1::uuid IS NULL OR te.user_id = $1) \
           AND ($2::text IS NULL OR te.severity = $2) \
           AND ($3::bool IS NULL OR te.acknowledged = $3) \
         ORDER BY te.created_at DESC \
         LIMIT $4 OFFSET $5",
    )
    .bind(query.user_id)
    .bind(query.severity.as_deref())
    .bind(query.acknowledged)
    .bind(page_size as i64)
    .bind(offset)
    .fetch_all(pool)
    .await?;

    let total: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM user_trust_events te \
         WHERE ($1::uuid IS NULL OR te.user_id = $1) \
           AND ($2::text IS NULL OR te.severity = $2) \
           AND ($3::bool IS NULL OR te.acknowledged = $3)",
    )
    .bind(query.user_id)
    .bind(query.severity.as_deref())
    .bind(query.acknowledged)
    .fetch_one(pool)
    .await?;

    let items: Vec<TrustEventResponse> = rows
        .iter()
        .map(|r| TrustEventResponse {
            id: r.get("id"),
            user_id: r.get("user_id"),
            user_display_name: r.get("user_display_name"),
            play_session_id: r.get("play_session_id"),
            rule_type: r.get("rule_type"),
            severity: r.get("severity"),
            score_impact: r.get("score_impact"),
            details: r.get("details"),
            acknowledged: r.get("acknowledged"),
            acknowledged_at: r.get("acknowledged_at"),
            created_at: r.get("created_at"),
        })
        .collect();

    let total_pages = (total as u32).div_ceil(page_size);

    Ok(TrustEventListResponse {
        items,
        total,
        page,
        page_size,
        total_pages,
    })
}

pub async fn acknowledge_trust_event(
    pool: &PgPool,
    event_id: uuid::Uuid,
    _acknowledger_user_id: uuid::Uuid,
) -> Result<AcknowledgeEventResponse, AnalyticsError> {
    let row = sqlx::query(
        "UPDATE user_trust_events \
         SET acknowledged = true, acknowledged_at = now() \
         WHERE id = $1 \
         RETURNING id, acknowledged_at",
    )
    .bind(event_id)
    .fetch_optional(pool)
    .await?
    .ok_or(AnalyticsError::TrustEventNotFound)?;

    Ok(AcknowledgeEventResponse {
        event_id: row.get("id"),
        acknowledged: true,
        acknowledged_at: row.get("acknowledged_at"),
    })
}

pub async fn get_geoip_status(
    geoip: &crate::services::geoip::GeoIpService,
    enabled: bool,
) -> Result<GeoIpStatusResponse, AnalyticsError> {
    let status = geoip.status();
    Ok(GeoIpStatusResponse {
        enabled,
        database_present: status.present_on_disk,
        database_path: if status.path.as_os_str().is_empty() {
            None
        } else {
            Some(status.path.to_string_lossy().into_owned())
        },
        database_age_days: status.age_days,
        database_size_bytes: status.size_bytes.map(|s| s as i64),
    })
}

// ---------------------------------------------------------------------------
// Trust Engine: Impossible Travel Detection
// -----------------------------------------------------------------------

/// Distance above which a jump is treated as intercontinental for severity
/// scoring. ~4000 km is roughly the width of a continent or a transatlantic
/// hop. Used when the destination country is new to the user.
const INTERCONTINENTAL_DISTANCE_KM: f64 = 4000.0;

/// Great-circle distance between two coordinate pairs using the Haversine
/// formula.
///
/// `d = 2r · arcsin(√(sin²((φ₂−φ₁)/2) + cos(φ₁)·cos(φ₂)·sin²((λ₂−λ₁)/2)))`
///
/// Where `r` = 6371 km (Earth's mean radius), `φ` = latitude, `λ` = longitude.
/// All angles are in degrees; conversion to radians is handled internally.
#[must_use]
pub fn haversine_distance(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    const EARTH_RADIUS_KM: f64 = 6371.0;
    let lat1_rad = lat1.to_radians();
    let lat2_rad = lat2.to_radians();
    let dlat = (lat2 - lat1).to_radians();
    let dlon = (lon2 - lon1).to_radians();
    let a = (dlat / 2.0).sin().powi(2)
        + lat1_rad.cos() * lat2_rad.cos() * (dlon / 2.0).sin().powi(2);
    EARTH_RADIUS_KM * 2.0 * a.sqrt().asin()
}

/// Implied travel speed in km/h given distance and elapsed time.
///
/// Returns `f64::INFINITY` when elapsed time is zero or negative (two sessions
/// with identical or inverted timestamps — a data artifact, not real travel).
#[must_use]
pub fn implied_velocity_kmh(distance_km: f64, elapsed_seconds: f64) -> f64 {
    if elapsed_seconds <= 0.0 {
        return f64::INFINITY;
    }
    distance_km / (elapsed_seconds / 3600.0)
}

/// Fire-and-forget entry point for play-session geo enrichment and impossible
/// travel detection.
///
/// Called via `tokio::spawn` after `start_playback` succeeds. All errors are
/// logged at `WARN` — enrichment failures never block playback or surface to
/// the API caller.
///
/// Performs three operations:
/// 1. Updates the `play_sessions` row with `ip_address`, `location_type`, and
///    `geo_*` columns derived from the GeoIP lookup.
/// 2. Upserts `user_location_history` for the detected country (powers the
///    90-day baseline suppression layer).
/// 3. Runs the 5-layer impossible-travel detection engine, creating a
///    `user_trust_events` row and decrementing the user's trust score if the
///    event survives all suppression layers.
pub async fn enrich_and_detect(
    state: &AppState,
    session_id: Uuid,
    user_id: Uuid,
    client_ip: Option<IpAddr>,
) {
    let config = state.runtime_config.load().analytics.clone();

    if !config.geoip_enabled {
        return;
    }

    let Some(ip) = client_ip else {
        return;
    };

    let location_type = geoip::classify_location(&ip, &[]);

    let geo = if location_type == LocationType::Wan {
        state.geoip.lookup(ip)
    } else {
        None
    };

    if let Err(e) = update_session_geo(&state.pool, session_id, ip, location_type, geo.as_ref()).await {
        tracing::warn!(session_id = %session_id, error = %e, "failed to update session geo data");
        return;
    }

    if let Some(ref g) = geo
        && let Some(ref country) = g.country_iso
        && let Err(e) = upsert_location_history(&state.pool, user_id, country).await
    {
        tracing::warn!(session_id = %session_id, error = %e, "failed to update location history");
    }

    if config.impossible_travel_enabled
        && location_type == LocationType::Wan
        && let Some(ref g) = geo
        && let Err(e) =
            detect_impossible_travel(&state.pool, &config, session_id, user_id, ip, g).await
    {
        tracing::warn!(session_id = %session_id, error = %e, "impossible travel detection error");
    }
}

async fn update_session_geo(
    pool: &PgPool,
    session_id: Uuid,
    ip: IpAddr,
    location_type: LocationType,
    geo: Option<&GeoLocation>,
) -> Result<(), sqlx::Error> {
    let (city, region, country, lat, lon) = match geo {
        Some(g) => (
            g.city.as_deref(),
            g.region.as_deref(),
            g.country_iso.as_deref(),
            g.latitude.map(|v| v as f32),
            g.longitude.map(|v| v as f32),
        ),
        None => (None, None, None, None, None),
    };

    sqlx::query(
        "UPDATE play_sessions \
         SET ip_address = $2::inet, location_type = $3, \
             geo_city = $4, geo_region = $5, geo_country = $6, \
             geo_lat = $7, geo_lon = $8, updated_at = now() \
         WHERE id = $1",
    )
    .bind(session_id)
    .bind(ip.to_string())
    .bind(location_type.as_db_str())
    .bind(city)
    .bind(region)
    .bind(country)
    .bind(lat)
    .bind(lon)
    .execute(pool)
    .await?;

    Ok(())
}

async fn upsert_location_history(
    pool: &PgPool,
    user_id: Uuid,
    country_code: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO user_location_history \
             (id, user_id, country_code, first_seen_at, last_seen_at, session_count) \
         VALUES (uuidv7(), $1, $2, now(), now(), 1) \
         ON CONFLICT (user_id, country_code) DO UPDATE \
         SET last_seen_at = now(), \
             session_count = user_location_history.session_count + 1, \
             updated_at = now()",
    )
    .bind(user_id)
    .bind(country_code)
    .execute(pool)
    .await?;
    Ok(())
}

/// The 5-layer impossible-travel detection engine.
///
/// Compares the current session's geo data against the user's most recent
/// prior WAN session within the lookback window. Applies suppression layers
/// in the order specified by ANALYTICS_SECURITY.md §Suppression Decision Flow.
/// If the event survives suppression and the implied velocity exceeds the
/// threshold, creates a trust event and decrements the trust score.
async fn detect_impossible_travel(
    pool: &PgPool,
    config: &AnalyticsConfig,
    session_id: Uuid,
    user_id: Uuid,
    current_ip: IpAddr,
    current_geo: &GeoLocation,
) -> Result<(), sqlx::Error> {
    let started_at: DateTime<Utc> =
        sqlx::query_scalar("SELECT started_at FROM play_sessions WHERE id = $1")
            .bind(session_id)
            .fetch_one(pool)
            .await?;

    let lookback_start = started_at - Duration::hours(config.lookback_hours as i64);

    let prev = sqlx::query(
        "SELECT started_at, ip_address, geo_city, geo_country, geo_lat, geo_lon, client_device \
         FROM play_sessions \
         WHERE user_id = $1 AND id != $2 \
           AND started_at >= $3 AND started_at < $4 \
           AND geo_lat IS NOT NULL AND geo_lon IS NOT NULL \
           AND location_type = 'wan' \
         ORDER BY started_at DESC \
         LIMIT 1",
    )
    .bind(user_id)
    .bind(session_id)
    .bind(lookback_start)
    .bind(started_at)
    .fetch_optional(pool)
    .await?;

    let Some(prev_row) = prev else {
        return Ok(());
    };

    let prev_lat: f32 = prev_row.try_get("geo_lat").unwrap_or(0.0);
    let prev_lon: f32 = prev_row.try_get("geo_lon").unwrap_or(0.0);
    let prev_country: Option<String> = prev_row.try_get("geo_country").ok();
    let prev_city: Option<String> = prev_row.try_get("geo_city").ok();
    let prev_device: Option<String> = prev_row.try_get("client_device").ok();
    let prev_started: DateTime<Utc> = prev_row.try_get("started_at").unwrap_or(started_at);
    let prev_ip_str: Option<String> = prev_row.try_get("ip_address").ok();
    let prev_ip = prev_ip_str.and_then(|s| s.parse::<IpAddr>().ok());

    let current_lat = current_geo.latitude.unwrap_or(0.0);
    let current_lon = current_geo.longitude.unwrap_or(0.0);
    let current_country = current_geo.country_iso.as_deref();

    // Layer 3: Same-country suppression
    if config.same_country_suppress
        && current_country.is_some()
        && current_country == prev_country.as_deref()
    {
        return Ok(());
    }

    // Layer 2: Trusted IP reduction
    let is_trusted = is_ip_trusted(&current_ip, &config.trusted_ips, &config.trusted_cidrs)
        || prev_ip
            .map(|ip| is_ip_trusted(&ip, &config.trusted_ips, &config.trusted_cidrs))
            .unwrap_or(false);

    // Distance and velocity
    let distance = haversine_distance(prev_lat as f64, prev_lon as f64, current_lat, current_lon);

    // Minimum-distance suppression
    if distance < config.min_distance_km as f64 {
        return Ok(());
    }

    let elapsed_seconds = (started_at - prev_started).num_seconds().max(1) as f64;
    let velocity = implied_velocity_kmh(distance, elapsed_seconds);

    // Velocity threshold check
    if velocity <= config.velocity_threshold_kmh as f64 {
        return Ok(());
    }

    // Determine severity
    let severity = if is_trusted {
        "low"
    } else {
        let country_code = current_country.unwrap_or("");
        let in_baseline = is_country_in_baseline(pool, user_id, country_code).await?;
        if in_baseline {
            "low"
        } else if distance > INTERCONTINENTAL_DISTANCE_KM {
            "high"
        } else {
            "medium"
        }
    };

    let score_impact: i32 = match severity {
        "low" => 2,
        "high" => 10,
        _ => 5,
    };

    let details = serde_json::json!({
        "previous_location": {
            "city": prev_city,
            "country": prev_country,
            "lat": prev_lat,
            "lon": prev_lon,
        },
        "new_location": {
            "city": current_geo.city,
            "country": current_geo.country_iso,
            "lat": current_lat,
            "lon": current_lon,
        },
        "distance_km": distance.round() as u64,
        "elapsed_seconds": elapsed_seconds as u64,
        "implied_velocity_kmh": velocity.round() as u64,
        "velocity_threshold_kmh": config.velocity_threshold_kmh,
        "trusted_ip": is_trusted,
        "same_device": prev_device.is_some() && prev_device.as_deref() == current_geo.time_zone.as_deref(),
    });

    create_trust_event(pool, user_id, session_id, severity, score_impact, details).await?;

    tracing::info!(
        session_id = %session_id,
        user_id = %user_id,
        severity = severity,
        distance_km = distance.round() as u64,
        velocity_kmh = velocity.round() as u64,
        "impossible travel detected"
    );

    Ok(())
}

fn is_ip_trusted(ip: &IpAddr, trusted_ips: &[String], trusted_cidrs: &[String]) -> bool {
    for t in trusted_ips {
        if let Ok(t_ip) = t.trim().parse::<IpAddr>()
            && t_ip == *ip
        {
            return true;
        }
    }
    for cidr in trusted_cidrs {
        if let Ok(net) = cidr.trim().parse::<ipnet::IpNet>()
            && net.contains(ip)
        {
            return true;
        }
    }
    false
}

async fn is_country_in_baseline(
    pool: &PgPool,
    user_id: Uuid,
    country_code: &str,
) -> Result<bool, sqlx::Error> {
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM user_location_history \
         WHERE user_id = $1 AND country_code = $2 \
           AND last_seen_at >= now() - interval '90 days')",
    )
    .bind(user_id)
    .bind(country_code)
    .fetch_one(pool)
    .await?;
    Ok(exists)
}

async fn create_trust_event(
    pool: &PgPool,
    user_id: Uuid,
    session_id: Uuid,
    severity: &str,
    score_impact: i32,
    details: serde_json::Value,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO user_trust_events \
             (id, user_id, play_session_id, rule_type, severity, score_impact, details) \
         VALUES (uuidv7(), $1, $2, 'impossible_travel', $3, $4, $5)",
    )
    .bind(user_id)
    .bind(session_id)
    .bind(severity)
    .bind(score_impact)
    .bind(&details)
    .execute(pool)
    .await?;

    upsert_trust_score(pool, user_id, score_impact).await?;

    Ok(())
}

async fn upsert_trust_score(
    pool: &PgPool,
    user_id: Uuid,
    score_impact: i32,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO user_trust_scores (id, user_id, score, total_violations, last_violation_at) \
         VALUES (uuidv7(), $1, GREATEST(0, 100 - $2), 1, now()) \
         ON CONFLICT (user_id) DO UPDATE \
         SET score = GREATEST(0, user_trust_scores.score - $2), \
             total_violations = user_trust_scores.total_violations + 1, \
             last_violation_at = now(), \
             updated_at = now()",
    )
    .bind(user_id)
    .bind(score_impact)
    .execute(pool)
    .await?;
    Ok(())
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

    // --- Trust Engine: Haversine + Velocity ---

    #[test]
    fn haversine_same_point_is_zero() {
        let d = haversine_distance(41.0, -87.0, 41.0, -87.0);
        assert!(d.abs() < 0.1, "same point should be ~0 km, got {d}");
    }

    #[test]
    fn haversine_chicago_to_london() {
        let d = haversine_distance(41.8781, -87.6298, 51.5074, -0.1278);
        assert!(
            (d - 6360.0).abs() < 100.0,
            "Chicago→London should be ~6360 km, got {d}"
        );
    }

    #[test]
    fn haversine_nyc_to_la() {
        let d = haversine_distance(40.7128, -74.0060, 34.0522, -118.2437);
        assert!(
            (d - 3940.0).abs() < 100.0,
            "NYC→LA should be ~3940 km, got {d}"
        );
    }

    #[test]
    fn haversine_is_symmetric() {
        let d1 = haversine_distance(35.0, 139.0, 40.0, -74.0);
        let d2 = haversine_distance(40.0, -74.0, 35.0, 139.0);
        assert!((d1 - d2).abs() < 0.001, "haversine should be symmetric");
    }

    #[test]
    fn velocity_normal_flight_speed() {
        let v = implied_velocity_kmh(6360.0, 8.0 * 3600.0);
        assert!(
            (v - 795.0).abs() < 5.0,
            "6360 km in 8h should be ~795 km/h, got {v}"
        );
    }

    #[test]
    fn velocity_impossible_travel() {
        let v = implied_velocity_kmh(6360.0, 0.5 * 3600.0);
        assert!(
            v > 1000.0,
            "6360 km in 30min should exceed 1000 km/h, got {v}"
        );
    }

    #[test]
    fn velocity_zero_elapsed_is_infinite() {
        let v = implied_velocity_kmh(500.0, 0.0);
        assert!(v.is_infinite(), "zero elapsed time should be infinite velocity");
    }

    #[test]
    fn velocity_negative_elapsed_is_infinite() {
        let v = implied_velocity_kmh(500.0, -10.0);
        assert!(v.is_infinite());
    }

    // --- Trusted IP matching ---

    #[test]
    fn trusted_ip_exact_match() {
        let trusted = vec!["8.8.8.8".to_string()];
        let ip: IpAddr = "8.8.8.8".parse().unwrap();
        assert!(is_ip_trusted(&ip, &trusted, &[]));
    }

    #[test]
    fn trusted_ip_no_match() {
        let trusted = vec!["8.8.8.8".to_string()];
        let ip: IpAddr = "1.1.1.1".parse().unwrap();
        assert!(!is_ip_trusted(&ip, &trusted, &[]));
    }

    #[test]
    fn trusted_cidr_match() {
        let cidrs = vec!["203.0.113.0/24".to_string()];
        let ip: IpAddr = "203.0.113.50".parse().unwrap();
        assert!(is_ip_trusted(&ip, &[], &cidrs));
    }

    #[test]
    fn trusted_cidr_no_match() {
        let cidrs = vec!["203.0.113.0/24".to_string()];
        let ip: IpAddr = "198.51.100.1".parse().unwrap();
        assert!(!is_ip_trusted(&ip, &[], &cidrs));
    }

    #[test]
    fn trusted_ipv6_cidr_match() {
        let cidrs = vec!["2001:db8::/32".to_string()];
        let ip: IpAddr = "2001:db8::1".parse().unwrap();
        assert!(is_ip_trusted(&ip, &[], &cidrs));
    }

    #[test]
    fn trusted_empty_lists_never_match() {
        let ip: IpAddr = "8.8.8.8".parse().unwrap();
        assert!(!is_ip_trusted(&ip, &[], &[]));
    }

    #[test]
    fn trusted_ip_with_whitespace_is_trimmed() {
        let trusted = vec!["  8.8.8.8  ".to_string()];
        let ip: IpAddr = "8.8.8.8".parse().unwrap();
        assert!(is_ip_trusted(&ip, &trusted, &[]));
    }
}

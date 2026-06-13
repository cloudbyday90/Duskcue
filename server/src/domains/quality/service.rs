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

use chrono::Utc;
use sqlx::Row;
use uuid::Uuid;

use crate::domains::quality::error::QualityError;
use crate::domains::quality::types::*;

static CONSERVATIVE_BASELINE_VIDEO_CODECS: &str = r#"["h264"]"#;
static CONSERVATIVE_BASELINE_AUDIO_CODECS: &str = r#"["aac"]"#;
static CONSERVATIVE_BASELINE_SUBTITLE_FORMATS: &str = r#"["srt","webvtt"]"#;
static CONSERVATIVE_BASELINE_CONTAINERS: &str = r#"["mp4"]"#;
static CONSERVATIVE_BASELINE_HDR_SUPPORT: &str = "[]";
static CONSERVATIVE_BASELINE_MAX_RESOLUTION: &str = "1080p";

pub static WIZARD_TEST_MATRIX: &[(&str, &str, &str, &str, &str, i32, &str)] = &[
    ("h264_8bit_1080p_mp4", "H.264 8-bit 1080p MP4", "h264", "1080p", "mp4", 8, "sdr"),
    ("h264_10bit_1080p_mp4", "H.264 10-bit 1080p MP4", "h264", "1080p", "mp4", 10, "sdr"),
    ("hevc_8bit_1080p_mp4", "HEVC 8-bit 1080p MP4", "hevc", "1080p", "mp4", 8, "sdr"),
    ("hevc_10bit_1080p_mp4", "HEVC 10-bit 1080p MP4", "hevc", "1080p", "mp4", 10, "sdr"),
    ("hevc_10bit_4k_hdr10_mkv", "HEVC 10-bit 4K HDR10 MKV", "hevc", "4k", "mkv", 10, "hdr10"),
    ("av1_8bit_1080p_mp4", "AV1 8-bit 1080p MP4", "av1", "1080p", "mp4", 8, "sdr"),
    ("av1_10bit_4k_mp4", "AV1 10-bit 4K MP4", "av1", "4k", "mp4", 10, "sdr"),
    ("dolby_vision_p8_mp4", "Dolby Vision Profile 8 MP4", "hevc", "4k", "mp4", 10, "dolby_vision"),
    ("aac_51_ac3_dts", "AAC 5.1 + AC3 + DTS audio", "aac", "", "", 0, ""),
    ("pgs_subtitle_overlay", "PGS subtitle overlay", "", "", "", 0, ""),
];

pub async fn report_capabilities(
    pool: &sqlx::PgPool,
    user_id: Uuid,
    device_identifier: &str,
    req: &ReportCapabilitiesRequest,
) -> Result<DeviceProfileResponse, QualityError> {
    let video_codecs = req.video_codecs
        .as_ref()
        .map(|v| serde_json::to_string(v).unwrap_or_else(|_| CONSERVATIVE_BASELINE_VIDEO_CODECS.to_string()))
        .unwrap_or_else(|| CONSERVATIVE_BASELINE_VIDEO_CODECS.to_string());
    let audio_codecs = req.audio_codecs
        .as_ref()
        .map(|v| serde_json::to_string(v).unwrap_or_else(|_| CONSERVATIVE_BASELINE_AUDIO_CODECS.to_string()))
        .unwrap_or_else(|| CONSERVATIVE_BASELINE_AUDIO_CODECS.to_string());
    let subtitle_formats = req.subtitle_formats
        .as_ref()
        .map(|v| serde_json::to_string(v).unwrap_or_else(|_| CONSERVATIVE_BASELINE_SUBTITLE_FORMATS.to_string()))
        .unwrap_or_else(|| CONSERVATIVE_BASELINE_SUBTITLE_FORMATS.to_string());
    let containers = req.containers
        .as_ref()
        .map(|v| serde_json::to_string(v).unwrap_or_else(|_| CONSERVATIVE_BASELINE_CONTAINERS.to_string()))
        .unwrap_or_else(|| CONSERVATIVE_BASELINE_CONTAINERS.to_string());
    let hdr_support = req.hdr_support
        .as_ref()
        .map(|v| serde_json::to_string(v).unwrap_or_else(|_| CONSERVATIVE_BASELINE_HDR_SUPPORT.to_string()))
        .unwrap_or_else(|| CONSERVATIVE_BASELINE_HDR_SUPPORT.to_string());

    let max_resolution = req.max_resolution
        .as_deref()
        .unwrap_or(CONSERVATIVE_BASELINE_MAX_RESOLUTION);
    let max_audio_channels = req.max_audio_channels.unwrap_or(2);
    let spatial_audio = req.spatial_audio.unwrap_or(false);
    let max_bitrate_bps = req.max_bitrate_bps.unwrap_or(6_000_000);
    let allow_client_side_dv_fallback = req.allow_client_side_dv_fallback.unwrap_or(true);

    let row = sqlx::query(
        r#"INSERT INTO device_profiles (
            device_identifier, platform, model, os_version,
            client_name, client_version,
            video_codecs, audio_codecs, subtitle_formats, containers,
            max_resolution, max_framerate, hdr_support,
            max_audio_channels, spatial_audio, max_bitrate_bps,
            allow_client_side_dv_fallback, profile_source, metadata
        ) VALUES ($1, $2, $3, $4, $5, $6, $7::jsonb, $8::jsonb, $9::jsonb, $10::jsonb, $11, $12, $13::jsonb, $14, $15, $16, $17, 'client_report', jsonb_build_object('reported_by_user_id', $18))
        ON CONFLICT (device_identifier) DO UPDATE SET
            platform = EXCLUDED.platform,
            model = EXCLUDED.model,
            os_version = EXCLUDED.os_version,
            client_name = EXCLUDED.client_name,
            client_version = EXCLUDED.client_version,
            video_codecs = EXCLUDED.video_codecs,
            audio_codecs = EXCLUDED.audio_codecs,
            subtitle_formats = EXCLUDED.subtitle_formats,
            containers = EXCLUDED.containers,
            max_resolution = EXCLUDED.max_resolution,
            max_framerate = EXCLUDED.max_framerate,
            hdr_support = EXCLUDED.hdr_support,
            max_audio_channels = EXCLUDED.max_audio_channels,
            spatial_audio = EXCLUDED.spatial_audio,
            max_bitrate_bps = EXCLUDED.max_bitrate_bps,
            allow_client_side_dv_fallback = EXCLUDED.allow_client_side_dv_fallback,
            updated_at = now(),
            metadata = device_profiles.metadata || EXCLUDED.metadata
        RETURNING id, device_identifier, platform, model, os_version,
            client_name, client_version,
            video_codecs, audio_codecs, subtitle_formats, containers,
            max_resolution, max_framerate, hdr_support,
            max_audio_channels, spatial_audio, max_bitrate_bps,
            allow_client_side_dv_fallback, profile_source, wizard_completed_at"#
    )
        .bind(device_identifier)
        .bind(&req.platform)
        .bind(&req.model)
        .bind(&req.os_version)
        .bind(&req.client_name)
        .bind(&req.client_version)
        .bind(&video_codecs)
        .bind(&audio_codecs)
        .bind(&subtitle_formats)
        .bind(&containers)
        .bind(max_resolution)
        .bind(req.max_framerate)
        .bind(&hdr_support)
        .bind(max_audio_channels)
        .bind(spatial_audio)
        .bind(max_bitrate_bps)
        .bind(allow_client_side_dv_fallback)
        .bind(user_id)
        .fetch_one(pool)
        .await
        .map_err(QualityError::Database)?;

    Ok(row_to_device_profile_response(&row))
}

pub async fn get_device_profile(
    pool: &sqlx::PgPool,
    device_identifier: &str,
) -> Result<DeviceProfileResponse, QualityError> {
    let row = sqlx::query(
        r#"SELECT id, device_identifier, platform, model, os_version,
            client_name, client_version,
            video_codecs, audio_codecs, subtitle_formats, containers,
            max_resolution, max_framerate, hdr_support,
            max_audio_channels, spatial_audio, max_bitrate_bps,
            allow_client_side_dv_fallback, profile_source, wizard_completed_at
        FROM device_profiles
        WHERE device_identifier = $1"#
    )
        .bind(device_identifier)
        .fetch_optional(pool)
        .await
        .map_err(QualityError::Database)?;

    match row {
        Some(r) => Ok(row_to_device_profile_response(&r)),
        None => Ok(create_conservative_baseline_response(device_identifier)),
    }
}

pub async fn list_capability_tests(
    pool: &sqlx::PgPool,
    device_identifier: &str,
) -> Result<CapabilityTestListResponse, QualityError> {
    let rows = sqlx::query(
        r#"SELECT t.id, t.test_format, t.test_description, t.result,
            t.actual_codec, t.actual_resolution, t.actual_bit_depth, t.actual_dynamic_range
        FROM device_capability_tests t
        JOIN device_profiles p ON t.device_profile_id = p.id
        WHERE p.device_identifier = $1
        ORDER BY t.created_at ASC"#
    )
        .bind(device_identifier)
        .fetch_all(pool)
        .await
        .map_err(QualityError::Database)?;

    let items = rows.iter().map(|r| CapabilityTestResponse {
        id: r.get("id"),
        test_format: r.get("test_format"),
        test_description: r.get("test_description"),
        result: r.get("result"),
        actual_codec: r.get("actual_codec"),
        actual_resolution: r.get("actual_resolution"),
        actual_bit_depth: r.get("actual_bit_depth"),
        actual_dynamic_range: r.get("actual_dynamic_range"),
    }).collect();

    Ok(CapabilityTestListResponse { items })
}

pub async fn start_wizard(
    pool: &sqlx::PgPool,
    user_id: Uuid,
    device_identifier: &str,
) -> Result<WizardStartResponse, QualityError> {
    let profile_row = sqlx::query(
        r#"SELECT id, wizard_completed_at FROM device_profiles WHERE device_identifier = $1"#
    )
        .bind(device_identifier)
        .fetch_optional(pool)
        .await
        .map_err(QualityError::Database)?;

    let profile_id = match profile_row {
        Some(row) => {
            if row.get::<Option<chrono::DateTime<Utc>>, _>("wizard_completed_at").is_some() {
                return Err(QualityError::WizardAlreadyCompleted);
            }
            row.get::<Uuid, _>("id")
        }
        None => {
            let row = sqlx::query(
                r#"INSERT INTO device_profiles (
                    device_identifier, platform, profile_source, metadata
                ) VALUES ($1, 'unknown', 'capability_wizard', jsonb_build_object('wizard_started_by', $2))
                RETURNING id"#
            )
                .bind(device_identifier)
                .bind(user_id)
                .fetch_one(pool)
                .await
                .map_err(QualityError::Database)?;
            row.get::<Uuid, _>("id")
        }
    };

    let mut tests = Vec::with_capacity(WIZARD_TEST_MATRIX.len());
    for (format, desc, codec, resolution, _container, bit_depth, dynamic_range) in WIZARD_TEST_MATRIX {
        let row = sqlx::query(
            r#"INSERT INTO device_capability_tests (
                device_profile_id, test_format, test_description, result,
                actual_codec, actual_resolution, actual_bit_depth, actual_dynamic_range
            ) VALUES ($1, $2, $3, 'pending', $4, $5, $6, $7)
            RETURNING id, test_format, test_description, result,
                actual_codec, actual_resolution, actual_bit_depth, actual_dynamic_range"#
        )
            .bind(profile_id)
            .bind(*format)
            .bind(*desc)
            .bind(if codec.is_empty() { None::<String> } else { Some(codec.to_string()) })
            .bind(if resolution.is_empty() { None::<String> } else { Some(resolution.to_string()) })
            .bind(if *bit_depth == 0 { None::<i32> } else { Some(*bit_depth) })
            .bind(if dynamic_range.is_empty() { None::<String> } else { Some(dynamic_range.to_string()) })
            .fetch_one(pool)
            .await
            .map_err(QualityError::Database)?;

        tests.push(CapabilityTestResponse {
            id: row.get("id"),
            test_format: row.get("test_format"),
            test_description: row.get("test_description"),
            result: row.get("result"),
            actual_codec: row.get("actual_codec"),
            actual_resolution: row.get("actual_resolution"),
            actual_bit_depth: row.get("actual_bit_depth"),
            actual_dynamic_range: row.get("actual_dynamic_range"),
        });
    }

    Ok(WizardStartResponse { profile_id, tests })
}

pub async fn submit_wizard_test_result(
    pool: &sqlx::PgPool,
    test_id: Uuid,
    req: &WizardTestResultRequest,
) -> Result<CapabilityTestResponse, QualityError> {
    validate_wizard_result(&req.result)?;

    let row = sqlx::query(
        r#"UPDATE device_capability_tests SET
            result = $1,
            actual_codec = COALESCE($2, actual_codec),
            actual_resolution = COALESCE($3, actual_resolution),
            actual_bit_depth = COALESCE($4, actual_bit_depth),
            actual_dynamic_range = COALESCE($5, actual_dynamic_range),
            details = COALESCE($6, details)
        WHERE id = $7
        RETURNING id, test_format, test_description, result,
            actual_codec, actual_resolution, actual_bit_depth, actual_dynamic_range"#
    )
        .bind(&req.result)
        .bind(&req.actual_codec)
        .bind(&req.actual_resolution)
        .bind(req.actual_bit_depth)
        .bind(&req.actual_dynamic_range)
        .bind(&req.details)
        .bind(test_id)
        .fetch_optional(pool)
        .await
        .map_err(QualityError::Database)?;

    let test_row = row.ok_or(QualityError::WizardTestNotFound)?;

    let response = CapabilityTestResponse {
        id: test_row.get("id"),
        test_format: test_row.get("test_format"),
        test_description: test_row.get("test_description"),
        result: test_row.get("result"),
        actual_codec: test_row.get("actual_codec"),
        actual_resolution: test_row.get("actual_resolution"),
        actual_bit_depth: test_row.get("actual_bit_depth"),
        actual_dynamic_range: test_row.get("actual_dynamic_range"),
    };

    if let Ok(profile_id) = get_profile_id_for_test(pool, test_id).await {
        let _ = try_complete_wizard(pool, profile_id).await;
    }

    Ok(response)
}

pub async fn get_or_create_device_profile(
    pool: &sqlx::PgPool,
    device_identifier: &str,
    platform: &str,
) -> Result<DeviceProfileResponse, QualityError> {
    let row = sqlx::query(
        r#"SELECT id, device_identifier, platform, model, os_version,
            client_name, client_version,
            video_codecs, audio_codecs, subtitle_formats, containers,
            max_resolution, max_framerate, hdr_support,
            max_audio_channels, spatial_audio, max_bitrate_bps,
            allow_client_side_dv_fallback, profile_source, wizard_completed_at
        FROM device_profiles
        WHERE device_identifier = $1"#
    )
        .bind(device_identifier)
        .fetch_optional(pool)
        .await
        .map_err(QualityError::Database)?;

    match row {
        Some(r) => Ok(row_to_device_profile_response(&r)),
        None => {
            let row = sqlx::query(
                r#"INSERT INTO device_profiles (
                    device_identifier, platform, profile_source
                ) VALUES ($1, $2, 'known_device')
                RETURNING id, device_identifier, platform, model, os_version,
                    client_name, client_version,
                    video_codecs, audio_codecs, subtitle_formats, containers,
                    max_resolution, max_framerate, hdr_support,
                    max_audio_channels, spatial_audio, max_bitrate_bps,
                    allow_client_side_dv_fallback, profile_source, wizard_completed_at"#
            )
                .bind(device_identifier)
                .bind(platform)
                .fetch_one(pool)
                .await
                .map_err(QualityError::Database)?;
            Ok(row_to_device_profile_response(&row))
        }
    }
}

fn validate_wizard_result(result: &str) -> Result<(), QualityError> {
    if VALID_WIZARD_RESULTS.contains(&result) || result == "pending" {
        Ok(())
    } else {
        Err(QualityError::InvalidTelemetry(format!(
            "invalid wizard result '{}', must be one of: {}",
            result,
            VALID_WIZARD_RESULTS.join(", ")
        )))
    }
}

async fn get_profile_id_for_test(
    pool: &sqlx::PgPool,
    test_id: Uuid,
) -> Result<Uuid, QualityError> {
    let row = sqlx::query(
        "SELECT device_profile_id FROM device_capability_tests WHERE id = $1"
    )
        .bind(test_id)
        .fetch_optional(pool)
        .await
        .map_err(QualityError::Database)?;

    row.map(|r| r.get("device_profile_id"))
        .ok_or(QualityError::WizardTestNotFound)
}

async fn try_complete_wizard(
    pool: &sqlx::PgPool,
    profile_id: Uuid,
) -> Result<bool, QualityError> {
    let total = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM device_capability_tests WHERE device_profile_id = $1"
    )
        .bind(profile_id)
        .fetch_one(pool)
        .await
        .map_err(QualityError::Database)?;

    let completed = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM device_capability_tests WHERE device_profile_id = $1 AND result != 'pending'"
    )
        .bind(profile_id)
        .fetch_one(pool)
        .await
        .map_err(QualityError::Database)?;

    if completed < total {
        return Ok(false);
    }

    let all_passed = derive_capabilities_from_wizard(pool, profile_id).await?;

    sqlx::query(
        r#"UPDATE device_profiles SET
            wizard_completed_at = now(),
            profile_source = 'capability_wizard',
            updated_at = now()
        WHERE id = $1 AND wizard_completed_at IS NULL"#
    )
        .bind(profile_id)
        .execute(pool)
        .await
        .map_err(QualityError::Database)?;

    Ok(all_passed)
}

async fn derive_capabilities_from_wizard(
    pool: &sqlx::PgPool,
    profile_id: Uuid,
) -> Result<bool, QualityError> {
    let tests = sqlx::query(
        "SELECT test_format, result FROM device_capability_tests WHERE device_profile_id = $1"
    )
        .bind(profile_id)
        .fetch_all(pool)
        .await
        .map_err(QualityError::Database)?;

    let mut video_codecs = vec!["h264".to_string()];
    let mut audio_codecs = vec!["aac".to_string()];
    let mut subtitle_formats = vec!["srt".to_string(), "webvtt".to_string()];
    let mut containers = vec!["mp4".to_string()];
    let mut max_resolution = "1080p".to_string();
    let mut hdr_formats: Vec<String> = Vec::new();
    let mut max_audio_channels = 2;

    for row in &tests {
        let format: String = row.get("test_format");
        let result: String = row.get("result");

        if result != "success" {
            continue;
        }

        match format.as_str() {
            "h264_10bit_1080p_mp4" => {
                video_codecs.push("h264_10bit".to_string());
            }
            "hevc_8bit_1080p_mp4" => {
                if !video_codecs.contains(&"hevc".to_string()) {
                    video_codecs.push("hevc".to_string());
                }
            }
            "hevc_10bit_1080p_mp4" => {
                if !video_codecs.contains(&"hevc".to_string()) {
                    video_codecs.push("hevc".to_string());
                }
                video_codecs.push("hevc_10bit".to_string());
            }
            "hevc_10bit_4k_hdr10_mkv" => {
                if !video_codecs.contains(&"hevc".to_string()) {
                    video_codecs.push("hevc".to_string());
                }
                video_codecs.push("hevc_10bit".to_string());
                if !containers.contains(&"mkv".to_string()) {
                    containers.push("mkv".to_string());
                }
                hdr_formats.push("hdr10".to_string());
                max_resolution = "4k".to_string();
            }
            "av1_8bit_1080p_mp4" => {
                video_codecs.push("av1".to_string());
            }
            "av1_10bit_4k_mp4" => {
                if !video_codecs.contains(&"av1".to_string()) {
                    video_codecs.push("av1".to_string());
                }
                video_codecs.push("av1_10bit".to_string());
                max_resolution = "4k".to_string();
            }
            "dolby_vision_p8_mp4" => {
                hdr_formats.push("dolby_vision".to_string());
            }
            "aac_51_ac3_dts" => {
                audio_codecs.push("ac3".to_string());
                audio_codecs.push("eac3".to_string());
                audio_codecs.push("dts".to_string());
                max_audio_channels = 6;
            }
            "pgs_subtitle_overlay" => {
                subtitle_formats.push("pgs".to_string());
            }
            _ => {}
        }
    }

    let h264_baseline = test_passed(&tests, "h264_8bit_1080p_mp4");
    if !h264_baseline {
        video_codecs = vec!["h264".to_string()];
    }

    let video_codecs_json = serde_json::to_string(&video_codecs).unwrap_or_else(|_| CONSERVATIVE_BASELINE_VIDEO_CODECS.to_string());
    let audio_codecs_json = serde_json::to_string(&audio_codecs).unwrap_or_else(|_| CONSERVATIVE_BASELINE_AUDIO_CODECS.to_string());
    let subtitle_formats_json = serde_json::to_string(&subtitle_formats).unwrap_or_else(|_| CONSERVATIVE_BASELINE_SUBTITLE_FORMATS.to_string());
    let containers_json = serde_json::to_string(&containers).unwrap_or_else(|_| CONSERVATIVE_BASELINE_CONTAINERS.to_string());
    let hdr_json = serde_json::to_string(&hdr_formats).unwrap_or_else(|_| CONSERVATIVE_BASELINE_HDR_SUPPORT.to_string());

    sqlx::query(
        r#"UPDATE device_profiles SET
            video_codecs = $1::jsonb,
            audio_codecs = $2::jsonb,
            subtitle_formats = $3::jsonb,
            containers = $4::jsonb,
            max_resolution = $5,
            hdr_support = $6::jsonb,
            max_audio_channels = $7,
            updated_at = now()
        WHERE id = $8"#
    )
        .bind(&video_codecs_json)
        .bind(&audio_codecs_json)
        .bind(&subtitle_formats_json)
        .bind(&containers_json)
        .bind(&max_resolution)
        .bind(&hdr_json)
        .bind(max_audio_channels)
        .bind(profile_id)
        .execute(pool)
        .await
        .map_err(QualityError::Database)?;

    Ok(h264_baseline)
}

fn test_passed(tests: &[sqlx::postgres::PgRow], format: &str) -> bool {
    tests.iter().any(|r| {
        let f: String = r.get("test_format");
        let res: String = r.get("result");
        f == format && res == "success"
    })
}

fn row_to_device_profile_response(row: &sqlx::postgres::PgRow) -> DeviceProfileResponse {
    let wizard_completed_at: Option<chrono::DateTime<Utc>> = row.try_get("wizard_completed_at").ok();
    DeviceProfileResponse {
        id: row.get("id"),
        device_identifier: row.get("device_identifier"),
        platform: row.get("platform"),
        model: row.get("model"),
        os_version: row.get("os_version"),
        client_name: row.get("client_name"),
        client_version: row.get("client_version"),
        video_codecs: row.get("video_codecs"),
        audio_codecs: row.get("audio_codecs"),
        subtitle_formats: row.get("subtitle_formats"),
        containers: row.get("containers"),
        max_resolution: row.get("max_resolution"),
        max_framerate: row.get("max_framerate"),
        hdr_support: row.get("hdr_support"),
        max_audio_channels: row.get("max_audio_channels"),
        spatial_audio: row.get("spatial_audio"),
        max_bitrate_bps: row.get("max_bitrate_bps"),
        allow_client_side_dv_fallback: row.get("allow_client_side_dv_fallback"),
        profile_source: row.get("profile_source"),
        wizard_completed_at: wizard_completed_at.map(|dt| dt.to_rfc3339()),
    }
}

fn create_conservative_baseline_response(device_identifier: &str) -> DeviceProfileResponse {
    DeviceProfileResponse {
        id: Uuid::nil(),
        device_identifier: device_identifier.to_string(),
        platform: "unknown".to_string(),
        model: None,
        os_version: None,
        client_name: None,
        client_version: None,
        video_codecs: serde_json::from_str(CONSERVATIVE_BASELINE_VIDEO_CODECS).unwrap_or_default(),
        audio_codecs: serde_json::from_str(CONSERVATIVE_BASELINE_AUDIO_CODECS).unwrap_or_default(),
        subtitle_formats: serde_json::from_str(CONSERVATIVE_BASELINE_SUBTITLE_FORMATS).unwrap_or_default(),
        containers: serde_json::from_str(CONSERVATIVE_BASELINE_CONTAINERS).unwrap_or_default(),
        max_resolution: Some(CONSERVATIVE_BASELINE_MAX_RESOLUTION.to_string()),
        max_framerate: None,
        hdr_support: serde_json::from_str(CONSERVATIVE_BASELINE_HDR_SUPPORT).unwrap_or_default(),
        max_audio_channels: Some(2),
        spatial_audio: false,
        max_bitrate_bps: Some(6_000_000),
        allow_client_side_dv_fallback: true,
        profile_source: "known_device".to_string(),
        wizard_completed_at: None,
    }
}

pub fn classify_network_tier(throughput_bps: i64) -> &'static str {
    if throughput_bps > 25_000_000 {
        "excellent"
    } else if throughput_bps >= 10_000_000 {
        "good"
    } else if throughput_bps >= 5_000_000 {
        "moderate"
    } else if throughput_bps >= 2_000_000 {
        "slow"
    } else if throughput_bps >= 500_000 {
        "very_slow"
    } else {
        "critical"
    }
}

pub fn compute_segment_throughput(segment_bytes: i64, download_start_ms: i64, download_end_ms: i64) -> Option<i64> {
    let duration_ms = download_end_ms.saturating_sub(download_start_ms);
    if duration_ms <= 0 || segment_bytes <= 0 {
        return None;
    }
    let throughput_bps = (segment_bytes * 8 * 1000) / duration_ms;
    Some(throughput_bps)
}

pub async fn compute_harmonic_mean_throughput(
    pool: &sqlx::PgPool,
    user_id: Uuid,
    window: i32,
) -> Option<i64> {
    let rows = sqlx::query(
        r#"SELECT throughput_bps FROM client_network_reports
        WHERE user_id = $1 AND throughput_bps IS NOT NULL
        ORDER BY created_at DESC LIMIT $2"#
    )
        .bind(user_id)
        .bind(window)
        .fetch_all(pool)
        .await
        .ok()?;

    if rows.is_empty() {
        return None;
    }

    let reciprocals: f64 = rows.iter()
        .filter_map(|r| {
            let bps: i64 = r.get("throughput_bps");
            if bps > 0 { Some(1.0 / bps as f64) } else { None }
        })
        .sum();

    if reciprocals <= 0.0 {
        return None;
    }

    Some((rows.len() as f64 / reciprocals) as i64)
}

pub async fn submit_segment_telemetry(
    pool: &sqlx::PgPool,
    user_id: Uuid,
    req: &SegmentTelemetryRequest,
    throughput_window: i32,
) -> Result<TelemetryAckResponse, QualityError> {
    let session_id = req.session_id
        .ok_or_else(|| QualityError::InvalidTelemetry("session_id is required".to_string()))?;

    let throughput_bps = match (req.segment_bytes, req.download_start_ms, req.download_end_ms) {
        (Some(bytes), Some(start), Some(end)) => compute_segment_throughput(bytes, start, end),
        _ => None,
    };

    let harmonic_throughput = if throughput_bps.is_some() {
        compute_harmonic_mean_throughput(pool, user_id, throughput_window).await
    } else {
        None
    };

    let effective_throughput = harmonic_throughput.or(throughput_bps);
    let network_tier = effective_throughput.map(|t| classify_network_tier(t).to_string());

    let row = sqlx::query(
        r#"INSERT INTO client_network_reports (
            user_id, session_id, report_type,
            segment_index, rung,
            payload_bytes, download_start_ms, download_end_ms,
            throughput_bps,
            buffer_seconds, rebuffer_count, rebuffer_total_ms,
            estimated_throughput_bps, network_tier
        ) VALUES ($1, $2, 'segment', $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
        RETURNING id"#
    )
        .bind(user_id)
        .bind(session_id)
        .bind(req.segment_index)
        .bind(&req.rung)
        .bind(req.segment_bytes)
        .bind(req.download_start_ms)
        .bind(req.download_end_ms)
        .bind(throughput_bps)
        .bind(req.buffer_seconds)
        .bind(req.rebuffer_count)
        .bind(req.rebuffer_total_ms)
        .bind(effective_throughput)
        .bind(&network_tier)
        .fetch_one(pool)
        .await
        .map_err(QualityError::Database)?;

    Ok(TelemetryAckResponse {
        report_id: row.get("id"),
        throughput_bps: effective_throughput,
        network_tier,
    })
}

pub async fn submit_bandwidth_probe_result(
    pool: &sqlx::PgPool,
    user_id: Uuid,
    req: &BandwidthProbeResultRequest,
) -> Result<ProbeAckResponse, QualityError> {
    let session_id = req.session_id
        .ok_or_else(|| QualityError::InvalidProbeResult("session_id is required".to_string()))?;

    let probe_bytes = req.probe_bytes
        .filter(|&b| b > 0)
        .ok_or_else(|| QualityError::InvalidProbeResult("probe_bytes must be positive".to_string()))?;
    let download_ms = req.download_ms
        .filter(|&d| d > 0)
        .ok_or_else(|| QualityError::InvalidProbeResult("download_ms must be positive".to_string()))?;

    let computed_throughput = (probe_bytes * 8 * 1000) / download_ms;
    let throughput_bps = req.estimated_throughput_bps.unwrap_or(computed_throughput);
    let network_tier = classify_network_tier(throughput_bps).to_string();

    let row = sqlx::query(
        r#"INSERT INTO client_network_reports (
            user_id, session_id, report_type,
            payload_bytes, download_start_ms, download_end_ms,
            throughput_bps, estimated_throughput_bps, network_tier
        ) VALUES ($1, $2, 'probe', $3, 0, $4, $5, $6, $7)
        RETURNING id"#
    )
        .bind(user_id)
        .bind(session_id)
        .bind(probe_bytes)
        .bind(download_ms)
        .bind(computed_throughput)
        .bind(throughput_bps)
        .bind(&network_tier)
        .fetch_one(pool)
        .await
        .map_err(QualityError::Database)?;

    Ok(ProbeAckResponse {
        report_id: row.get("id"),
        throughput_bps: Some(throughput_bps),
        network_tier: Some(network_tier),
    })
}

pub async fn submit_qoe_report(
    pool: &sqlx::PgPool,
    user_id: Uuid,
    req: &QoeReportRequest,
    report_interval_seconds: i32,
) -> Result<QoeAckResponse, QualityError> {
    let session_id = req.session_id
        .ok_or_else(|| QualityError::InvalidTelemetry("session_id is required".to_string()))?;

    let row = sqlx::query(
        r#"INSERT INTO qoe_reports (
            user_id, session_id, report_interval_seconds,
            startup_time_ms, rebuffer_ratio, average_bitrate_bps,
            switches_per_minute, quality_drops,
            current_rung, current_buffer_seconds
        ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
        RETURNING id"#
    )
        .bind(user_id)
        .bind(session_id)
        .bind(report_interval_seconds)
        .bind(req.startup_time_ms)
        .bind(req.rebuffer_ratio)
        .bind(req.average_bitrate_bps)
        .bind(req.switches_per_minute)
        .bind(req.quality_drops)
        .bind(&req.current_rung)
        .bind(req.current_buffer_seconds)
        .fetch_one(pool)
        .await
        .map_err(QualityError::Database)?;

    Ok(QoeAckResponse {
        report_id: row.get("id"),
    })
}

pub async fn get_network_quality_summary(
    pool: &sqlx::PgPool,
) -> Result<Vec<NetworkQualitySummary>, QualityError> {
    let rows = sqlx::query(
        r#"SELECT
            cnr.user_id,
            cnr.network_tier AS latest_tier,
            cnr.throughput_bps AS latest_throughput_bps,
            summary.sample_count
        FROM client_network_reports cnr
        INNER JOIN LATERAL (
            SELECT user_id, COUNT(*) AS sample_count
            FROM client_network_reports
            WHERE created_at > now() - interval '24 hours'
            GROUP BY user_id
        ) summary ON summary.user_id = cnr.user_id
        WHERE cnr.created_at = (
            SELECT MAX(c2.created_at)
            FROM client_network_reports c2
            WHERE c2.user_id = cnr.user_id
        )
        ORDER BY cnr.created_at DESC"#
    )
        .fetch_all(pool)
        .await
        .map_err(QualityError::Database)?;

    Ok(rows.iter().map(|r| NetworkQualitySummary {
        user_id: r.get("user_id"),
        latest_tier: r.get("latest_tier"),
        latest_throughput_bps: r.get("latest_throughput_bps"),
        sample_count: r.get("sample_count"),
    }).collect())
}

pub async fn get_device_capability_summary(
    pool: &sqlx::PgPool,
) -> Result<Vec<DeviceCapabilitySummary>, QualityError> {
    let rows = sqlx::query(
        r#"SELECT
            platform,
            COUNT(*) AS device_count,
            COALESCE(
                AVG(CASE WHEN wizard_completed_at IS NOT NULL THEN 1.0 ELSE 0.0 END),
                0.0
            ) AS wizard_completion_rate,
            COALESCE(
                (SELECT jsonb_agg(DISTINCT elem)
                 FROM device_profiles dp2, jsonb_array_elements_text(dp2.video_codecs) elem
                 WHERE dp2.platform = dp.platform
                 LIMIT 5),
                '[]'::jsonb
            )::text AS top_video_codecs
        FROM device_profiles dp
        GROUP BY platform
        ORDER BY device_count DESC"#
    )
        .fetch_all(pool)
        .await
        .map_err(QualityError::Database)?;

    Ok(rows.iter().map(|r| {
        let codecs_text: String = r.try_get("top_video_codecs")
            .unwrap_or_else(|_| "[]".to_string());
        let top_codecs: Vec<String> = serde_json::from_str(&codecs_text)
            .unwrap_or_default();
        DeviceCapabilitySummary {
            platform: r.get("platform"),
            device_count: r.get("device_count"),
            wizard_completion_rate: r.get::<f64, _>("wizard_completion_rate") * 100.0,
            top_video_codecs: top_codecs,
        }
    }).collect())
}

pub async fn get_qoe_summary(
    pool: &sqlx::PgPool,
) -> Result<Vec<QoeSummary>, QualityError> {
    let rows = sqlx::query(
        r#"SELECT DISTINCT ON (session_id)
            session_id, user_id,
            startup_time_ms, rebuffer_ratio, average_bitrate_bps,
            switches_per_minute, quality_drops
        FROM qoe_reports
        ORDER BY session_id, created_at DESC
        LIMIT 100"#
    )
        .fetch_all(pool)
        .await
        .map_err(QualityError::Database)?;

    Ok(rows.iter().map(|r| QoeSummary {
        session_id: r.get("session_id"),
        user_id: r.get("user_id"),
        startup_time_ms: r.get("startup_time_ms"),
        rebuffer_ratio: r.get("rebuffer_ratio"),
        average_bitrate_bps: r.get("average_bitrate_bps"),
        switches_per_minute: r.get("switches_per_minute"),
        quality_drops: r.get("quality_drops"),
    }).collect())
}

pub async fn get_transcode_breakdown(
    pool: &sqlx::PgPool,
) -> Result<TranscodeBreakdown, QualityError> {
    let row = sqlx::query(
        r#"SELECT
            COUNT(*) AS total,
            COUNT(*) FILTER (WHERE metadata->>'playback_type' = 'direct_play') AS direct_play,
            COUNT(*) FILTER (WHERE metadata->>'playback_type' = 'direct_stream') AS direct_stream,
            COUNT(*) FILTER (WHERE metadata->>'playback_type' = 'transcode') AS transcode
        FROM play_sessions
        WHERE ended_at IS NOT NULL"#
    )
        .fetch_one(pool)
        .await
        .map_err(QualityError::Database)?;

    let total: i64 = row.try_get("total").unwrap_or(0);
    let direct_play: i64 = row.try_get("direct_play").unwrap_or(0);
    let direct_stream: i64 = row.try_get("direct_stream").unwrap_or(0);
    let transcode: i64 = row.try_get("transcode").unwrap_or(0);

    let direct_play_percentage = if total > 0 {
        (direct_play as f64 / total as f64) * 100.0
    } else {
        0.0
    };

    Ok(TranscodeBreakdown {
        direct_play_count: direct_play,
        direct_stream_count: direct_stream,
        transcode_count: transcode,
        total_sessions: total,
        direct_play_percentage,
    })
}

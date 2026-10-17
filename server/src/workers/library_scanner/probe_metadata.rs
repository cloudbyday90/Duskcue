// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use std::path::Path;

use serde_json::{Value, json};

use super::model::FfprobeStream;

pub(super) fn known(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| {
        !value.is_empty()
            && !["unknown", "n/a", "none", "unspecified"]
                .contains(&value.to_ascii_lowercase().as_str())
    })
}

pub(super) fn positive_integer(value: Option<&str>) -> Option<i32> {
    value?.parse::<i32>().ok().filter(|number| *number > 0)
}

pub(super) fn frame_rate(value: Option<&str>) -> Option<f64> {
    let value = value?;
    let rate = match value.split_once('/') {
        Some((numerator, denominator)) => {
            let numerator = numerator.parse::<f64>().ok()?;
            let denominator = denominator.parse::<f64>().ok()?;
            if numerator <= 0.0 || denominator <= 0.0 {
                return None;
            }
            numerator / denominator
        }
        None => value.parse().ok()?,
    };
    (rate.is_finite() && rate > 0.0).then_some(rate)
}

pub(super) fn canonical_container(format: Option<&str>, path: &Path) -> String {
    let Some(format) = known(format) else {
        return "unknown".into();
    };
    let names: Vec<_> = format
        .split(',')
        .map(str::trim)
        .map(str::to_ascii_lowercase)
        .collect();
    let suffix = path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase);
    if names
        .iter()
        .all(|name| ["matroska", "webm"].contains(&name.as_str()))
    {
        return if names == ["webm"]
            || (suffix.as_deref() == Some("webm") && names.iter().any(|name| name == "webm"))
        {
            "webm".into()
        } else {
            "mkv".into()
        };
    }
    if names
        .iter()
        .all(|name| ["mov", "mp4", "m4a", "3gp", "3g2", "mj2"].contains(&name.as_str()))
    {
        return match suffix.as_deref() {
            Some("mov") if names.iter().any(|name| name == "mov") => "mov",
            Some("3gp") if names.iter().any(|name| name == "3gp") => "3gp",
            Some("3g2") if names.iter().any(|name| name == "3g2") => "3g2",
            _ if names.iter().any(|name| name == "mp4") => "mp4",
            _ => names[0].as_str(),
        }
        .into();
    }
    if names.len() == 1 {
        return match names[0].as_str() {
            "mpegts" => "ts",
            "mpeg" => "mpeg",
            name => name,
        }
        .into();
    }
    format.to_ascii_lowercase()
}

fn sample_bits(value: Option<&Value>) -> Option<u32> {
    let value = value?;
    let bits = value.as_u64().or_else(|| value.as_str()?.parse().ok())?;
    (1..=64).contains(&bits).then_some(bits as u32)
}

fn pixel_bits(format: Option<&str>) -> Option<u32> {
    let format = known(format)?;
    if [
        "rgb24", "bgr24", "rgba", "bgra", "argb", "abgr", "gray", "gray8", "pal8", "nv12", "nv21",
        "yuyv422", "uyvy422",
    ]
    .contains(&format)
    {
        return Some(8);
    }
    if format.starts_with("rgb48")
        || format.starts_with("bgr48")
        || format.starts_with("rgba64")
        || format.starts_with("bgra64")
    {
        return Some(16);
    }
    if format.starts_with("x2rgb10") || format.starts_with("x2bgr10") {
        return Some(10);
    }
    for (prefix, bits) in [
        ("p010", 10),
        ("p012", 12),
        ("p016", 16),
        ("p210", 10),
        ("p212", 12),
        ("p216", 16),
        ("p410", 10),
        ("p412", 12),
        ("p416", 16),
    ] {
        if format.starts_with(prefix) {
            return Some(bits);
        }
    }
    let without_endian = format.trim_end_matches("le").trim_end_matches("be");
    if !["yuv", "yuva", "gbr", "gray"]
        .iter()
        .any(|prefix| without_endian.starts_with(prefix))
    {
        return None;
    }
    if let Some(depth) = without_endian
        .rsplit_once('p')
        .map(|(_, depth)| depth)
        .or_else(|| without_endian.strip_prefix("gray"))
    {
        if depth.is_empty() {
            return Some(8);
        }
        if let Ok(bits) = depth.parse::<u32>() {
            return [8, 9, 10, 12, 14, 16].contains(&bits).then_some(bits);
        }
    }
    None
}

fn normalized_level(codec: Option<&str>, level: Option<i64>) -> Option<f64> {
    let level = level.filter(|level| *level > 0)? as f64;
    match codec {
        Some("h264") => Some(level / 10.0),
        Some("hevc") => Some(level / 30.0),
        _ => None,
    }
}

pub(super) fn video_metadata(stream: &FfprobeStream) -> Value {
    let raw_bits = sample_bits(stream.bits_per_raw_sample.as_ref());
    let rate = frame_rate(stream.avg_frame_rate.as_deref())
        .or_else(|| frame_rate(stream.r_frame_rate.as_deref()));
    json!({
        "index": stream.stream_index(), "codec": known(stream.codec_name.as_deref()),
        "profile": known(stream.profile.as_deref()), "level": normalized_level(stream.codec_name.as_deref(), stream.level), "level_raw": stream.level,
        "width": stream.width.filter(|value| *value > 0), "height": stream.height.filter(|value| *value > 0),
        "bit_depth": raw_bits.or_else(|| pixel_bits(stream.pix_fmt.as_deref())), "bits_per_raw_sample": raw_bits,
        "pixel_format": known(stream.pix_fmt.as_deref()), "avg_frame_rate": stream.avg_frame_rate,
        "r_frame_rate": stream.r_frame_rate, "frame_rate": rate,
        "field_order": known(stream.field_order.as_deref()), "sample_aspect_ratio": known(stream.sample_aspect_ratio.as_deref()),
        "display_aspect_ratio": known(stream.display_aspect_ratio.as_deref()), "color_transfer": known(stream.color_transfer.as_deref()),
        "color_primaries": known(stream.color_primaries.as_deref()), "color_space": known(stream.color_space.as_deref()),
        "color_range": known(stream.color_range.as_deref()), "chroma_location": known(stream.chroma_location.as_deref()),
        "side_data": stream.side_data_list,
    })
}

pub(super) fn dolby_vision(stream: &FfprobeStream) -> Option<Value> {
    let side_data = stream.side_data_list.as_ref()?.iter().find(|data| {
        data.get("side_data_type")
            .and_then(Value::as_str)
            .is_some_and(|kind| kind.eq_ignore_ascii_case("DOVI configuration record"))
    })?;
    let profile = side_data
        .get("dv_profile")?
        .as_u64()
        .filter(|value| (1..=10).contains(value))?;
    let compatibility = side_data
        .get("dv_bl_signal_compatibility_id")
        .and_then(Value::as_u64);
    Some(json!({
        "profile": profile, "level": side_data.get("dv_level").and_then(Value::as_u64),
        "compatibility_id": compatibility,
        "compatibility_mode": match compatibility { Some(1) => Some("hdr10"), Some(2) => Some("sdr"), Some(4) => Some("hlg"), _ => None },
        "rpu_present": side_data.get("rpu_present_flag").and_then(Value::as_u64).map(|value| value == 1),
        "enhancement_layer": side_data.get("el_present_flag").and_then(Value::as_u64).map(|value| value == 1),
        "base_layer": side_data.get("bl_present_flag").and_then(Value::as_u64).map(|value| value == 1),
        "side_data": side_data,
    }))
}

pub(super) fn dynamic_range(stream: &FfprobeStream, dolby: Option<&Value>) -> Option<String> {
    if let Some(profile) = dolby.and_then(|value| value["profile"].as_u64()) {
        return Some(format!("dolby_vision_p{profile}"));
    }
    let range = match known(stream.color_transfer.as_deref())? {
        "smpte2084" => "hdr10",
        "arib-std-b67" => "hlg",
        "bt709" | "gamma22" | "gamma28" | "smpte170m" | "smpte240m" | "iec61966-2-1"
        | "bt2020-10" | "bt2020-12" => "sdr",
        _ => return None,
    };
    Some(range.into())
}

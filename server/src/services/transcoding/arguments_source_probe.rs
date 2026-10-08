// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::*;

pub(super) async fn require_two_second_keyframes(directory: &Path) -> anyhow::Result<()> {
    let frames: Value = serde_json::from_slice(
        &media_command(
            directory,
            "ffprobe",
            [
                "-v",
                "error",
                "-select_streams",
                "v:0",
                "-show_frames",
                "-show_entries",
                "frame=key_frame,best_effort_timestamp_time",
                "-of",
                "json",
                "/fixtures/source.mkv",
            ]
            .into_iter()
            .map(String::from)
            .collect(),
        )
        .await?,
    )?;
    let frames = frames["frames"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("source frame inventory absent"))?;
    anyhow::ensure!(
        (190..=194).contains(&frames.len()),
        "source video frame count changed"
    );
    let mut keyframes = Vec::new();
    for frame in frames {
        if frame["key_frame"] == json!(1) {
            let timestamp: f64 = frame["best_effort_timestamp_time"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("source keyframe timestamp absent"))?
                .parse()?;
            anyhow::ensure!(
                timestamp.is_finite() && timestamp >= 0.0,
                "invalid source keyframe timestamp"
            );
            keyframes.push(timestamp);
        }
    }
    anyhow::ensure!(
        keyframes.len() == 4 && keyframes[0] <= 0.1,
        "source needs four two-second GOPs"
    );
    for timestamps in keyframes.windows(2) {
        anyhow::ensure!(
            (timestamps[1] - timestamps[0] - 2.0).abs() <= 0.01,
            "source keyframe cadence differs from HLS segment duration"
        );
    }
    println!(
        "\nDUSKCUE_SOURCE_KEYFRAMES={}",
        json!({"video_frames":frames.len(),"keyframe_seconds":keyframes,"segment_seconds":2})
    );
    Ok(())
}

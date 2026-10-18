// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::HwAccelMethod;
use crate::state::CpuConfig;

pub(super) fn build_video_encode_args(
    encoder: &str,
    codec: &str,
    width: u32,
    height: u32,
    maxrate: u32,
    gop_size: u32,
    _hw_accel: HwAccelMethod,
) -> Vec<String> {
    let is_hw = !encoder.contains("libx2");

    let mut args = vec!["-c:v:0".to_string(), encoder.to_string()];

    if !is_hw {
        args.extend([
            "-preset".to_string(),
            "veryfast".to_string(),
            "-crf".to_string(),
            "23".to_string(),
        ]);
    }

    args.extend([
        "-maxrate".to_string(),
        maxrate.to_string(),
        "-bufsize".to_string(),
        maxrate.saturating_mul(2).to_string(),
        "-vf".to_string(),
        format!(
            "scale={width}:{height}:force_original_aspect_ratio=decrease,pad={width}:{height}:(ow-iw)/2:(oh-ih)/2"
        ),
    ]);

    args.extend([
        "-g".to_string(),
        gop_size.to_string(),
        "-keyint_min".to_string(),
        gop_size.to_string(),
        "-sc_threshold".to_string(),
        "0".to_string(),
    ]);

    if !is_hw {
        args.extend([
            "-profile:v".to_string(),
            if matches!(codec, "hevc" | "h265") {
                "main"
            } else {
                "high"
            }
            .to_string(),
            "-pix_fmt".to_string(),
            "yuv420p".to_string(),
        ]);
    }

    if matches!(codec, "hevc" | "h265") {
        args.extend(["-tag:v".to_string(), "hvc1".to_string()]);
    }

    args
}

pub(super) fn build_audio_encode_args(
    audio_codec: &str,
    channels: u32,
    bitrate: u32,
) -> Vec<String> {
    let encoder = match audio_codec {
        "ac3" | "ac-3" => "ac3",
        "eac3" | "e-ac-3" => "eac3",
        "opus" => "libopus",
        _ => "aac",
    };

    vec![
        "-c:a:0".to_string(),
        encoder.to_string(),
        "-ac".to_string(),
        channels.to_string(),
        "-b:a".to_string(),
        bitrate.to_string(),
    ]
}

pub(super) fn build_threading_args(cpu: &CpuConfig) -> Vec<String> {
    let mut args = Vec::new();

    if let Some(threads) = cpu.ffmpeg_threads {
        args.extend(["-threads".to_string(), threads.to_string()]);
    }

    args.extend(["-thread_type".to_string(), cpu.ffmpeg_thread_type.clone()]);

    args
}

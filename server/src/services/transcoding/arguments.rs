// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use std::path::Path;

pub fn build_ffmpeg_input_args(seek_position_ms: Option<i64>, source_path: &Path) -> Vec<String> {
    let mut args = Vec::new();

    if let Some(ms) = seek_position_ms
        && ms > 0
    {
        let secs = ms as f64 / 1000.0;
        args.extend(["-ss".to_string(), format!("{secs:.3}")]);
    }

    args.extend([
        "-analyzeduration".to_string(),
        "200M".to_string(),
        "-probesize".to_string(),
        "1G".to_string(),
        "-fflags".to_string(),
        "+genpts".to_string(),
        "-i".to_string(),
        source_path.to_string_lossy().to_string(),
    ]);

    args
}

pub fn build_subtitle_burn_in_filter(source_path: &Path, subtitle_ordinal: usize) -> String {
    let escaped_path = source_path
        .to_string_lossy()
        .replace('\\', "\\\\")
        .replace(':', "\\:")
        .replace('\'', "\\'");
    format!("[0:v:0]subtitles=filename='{escaped_path}':si={subtitle_ordinal}[video]")
}

pub fn build_hls_output_args(
    segment_duration: u32,
    segment_filename: &str,
    manifest_path: &str,
) -> Vec<String> {
    vec![
        "-f".to_string(),
        "hls".to_string(),
        "-hls_time".to_string(),
        segment_duration.to_string(),
        "-hls_segment_type".to_string(),
        "fmp4".to_string(),
        "-hls_fmp4_init_filename".to_string(),
        "init.mp4".to_string(),
        "-hls_list_size".to_string(),
        "0".to_string(),
        "-hls_playlist_type".to_string(),
        "event".to_string(),
        "-hls_segment_filename".to_string(),
        segment_filename.to_string(),
        "-y".to_string(),
        manifest_path.to_string(),
    ]
}

pub fn build_stream_mapping_args(
    audio_stream_index: Option<i32>,
    subtitle_filter: Option<String>,
) -> Vec<String> {
    let mut args = match subtitle_filter {
        Some(filter) => vec![
            "-filter_complex".into(),
            filter,
            "-map".into(),
            "[video]".into(),
        ],
        None => vec!["-map".into(), "0:v:0".into()],
    };
    args.extend([
        "-map".into(),
        audio_stream_index.map_or_else(|| "0:a:0?".into(), |index| format!("0:{index}")),
    ]);
    args
}

pub fn compose_video_filters(
    mut encoder_args: Vec<String>,
    subtitle_filter: Option<String>,
) -> Result<(Vec<String>, Option<String>), &'static str> {
    let Some(subtitle_filter) = subtitle_filter else {
        return Ok((encoder_args, None));
    };
    let prefix = subtitle_filter
        .strip_suffix("[video]")
        .ok_or("subtitle filter output is invalid")?;
    let position = encoder_args
        .iter()
        .position(|argument| argument == "-vf")
        .ok_or("video scaling filter is missing")?;
    if position + 1 >= encoder_args.len() {
        return Err("video scaling filter is missing");
    }
    let scaling = encoder_args.remove(position + 1);
    encoder_args.remove(position);
    Ok((encoder_args, Some(format!("{prefix},{scaling}[video]"))))
}

#[cfg(test)]
#[path = "arguments_tests.rs"]
mod tests;

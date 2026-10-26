// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use std::path::{Path, PathBuf};

use uuid::Uuid;

use super::PlaybackError;
use crate::services::transcoding::{TranscodeManager, TranscodeRendition};

use crate::services::vod_lifecycle::{Content, PLAYLIST, Prepared};
use axum::{
    body::Body,
    http::{StatusCode, header},
    response::Response,
};

fn playlist_response(content: String) -> Result<Prepared, PlaybackError> {
    if content.len() > PLAYLIST {
        return Err(PlaybackError::SessionNotFound);
    }
    let response = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/vnd.apple.mpegurl")
        .header(header::CONTENT_LENGTH, content.len().to_string())
        .header(header::CACHE_CONTROL, "no-cache, no-store, must-revalidate")
        .body(Body::empty())
        .expect("static HLS headers");
    let (parts, _) = response.into_parts();
    Ok(Prepared {
        status: parts.status,
        headers: parts.headers,
        content: Content::Bytes(content.into()),
    })
}

pub async fn get_transcode_manifest(
    manager: &TranscodeManager,
    session_id: Uuid,
) -> Result<Response, PlaybackError> {
    manager.output.ensure_open()?;
    let session = manager
        .get_session(&session_id)
        .ok_or(PlaybackError::SessionNotFound)?;
    manager
        .output
        .response(Some(session_id), move |ctx| {
            Box::pin(async move {
                let content = ctx
                    .text(&session.manifest_path, &session.segment_dir)
                    .await?;
                let normalized = normalize_manifest(&content, session_id, &session.rendition_name)?;
                if is_master(&content) {
                    for line in content
                        .lines()
                        .map(str::trim)
                        .filter(|line| !line.is_empty() && !line.starts_with('#'))
                    {
                        let (_, path) = variant_reference(line)?;
                        let id = ctx
                            .open(&session.segment_dir.join(path), Some(&session.segment_dir))
                            .await?;
                        ctx.close_file(id).await;
                    }
                }
                playlist_response(normalized)
            })
        })
        .await
}

pub async fn get_transcode_playlist(
    manager: &TranscodeManager,
    session_id: Uuid,
    rendition: &str,
) -> Result<Response, PlaybackError> {
    manager.output.ensure_open()?;
    validate_rendition(rendition)?;
    let session = manager
        .get_session(&session_id)
        .ok_or(PlaybackError::SessionNotFound)?;
    let rendition = rendition.to_owned();
    manager
        .output
        .response(Some(session_id), move |ctx| {
            Box::pin(async move {
                let manifest = ctx
                    .text(&session.manifest_path, &session.segment_dir)
                    .await?;
                let path = rendition_playlist(
                    &manifest,
                    &session.segment_dir,
                    &session.manifest_path,
                    &session.rendition_name,
                    &rendition,
                )?;
                let content = ctx.text(&path, &session.segment_dir).await?;
                if is_master(&content) {
                    return Err(PlaybackError::SessionNotFound);
                }
                playlist_response(normalize_manifest(&content, session_id, &rendition)?)
            })
        })
        .await
}

pub async fn get_transcode_segment(
    manager: &TranscodeManager,
    session_id: Uuid,
    rendition: &str,
    filename: &str,
) -> Result<Response, PlaybackError> {
    manager.output.ensure_open()?;
    validate_rendition(rendition)?;
    validate_segment_filename(filename)?;
    let session = manager
        .get_session(&session_id)
        .ok_or(PlaybackError::SessionNotFound)?;
    let rendition = rendition.to_owned();
    let filename = filename.to_owned();
    manager
        .output
        .response(Some(session_id), move |ctx| {
            Box::pin(async move {
                let manifest = ctx
                    .text(&session.manifest_path, &session.segment_dir)
                    .await?;
                let playlist = rendition_playlist(
                    &manifest,
                    &session.segment_dir,
                    &session.manifest_path,
                    &session.rendition_name,
                    &rendition,
                )?;
                let directory = playlist.parent().ok_or(PlaybackError::SessionNotFound)?;
                let id = ctx
                    .open(&directory.join(&filename), Some(&session.segment_dir))
                    .await?;
                let length = ctx.metadata(id).await?.len();
                let response = Response::builder()
                    .status(StatusCode::OK)
                    .header(
                        header::CONTENT_TYPE,
                        if filename.ends_with(".m4s") {
                            "video/iso.segment"
                        } else {
                            "video/mp4"
                        },
                    )
                    .header(header::CONTENT_LENGTH, length.to_string())
                    .header(header::CACHE_CONTROL, "private, no-store")
                    .body(Body::empty())
                    .expect("static segment headers");
                let (parts, _) = response.into_parts();
                Ok(Prepared {
                    status: parts.status,
                    headers: parts.headers,
                    content: Content::File {
                        id,
                        offset: 0,
                        length,
                    },
                })
            })
        })
        .await
}

fn validate_rendition(name: &str) -> Result<(), PlaybackError> {
    if name.is_empty()
        || name.len() > 64
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
    {
        return Err(PlaybackError::SessionNotFound);
    }
    Ok(())
}

fn validate_segment_filename(name: &str) -> Result<(), PlaybackError> {
    if name == "init.mp4" {
        return Ok(());
    }
    let number = name
        .strip_prefix("seg_")
        .and_then(|rest| rest.strip_suffix(".m4s"));
    if name.len() > 64
        || !number.is_some_and(|number| {
            !number.is_empty() && number.bytes().all(|byte| byte.is_ascii_digit())
        })
    {
        return Err(PlaybackError::SessionNotFound);
    }
    Ok(())
}

fn is_master(content: &str) -> bool {
    content
        .lines()
        .any(|line| line.starts_with("#EXT-X-STREAM-INF:"))
}

fn variant_reference(uri: &str) -> Result<(String, PathBuf), PlaybackError> {
    let components: Vec<&str> = uri.split('/').collect();
    let rendition = match components.as_slice() {
        [file] => file.strip_suffix("_index.m3u8"),
        [rendition, "index.m3u8"] => Some(*rendition),
        [rendition, file] if *file == format!("{rendition}_index.m3u8") => Some(*rendition),
        _ => None,
    }
    .ok_or(PlaybackError::SessionNotFound)?;
    validate_rendition(rendition)?;
    Ok((rendition.to_string(), PathBuf::from(uri)))
}

fn rendition_playlist(
    content: &str,
    directory: &Path,
    manifest: &Path,
    session_rendition: &str,
    requested: &str,
) -> Result<PathBuf, PlaybackError> {
    validate_rendition(requested)?;
    if !is_master(content) {
        if requested != session_rendition {
            return Err(PlaybackError::SessionNotFound);
        }
        return Ok(manifest.to_path_buf());
    }
    let mut selected = None;
    for line in content
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
    {
        let (rendition, path) = variant_reference(line)?;
        if rendition == requested {
            if selected.is_some() {
                return Err(PlaybackError::SessionNotFound);
            }
            selected = Some(directory.join(path));
        }
    }
    selected.ok_or(PlaybackError::SessionNotFound)
}

fn public_variant(uri: &str, session_id: Uuid) -> Result<String, PlaybackError> {
    let (rendition, _) = variant_reference(uri)?;
    Ok(format!(
        "/api/v1/transcode/{session_id}/{rendition}/index.m3u8"
    ))
}

fn public_segment(
    filename: &str,
    session_id: Uuid,
    rendition: &str,
) -> Result<String, PlaybackError> {
    validate_segment_filename(filename)?;
    Ok(format!(
        "/api/v1/transcode/{session_id}/{rendition}/{filename}"
    ))
}

fn normalize_manifest(
    content: &str,
    session_id: Uuid,
    rendition: &str,
) -> Result<String, PlaybackError> {
    validate_rendition(rendition)?;
    if content.lines().find(|line| !line.trim().is_empty()) != Some("#EXTM3U") {
        return Err(PlaybackError::SessionNotFound);
    }
    let master = is_master(content);
    let mut variants = std::collections::HashSet::new();
    let mut result = Vec::new();
    let mut bytes = 0_usize;
    for line in content.lines() {
        let line = line.trim();
        if line.starts_with('#') && line.contains("URI=") {
            let valid_tag = !master && line.starts_with("#EXT-X-MAP:");
            if !valid_tag {
                return Err(PlaybackError::SessionNotFound);
            }
            let (prefix, remaining) = line
                .split_once("URI=\"")
                .ok_or(PlaybackError::SessionNotFound)?;
            if !prefix.ends_with([':', ',']) {
                return Err(PlaybackError::SessionNotFound);
            }
            let (uri, suffix) = remaining
                .split_once('"')
                .ok_or(PlaybackError::SessionNotFound)?;
            if suffix.contains("URI=") {
                return Err(PlaybackError::SessionNotFound);
            }
            let normalized = public_segment(uri, session_id, rendition)?;
            result.push(format!("{prefix}URI=\"{normalized}\"{suffix}"));
        } else if !line.is_empty() && !line.starts_with('#') {
            result.push(if master {
                let (variant, _) = variant_reference(line)?;
                if !variants.insert(variant) {
                    return Err(PlaybackError::SessionNotFound);
                }
                public_variant(line, session_id)?
            } else {
                public_segment(line, session_id, rendition)?
            });
        } else {
            result.push(line.to_string());
        }
        bytes = bytes
            .saturating_add(result.last().map_or(0, String::len))
            .saturating_add(1);
        if bytes > PLAYLIST {
            return Err(PlaybackError::SessionNotFound);
        }
    }
    Ok(format!("{}\n", result.join("\n")))
}

pub fn generate_master_manifest(_session_id: Uuid, renditions: &[TranscodeRendition]) -> String {
    let mut lines = vec![
        "#EXTM3U".to_string(),
        "#EXT-X-VERSION:7".to_string(),
        "#EXT-X-INDEPENDENT-SEGMENTS".to_string(),
    ];
    for rendition in renditions {
        lines.push(format!(
            "#EXT-X-STREAM-INF:BANDWIDTH={},RESOLUTION={}x{},CODECS=\"avc1.64001f,mp4a.40.2\"",
            rendition
                .video_bitrate
                .saturating_add(rendition.audio_bitrate),
            rendition.width,
            rendition.height
        ));
        lines.push(format!("{}/index.m3u8", rendition.name));
    }
    format!("{}\n", lines.join("\n"))
}

#[cfg(test)]
#[path = "hls_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "hls_http_tests.rs"]
mod http_tests;

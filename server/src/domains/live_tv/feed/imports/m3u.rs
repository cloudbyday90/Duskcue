// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::{
    ImportError,
    limits::{self, Work},
    model::Channel,
};
use ring::digest::{Context, SHA256};
use serde_json::json;
use std::collections::{BTreeMap, HashSet};
use uuid::Uuid;

pub(super) fn parse(
    bytes: &[u8],
    assignment: Uuid,
    work: &mut Work,
) -> Result<Vec<Channel>, ImportError> {
    if bytes.len() > limits::PLAYLIST {
        return Err(ImportError::Limit);
    }
    let text = std::str::from_utf8(bytes).map_err(|_| ImportError::Format)?;
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut lines = text.lines();
    let header = lines.next().ok_or(ImportError::Format)?;
    limits::text(header, 16384)?;
    if header.len() > 16384 || !(header == "#EXTM3U" || header.starts_with("#EXTM3U ")) {
        return Err(ImportError::Format);
    }
    let mut pending = None;
    let mut channels = Vec::new();
    let mut seen = HashSet::new();
    for line in lines {
        work.check()?;
        if line.len() > 16384 {
            return Err(ImportError::Limit);
        }
        limits::text(line, 16384)?;
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Some(info) = line.strip_prefix("#EXTINF:") {
            if pending.is_some() {
                return Err(ImportError::Format);
            }
            pending = Some(attributes(info)?);
        } else if line.starts_with('#') {
            if line.starts_with("#EXT-X-") || line.starts_with("#EXTM3U") {
                return Err(ImportError::Format);
            }
        } else {
            let (attrs, name) = pending.take().ok_or(ImportError::Format)?;
            limits::text(line, limits::RECORD)?;
            let url = url::Url::parse(line).map_err(|_| ImportError::Format)?;
            if !matches!(url.scheme(), "http" | "https")
                || url.host().is_none()
                || url.fragment().is_some()
                || !url.username().is_empty()
                || url.password().is_some()
            {
                return Err(ImportError::Format);
            }
            let path = url.path().strip_suffix('/').unwrap_or(url.path());
            let mut parts = path.rsplit('/');
            let raw = parts.next().ok_or(ImportError::Format)?;
            let upstream = Uuid::parse_str(raw).map_err(|_| ImportError::Format)?;
            if upstream.hyphenated().to_string() != raw
                || parts.next() != Some("stream")
                || parts.next() != Some("ts")
                || parts.next() != Some("proxy")
                || !seen.insert(upstream)
            {
                return Err(ImportError::Format);
            }
            let mut hash = Context::new(&SHA256);
            hash.update(b"duskcue-live-channel\0\x01");
            hash.update(assignment.as_bytes());
            hash.update(upstream.as_bytes());
            let guide = attrs.get("tvg-id").filter(|s| !s.is_empty()).cloned();
            if let Some(value) = &guide {
                limits::text(value, 512)?;
            }
            let logo = attrs.get("tvg-logo").filter(|s| !s.is_empty()).cloned();
            if let Some(value) = &logo {
                limits::text(value, limits::RECORD)?;
            }
            let metadata = json!({"upstream_uuid":upstream,"name":name,"group":attrs.get("group-title"),"number":attrs.get("tvg-chno"),"tvg_name":attrs.get("tvg-name"),"guide_key":guide});
            let payload = limits::json(&metadata)?;
            work.payload(
                payload.len() + line.len() + logo.as_ref().map_or(0, String::len),
                0,
            )?;
            channels.push(Channel {
                id: Uuid::now_v7(),
                key: hash.finish().as_ref().to_vec(),
                guide,
                metadata,
                stream: line.into(),
                logo,
            });
            if channels.len() > limits::CHANNELS {
                return Err(ImportError::Limit);
            }
        }
    }
    if pending.is_some() {
        return Err(ImportError::Format);
    }
    Ok(channels)
}

fn attributes(info: &str) -> Result<(BTreeMap<String, String>, String), ImportError> {
    let mut quote = false;
    let comma = info
        .char_indices()
        .find_map(|(i, c)| {
            if c == '"' {
                quote = !quote;
            }
            (c == ',' && !quote).then_some(i)
        })
        .ok_or(ImportError::Format)?;
    let name = info[comma + 1..].trim().to_owned();
    limits::text(&name, limits::RECORD)?;
    let head = info[..comma].trim();
    let duration_end = head.find(char::is_whitespace).unwrap_or(head.len());
    head[..duration_end]
        .parse::<i64>()
        .map_err(|_| ImportError::Format)?;
    let mut rest = head[duration_end..].trim_start();
    let mut attrs = BTreeMap::new();
    while !rest.is_empty() {
        let end = rest.find('=').ok_or(ImportError::Format)?;
        let key = &rest[..end];
        if key.is_empty()
            || !key
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
        {
            return Err(ImportError::Format);
        }
        rest = rest[end + 1..]
            .strip_prefix('"')
            .ok_or(ImportError::Format)?;
        let end = rest.find('"').ok_or(ImportError::Format)?;
        let value = &rest[..end];
        limits::text(value, limits::RECORD)?;
        if attrs.insert(key.to_owned(), value.to_owned()).is_some() || attrs.len() > 32 {
            return Err(ImportError::Format);
        }
        rest = &rest[end + 1..];
        if !rest.is_empty() && !rest.starts_with(char::is_whitespace) {
            return Err(ImportError::Format);
        }
        rest = rest.trim_start();
    }
    Ok((attrs, name))
}

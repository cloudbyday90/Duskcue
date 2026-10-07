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

use crate::domains::playback::PlaybackError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Publication {
    Vod,
    Event,
}

#[derive(Debug, PartialEq, Eq)]
pub(super) struct References {
    pub initialization: String,
    pub segment: String,
    pub publication: Publication,
    pub ended: bool,
}

pub(super) fn parse(content: &str) -> Result<Option<References>, PlaybackError> {
    if !content.ends_with(['\n', '\r']) {
        return Ok(None);
    }
    let mut lines = content
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty());
    if lines.next() != Some("#EXTM3U") {
        return Err(invalid("invalid HLS startup playlist header"));
    }
    let mut initialization = None;
    let mut first_segment = None;
    let mut publication = None;
    let mut target_duration = false;
    let mut segment_pending = false;
    let mut ended = false;
    for line in lines {
        if line.len() > 1024 {
            return Err(invalid("oversized HLS startup playlist line"));
        }
        if let Some(value) = line.strip_prefix("#EXT-X-PLAYLIST-TYPE:") {
            let value = match value {
                "VOD" => Publication::Vod,
                "EVENT" => Publication::Event,
                _ => return Err(invalid("unsupported HLS startup playlist type")),
            };
            if publication.replace(value).is_some() {
                return Err(invalid("duplicate HLS startup playlist type"));
            }
        } else if let Some(value) = line.strip_prefix("#EXT-X-TARGETDURATION:") {
            if !value
                .parse::<u64>()
                .is_ok_and(|duration| duration > 0 && duration <= 86400)
                || target_duration
            {
                return Err(invalid("invalid HLS startup target duration"));
            }
            target_duration = true;
        } else if line.starts_with("#EXT-X-MAP:") {
            let filename = line
                .strip_prefix("#EXT-X-MAP:URI=\"")
                .and_then(|value| value.strip_suffix('"'))
                .filter(|value| *value == "init.mp4")
                .ok_or_else(|| invalid("unowned or unsupported HLS initialization reference"))?;
            if initialization.replace(filename.to_string()).is_some() {
                return Err(invalid("duplicate HLS startup initialization reference"));
            }
        } else if let Some(value) = line.strip_prefix("#EXTINF:") {
            let duration = value
                .split_once(',')
                .map(|(duration, _)| duration)
                .and_then(|duration| duration.parse::<f64>().ok());
            if ended
                || segment_pending
                || !duration.is_some_and(|duration| duration.is_finite() && duration > 0.0)
            {
                return Err(invalid("invalid HLS startup segment duration"));
            }
            segment_pending = true;
        } else if line == "#EXT-X-ENDLIST" {
            if ended || segment_pending {
                return Ok(None);
            }
            ended = true;
        } else if line.contains("URI=")
            || line.starts_with("#EXT-X-STREAM-INF:")
            || line.starts_with("#EXT-X-BYTERANGE:")
        {
            return Err(invalid("unsupported HLS startup resource reference"));
        } else if !line.starts_with('#') {
            if !segment_pending || ended || !segment_name(line) {
                return Err(invalid("unowned or invalid HLS startup segment reference"));
            }
            segment_pending = false;
            if first_segment.is_none() {
                first_segment = Some(line.to_string());
            }
        }
    }
    if segment_pending || !target_duration {
        return Ok(None);
    }
    let (Some(initialization), Some(segment), Some(publication)) =
        (initialization, first_segment, publication)
    else {
        return Ok(None);
    };
    if publication == Publication::Vod && !ended {
        return Ok(None);
    }
    Ok(Some(References {
        initialization,
        segment,
        publication,
        ended,
    }))
}

pub(super) fn segment_name(filename: &str) -> bool {
    filename.len() <= 64
        && filename
            .strip_prefix("seg_")
            .and_then(|value| value.strip_suffix(".m4s"))
            .is_some_and(|number| {
                !number.is_empty() && number.bytes().all(|byte| byte.is_ascii_digit())
            })
}

fn invalid(message: &'static str) -> PlaybackError {
    PlaybackError::FfmpegFailed(message.into())
}

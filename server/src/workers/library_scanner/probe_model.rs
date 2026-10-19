// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Deserialize)]
pub(super) struct FfprobeFormat {
    pub format_name: Option<String>,
    pub duration: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct FfprobeDisposition {
    pub forced: Option<i64>,
    pub hearing_impaired: Option<i64>,
    pub default: Option<i64>,
    pub visual_impaired: Option<i64>,
    pub captions: Option<i64>,
    pub descriptions: Option<i64>,
    pub attached_pic: Option<i64>,
    pub timed_thumbnails: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub(super) struct FfprobeStream {
    pub index: Option<i64>,
    pub codec_type: Option<String>,
    pub codec_name: Option<String>,
    pub codec_tag_string: Option<String>,
    pub extradata: Option<String>,
    pub extradata_size: Option<usize>,
    pub profile: Option<String>,
    pub level: Option<i64>,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub bit_rate: Option<String>,
    pub pix_fmt: Option<String>,
    pub bits_per_raw_sample: Option<Value>,
    pub field_order: Option<String>,
    pub sample_aspect_ratio: Option<String>,
    pub display_aspect_ratio: Option<String>,
    pub color_transfer: Option<String>,
    pub color_primaries: Option<String>,
    pub color_space: Option<String>,
    pub color_range: Option<String>,
    pub chroma_location: Option<String>,
    pub avg_frame_rate: Option<String>,
    pub r_frame_rate: Option<String>,
    pub channels: Option<i32>,
    pub sample_rate: Option<String>,
    pub channel_layout: Option<String>,
    pub disposition: Option<FfprobeDisposition>,
    pub tags: Option<HashMap<String, String>>,
    pub side_data_list: Option<Vec<Value>>,
}

impl FfprobeStream {
    pub(super) fn moving_video(&self) -> bool {
        self.codec_type.as_deref() == Some("video")
            && !self.disposition.as_ref().is_some_and(|flags| {
                flags.attached_pic == Some(1) || flags.timed_thumbnails == Some(1)
            })
    }

    pub(super) fn stream_index(&self) -> Option<i32> {
        self.index
            .and_then(|index| i32::try_from(index).ok())
            .filter(|index| *index >= 0)
    }

    pub(super) fn language(&self) -> String {
        self.tags
            .as_ref()
            .and_then(|tags| tags.get("language"))
            .cloned()
            .unwrap_or_else(|| "und".into())
    }

    pub(super) fn title(&self) -> Option<&str> {
        self.tags.as_ref()?.get("title").map(String::as_str)
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub(super) struct FfprobeChapter {
    id: Option<i64>,
    start_time: Option<String>,
    end_time: Option<String>,
    tags: Option<HashMap<String, String>>,
}

#[derive(Debug, Deserialize)]
pub(super) struct FfprobeOutput {
    pub format: Option<FfprobeFormat>,
    pub streams: Option<Vec<FfprobeStream>>,
    pub chapters: Option<Vec<FfprobeChapter>>,
    pub error: Option<Value>,
}

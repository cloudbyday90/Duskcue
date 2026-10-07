// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use serde_json::Value;

pub(crate) fn default_audio_stream_index(streams: &Value) -> Option<i32> {
    let audio = streams.get("audio")?.as_array()?;
    let valid = |stream: &&Value| {
        stream
            .get("index")
            .and_then(Value::as_i64)
            .and_then(|index| i32::try_from(index).ok())
            .filter(|index| *index >= 0)
    };
    let is_default = |stream: &&Value| match stream
        .get("disposition")
        .and_then(|disposition| disposition.get("default"))
        .and_then(Value::as_i64)
    {
        Some(1) => true,
        Some(0) => false,
        _ => stream
            .get("is_default")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    };
    audio
        .iter()
        .filter(is_default)
        .filter_map(|stream| valid(&stream))
        .min()
        .or_else(|| audio.iter().filter_map(|stream| valid(&stream)).min())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn honors_actual_default_and_preserves_zero_first_stream_fallback() {
        let streams = json!({"audio": [{"index": 0, "disposition": {"default": 0}}, {"index": 3, "disposition": {"default": 1}}]});
        assert_eq!(default_audio_stream_index(&streams), Some(3));
        assert_eq!(
            default_audio_stream_index(&json!({"audio": [{"index": 3}, {"index": 0}]})),
            Some(0)
        );
    }

    #[test]
    fn does_not_invent_an_index_from_malformed_or_absent_stream_metadata() {
        for streams in [
            json!({}),
            json!({"audio": []}),
            json!({"audio": [{"index": -1}, {"index": "1"}, {"index": 2147483648i64}]}),
        ] {
            assert_eq!(default_audio_stream_index(&streams), None);
        }
    }

    #[test]
    fn structured_false_overrides_conflicting_legacy_default() {
        let streams = json!({"audio": [{"index": 1, "is_default": true, "disposition": {"default": 0}}, {"index": 2, "is_default": true}]});
        assert_eq!(default_audio_stream_index(&streams), Some(2));
    }
}

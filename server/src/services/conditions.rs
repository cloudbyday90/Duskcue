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

//! Media condition/filter evaluation — evaluates JSONB filter rules against media
//! item metadata to determine whether an overlay definition or smart collection
//! applies to a given item.
//!
//! Shared between overlay definitions (see [METADATA_OVERLAYS.md](../../docs/design/METADATA_OVERLAYS.md)
//! §Conditions) and smart collections/playlists (see [COLLECTIONS.md](../../docs/design/COLLECTIONS.md)
//! §Smart Filter Syntax). The condition JSONB schema supports nested `and`/`or`
//! logical operators with leaf rules testing media-item fields against literal
//! values via 8 comparison operators.
//!
//! ## Condition schema
//!
//! ```json
//! {
//!   "operator": "and",
//!   "rules": [
//!     { "field": "video_resolution", "op": "eq", "value": "4K" },
//!     { "field": "audio_codec", "op": "in", "values": ["TrueHD", "DTS-HD MA"] },
//!     { "field": "audio_channels", "op": "gte", "value": 6 },
//!     { "field": "has_dolby_vision", "op": "exists", "value": true },
//!     { "operator": "or", "rules": [ ... ] }
//!   ]
//! }
//! ```
//!
//! Empty conditions `{}` or `null` mean "apply to all items" (always match).
//!
//! ## Evaluation semantics
//!
//! - Text comparisons (`eq`, `neq`, `in`) are **case-insensitive** — admin-facing
//!   values like `"4k"` match DB-stored `"4K"`.
//! - Numeric comparisons (`gt`, `gte`, `lt`, `lte`) require numeric fields
//!   (`critic_rating`, `audio_channels`) and numeric JSON values.
//! - The `exists` operator checks field presence: `"value": true` → field must
//!   be present/non-null; `"value": false` → field must be absent/null.
//! - The `matches` operator compiles the JSON string value as a regex and tests
//!   the field text. Case-insensitive via `(?i)` in the pattern if desired.
//! - Malformed rules log a warning and evaluate to `false` (overlay not applied).
//!   Structural validation at create/update time surfaces `OVERLAY_002` to the API.
//!
//! ## Design
//!
//! Pure, stateless, synchronous. No DB, no `AppState`, no async. The domain layer
//! builds a [`MediaFilterContext`] from DB queries and calls [`evaluate`]. This
//! follows the established service-module pattern (`decision_engine.rs`,
//! `segments.rs`, `storyboards.rs`).

use regex::Regex;
use serde_json::Value;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Context
// ---------------------------------------------------------------------------

/// All media-item metadata fields that conditions can test.
///
/// Built by the domain layer from `media_items`, `media_files`, `genres`, and
/// metadata JSONB. `Option` fields reflect nullable DB columns; `bool` fields
/// are always present (derived).
#[derive(Debug, Clone, Default)]
pub struct MediaFilterContext {
    pub media_type: String,
    pub library_id: Option<Uuid>,
    pub content_rating: Option<String>,
    pub critic_rating: Option<f64>,
    pub genres: Vec<String>,

    pub video_resolution: Option<String>,
    pub video_codec: Option<String>,
    pub video_dynamic_range: Option<String>,
    pub audio_codec: Option<String>,
    pub audio_channels: Option<i32>,
    pub container_format: Option<String>,
    pub has_dolby_vision: bool,
    pub has_multiple_versions: bool,
    pub edition: Option<String>,

    pub original_language: Option<String>,
    pub streaming_on: Vec<String>,
}

// ---------------------------------------------------------------------------
// Public entry points
// ---------------------------------------------------------------------------

/// Evaluate a condition JSONB against a media-item context.
///
/// Returns `true` if the conditions match (overlay should apply). Empty/null
/// conditions match all items. Malformed conditions log a warning and return
/// `false` so the overlay is safely skipped.
pub fn evaluate(conditions: &Value, ctx: &MediaFilterContext) -> bool {
    match conditions {
        Value::Null => true,
        Value::Bool(b) => *b,
        Value::Object(map) if map.is_empty() => true,
        Value::Object(_) => evaluate_group(conditions, ctx),
        _ => {
            tracing::warn!("conditions JSONB is not an object, treating as match-all");
            true
        }
    }
}

/// Validate the structural integrity of a condition JSONB without evaluating it.
///
/// Called by the overlay create/update handlers to surface `OVERLAY_002` for
/// malformed conditions before persistence. Does not check field names against
/// the supported-fields table (unknown fields simply never match at evaluation
/// time); only verifies the recursive `operator`/`rules`/`field`/`op` structure.
pub fn validate_structure(conditions: &Value) -> Result<(), ConditionError> {
    match conditions {
        Value::Null | Value::Bool(_) => Ok(()),
        Value::Object(map) if map.is_empty() => Ok(()),
        Value::Object(_) => validate_group(conditions),
        _ => Err(ConditionError::InvalidStructure(
            "conditions must be a JSON object".into(),
        )),
    }
}

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

#[derive(Debug, thiserror::Error)]
pub enum ConditionError {
    #[error("invalid condition structure: {0}")]
    InvalidStructure(String),

    #[error("missing required key `{0}` in rule")]
    MissingKey(String),

    #[error("invalid operator: {0}")]
    InvalidOperator(String),
}

// ---------------------------------------------------------------------------
// Recursive evaluator
// ---------------------------------------------------------------------------

fn evaluate_group(group: &Value, ctx: &MediaFilterContext) -> bool {
    let Some(obj) = group.as_object() else {
        return true;
    };

    let operator = obj
        .get("operator")
        .and_then(|v| v.as_str())
        .unwrap_or("and");

    let Some(rules) = obj.get("rules").and_then(|v| v.as_array()) else {
        return true;
    };

    if rules.is_empty() {
        return true;
    }

    match operator {
        "and" => rules.iter().all(|r| evaluate_node(r, ctx)),
        "or" => rules.iter().any(|r| evaluate_node(r, ctx)),
        other => {
            tracing::warn!(operator = other, "unknown logical operator, defaulting to 'and'");
            rules.iter().all(|r| evaluate_node(r, ctx))
        }
    }
}

fn evaluate_node(node: &Value, ctx: &MediaFilterContext) -> bool {
    let Some(obj) = node.as_object() else {
        return false;
    };

    if obj.contains_key("operator") {
        return evaluate_group(node, ctx);
    }

    evaluate_leaf(obj, ctx)
}

fn evaluate_leaf(obj: &serde_json::Map<String, Value>, ctx: &MediaFilterContext) -> bool {
    let Some(field) = obj.get("field").and_then(|v| v.as_str()) else {
        return false;
    };
    let Some(op) = obj.get("op").and_then(|v| v.as_str()) else {
        return false;
    };

    match op {
        "eq" => compare_eq(field, obj.get("value"), ctx),
        "neq" => !compare_eq(field, obj.get("value"), ctx),
        "in" => {
            let Some(values) = obj.get("values").and_then(|v| v.as_array()) else {
                return false;
            };
            values.iter().any(|v| compare_eq(field, Some(v), ctx))
        }
        "gt" | "gte" | "lt" | "lte" => compare_numeric(field, op, obj.get("value"), ctx),
        "exists" => {
            let want = obj.get("value").and_then(|v| v.as_bool()).unwrap_or(true);
            check_exists(field, want, ctx)
        }
        "matches" => {
            let Some(pattern) = obj.get("value").and_then(|v| v.as_str()) else {
                return false;
            };
            match_regex(field, pattern, ctx)
        }
        other => {
            tracing::warn!(op = other, field = field, "unknown comparison operator");
            false
        }
    }
}

// ---------------------------------------------------------------------------
// Comparison helpers
// ---------------------------------------------------------------------------

fn compare_eq(field: &str, value: Option<&Value>, ctx: &MediaFilterContext) -> bool {
    let Some(value) = value else {
        return false;
    };

    match field {
        "genre" => value
            .as_str()
            .map(|v| ctx.genres.iter().any(|g| eq_ignore_case(g, v)))
            .unwrap_or(false),

        "streaming_on" => value
            .as_str()
            .map(|v| ctx.streaming_on.iter().any(|s| eq_ignore_case(s, v)))
            .unwrap_or(false),

        "library_id" => {
            let Some(target) = value.as_str() else {
                return false;
            };
            ctx.library_id
                .map(|id| id.to_string() == target)
                .unwrap_or(false)
        }

        "has_dolby_vision" => value.as_bool().map(|v| v == ctx.has_dolby_vision).unwrap_or(false),

        "has_multiple_versions" => {
            value.as_bool().map(|v| v == ctx.has_multiple_versions).unwrap_or(false)
        }

        "audio_channels" => {
            let Some(target) = json_to_i64(value) else {
                return false;
            };
            ctx.audio_channels.map(|c| c as i64 == target).unwrap_or(false)
        }

        "critic_rating" | "critic_rating_above" => {
            let Some(target) = json_to_f64(value) else {
                return false;
            };
            ctx.critic_rating.map(|r| (r - target).abs() < f64::EPSILON).unwrap_or(false)
        }

        _ => {
            let Some(ctx_text) = text_field(field, ctx) else {
                return false;
            };
            let Some(val_text) = value.as_str() else {
                return false;
            };
            eq_ignore_case(ctx_text, val_text)
        }
    }
}

fn compare_numeric(field: &str, op: &str, value: Option<&Value>, ctx: &MediaFilterContext) -> bool {
    let Some(value) = value else {
        return false;
    };

    let (ctx_num, target) = match field {
        "audio_channels" => {
            let Some(t) = json_to_i64(value) else {
                return false;
            };
            match ctx.audio_channels {
                Some(c) => (c as f64, t as f64),
                None => return false,
            }
        }
        "critic_rating" | "critic_rating_above" => {
            let Some(t) = json_to_f64(value) else {
                return false;
            };
            match ctx.critic_rating {
                Some(r) => (r, t),
                None => return false,
            }
        }
        _ => {
            tracing::warn!(field = field, "numeric comparison on non-numeric field");
            return false;
        }
    };

    match op {
        "gt" => ctx_num > target,
        "gte" => ctx_num >= target,
        "lt" => ctx_num < target,
        "lte" => ctx_num <= target,
        _ => false,
    }
}

fn check_exists(field: &str, want_exists: bool, ctx: &MediaFilterContext) -> bool {
    let present = match field {
        "video_resolution" => ctx.video_resolution.is_some(),
        "video_codec" => ctx.video_codec.is_some(),
        "video_dynamic_range" => ctx.video_dynamic_range.is_some(),
        "audio_codec" => ctx.audio_codec.is_some(),
        "audio_channels" => ctx.audio_channels.is_some(),
        "container_format" => ctx.container_format.is_some(),
        "content_rating" => ctx.content_rating.is_some(),
        "critic_rating" | "critic_rating_above" => ctx.critic_rating.is_some(),
        "original_language" => ctx.original_language.is_some(),
        "edition" => ctx.edition.is_some(),
        "media_type" => !ctx.media_type.is_empty(),
        "library_id" => ctx.library_id.is_some(),
        "genre" => !ctx.genres.is_empty(),
        "streaming_on" => !ctx.streaming_on.is_empty(),
        "has_dolby_vision" => ctx.has_dolby_vision,
        "has_multiple_versions" => ctx.has_multiple_versions,
        _ => false,
    };

    present == want_exists
}

fn match_regex(field: &str, pattern: &str, ctx: &MediaFilterContext) -> bool {
    let Some(text) = text_field(field, ctx) else {
        return false;
    };
    match Regex::new(pattern) {
        Ok(re) => re.is_match(text),
        Err(e) => {
            tracing::warn!(field = field, pattern = pattern, error = %e, "invalid regex in condition");
            false
        }
    }
}

// ---------------------------------------------------------------------------
// Field accessors
// ---------------------------------------------------------------------------

fn text_field<'a>(field: &str, ctx: &'a MediaFilterContext) -> Option<&'a str> {
    match field {
        "video_resolution" => ctx.video_resolution.as_deref(),
        "video_codec" => ctx.video_codec.as_deref(),
        "video_dynamic_range" => ctx.video_dynamic_range.as_deref(),
        "audio_codec" => ctx.audio_codec.as_deref(),
        "container_format" => ctx.container_format.as_deref(),
        "content_rating" => ctx.content_rating.as_deref(),
        "media_type" => Some(&ctx.media_type),
        "original_language" => ctx.original_language.as_deref(),
        "edition" => ctx.edition.as_deref(),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Structural validation (without evaluation)
// ---------------------------------------------------------------------------

fn validate_group(group: &Value) -> Result<(), ConditionError> {
    let obj = group
        .as_object()
        .ok_or_else(|| ConditionError::InvalidStructure("rule group must be an object".into()))?;

    let operator = obj
        .get("operator")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ConditionError::MissingKey("operator".into()))?;

    if operator != "and" && operator != "or" {
        return Err(ConditionError::InvalidOperator(operator.into()));
    }

    let rules = obj
        .get("rules")
        .and_then(|v| v.as_array())
        .ok_or_else(|| ConditionError::MissingKey("rules".into()))?;

    for rule in rules {
        validate_node(rule)?;
    }

    Ok(())
}

fn validate_node(node: &Value) -> Result<(), ConditionError> {
    let obj = node
        .as_object()
        .ok_or_else(|| ConditionError::InvalidStructure("rule must be an object".into()))?;

    if obj.contains_key("operator") {
        return validate_group(node);
    }

    validate_leaf_structure(obj)
}

fn validate_leaf_structure(obj: &serde_json::Map<String, Value>) -> Result<(), ConditionError> {
    let _field = obj
        .get("field")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ConditionError::MissingKey("field".into()))?;

    let op = obj
        .get("op")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ConditionError::MissingKey("op".into()))?;

    const VALID_OPS: &[&str] = &["eq", "neq", "in", "gt", "gte", "lt", "lte", "exists", "matches"];
    if !VALID_OPS.contains(&op) {
        return Err(ConditionError::InvalidOperator(op.into()));
    }

    if op == "in" {
        if !obj.contains_key("values") {
            return Err(ConditionError::MissingKey("values".into()));
        }
    } else if !obj.contains_key("value") {
        return Err(ConditionError::MissingKey("value".into()));
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Utilities
// ---------------------------------------------------------------------------

fn eq_ignore_case(a: &str, b: &str) -> bool {
    a.eq_ignore_ascii_case(b)
}

fn json_to_f64(v: &Value) -> Option<f64> {
    match v {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.parse().ok(),
        _ => None,
    }
}

fn json_to_i64(v: &Value) -> Option<i64> {
    match v {
        Value::Number(n) => n.as_i64().or_else(|| n.as_f64().map(|f| f as i64)),
        Value::String(s) => s.parse().ok(),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn sample_ctx() -> MediaFilterContext {
        MediaFilterContext {
            media_type: "movie".into(),
            library_id: Some(Uuid::parse_str("0190a5c0-0000-7000-8000-000000000001").unwrap()),
            content_rating: Some("R".into()),
            critic_rating: Some(8.5),
            genres: vec!["Action".into(), "Sci-Fi".into()],
            video_resolution: Some("4K".into()),
            video_codec: Some("HEVC".into()),
            video_dynamic_range: Some("hdr10".into()),
            audio_codec: Some("TrueHD".into()),
            audio_channels: Some(8),
            container_format: Some("MKV".into()),
            has_dolby_vision: true,
            has_multiple_versions: true,
            edition: Some("Extended".into()),
            original_language: Some("en".into()),
            streaming_on: vec!["netflix".into()],
        }
    }

    fn evaluate_json(cond: Value, ctx: &MediaFilterContext) -> bool {
        evaluate(&cond, ctx)
    }

    // ---- empty / null conditions ----

    #[test]
    fn null_conditions_match_all() {
        assert!(evaluate_json(Value::Null, &sample_ctx()));
    }

    #[test]
    fn empty_object_conditions_match_all() {
        assert!(evaluate_json(json!({}), &sample_ctx()));
    }

    #[test]
    fn bool_true_matches() {
        assert!(evaluate_json(json!(true), &sample_ctx()));
    }

    #[test]
    fn bool_false_does_not_match() {
        assert!(!evaluate_json(json!(false), &sample_ctx()));
    }

    // ---- eq operator ----

    #[test]
    fn eq_text_match_case_insensitive() {
        let cond = json!({"operator": "and", "rules": [{"field": "video_resolution", "op": "eq", "value": "4k"}]});
        assert!(evaluate_json(cond, &sample_ctx()));
    }

    #[test]
    fn eq_text_no_match() {
        let cond = json!({"operator": "and", "rules": [{"field": "video_codec", "op": "eq", "value": "H.264"}]});
        assert!(!evaluate_json(cond, &sample_ctx()));
    }

    #[test]
    fn eq_media_type() {
        let cond = json!({"operator": "and", "rules": [{"field": "media_type", "op": "eq", "value": "movie"}]});
        assert!(evaluate_json(cond, &sample_ctx()));
    }

    #[test]
    fn eq_audio_channels_integer() {
        let cond = json!({"operator": "and", "rules": [{"field": "audio_channels", "op": "eq", "value": 8}]});
        assert!(evaluate_json(cond, &sample_ctx()));
    }

    #[test]
    fn eq_audio_channels_no_match() {
        let cond = json!({"operator": "and", "rules": [{"field": "audio_channels", "op": "eq", "value": 2}]});
        assert!(!evaluate_json(cond, &sample_ctx()));
    }

    #[test]
    fn eq_critic_rating_numeric() {
        let cond = json!({"operator": "and", "rules": [{"field": "critic_rating", "op": "eq", "value": 8.5}]});
        assert!(evaluate_json(cond, &sample_ctx()));
    }

    #[test]
    fn eq_boolean_field_true() {
        let cond = json!({"operator": "and", "rules": [{"field": "has_dolby_vision", "op": "eq", "value": true}]});
        assert!(evaluate_json(cond, &sample_ctx()));
    }

    #[test]
    fn eq_boolean_field_false_no_match() {
        let cond = json!({"operator": "and", "rules": [{"field": "has_dolby_vision", "op": "eq", "value": false}]});
        assert!(!evaluate_json(cond, &sample_ctx()));
    }

    #[test]
    fn eq_genre_membership() {
        let cond = json!({"operator": "and", "rules": [{"field": "genre", "op": "eq", "value": "action"}]});
        assert!(evaluate_json(cond, &sample_ctx()));
    }

    #[test]
    fn eq_genre_no_membership() {
        let cond = json!({"operator": "and", "rules": [{"field": "genre", "op": "eq", "value": "horror"}]});
        assert!(!evaluate_json(cond, &sample_ctx()));
    }

    #[test]
    fn eq_streaming_on() {
        let cond = json!({"operator": "and", "rules": [{"field": "streaming_on", "op": "eq", "value": "netflix"}]});
        assert!(evaluate_json(cond, &sample_ctx()));
    }

    #[test]
    fn eq_library_id() {
        let cond = json!({"operator": "and", "rules": [{"field": "library_id", "op": "eq", "value": "0190a5c0-0000-7000-8000-000000000001"}]});
        assert!(evaluate_json(cond, &sample_ctx()));
    }

    // ---- neq operator ----

    #[test]
    fn neq_text_no_match_returns_true() {
        let cond = json!({"operator": "and", "rules": [{"field": "video_codec", "op": "neq", "value": "H.264"}]});
        assert!(evaluate_json(cond, &sample_ctx()));
    }

    #[test]
    fn neq_text_match_returns_false() {
        let cond = json!({"operator": "and", "rules": [{"field": "video_codec", "op": "neq", "value": "hevc"}]});
        assert!(!evaluate_json(cond, &sample_ctx()));
    }

    // ---- in operator ----

    #[test]
    fn in_list_match() {
        let cond = json!({"operator": "and", "rules": [{"field": "audio_codec", "op": "in", "values": ["TrueHD", "DTS-HD MA"]}]});
        assert!(evaluate_json(cond, &sample_ctx()));
    }

    #[test]
    fn in_list_no_match() {
        let cond = json!({"operator": "and", "rules": [{"field": "audio_codec", "op": "in", "values": ["AAC", "AC3"]}]});
        assert!(!evaluate_json(cond, &sample_ctx()));
    }

    #[test]
    fn in_list_case_insensitive() {
        let cond = json!({"operator": "and", "rules": [{"field": "audio_codec", "op": "in", "values": ["truehd"]}]});
        assert!(evaluate_json(cond, &sample_ctx()));
    }

    #[test]
    fn in_list_empty_values_no_match() {
        let cond = json!({"operator": "and", "rules": [{"field": "audio_codec", "op": "in", "values": []}]});
        assert!(!evaluate_json(cond, &sample_ctx()));
    }

    // ---- numeric operators ----

    #[test]
    fn gte_numeric_match() {
        let cond = json!({"operator": "and", "rules": [{"field": "audio_channels", "op": "gte", "value": 6}]});
        assert!(evaluate_json(cond, &sample_ctx()));
    }

    #[test]
    fn gte_numeric_no_match() {
        let cond = json!({"operator": "and", "rules": [{"field": "audio_channels", "op": "gte", "value": 10}]});
        assert!(!evaluate_json(cond, &sample_ctx()));
    }

    #[test]
    fn lt_numeric_match() {
        let cond = json!({"operator": "and", "rules": [{"field": "audio_channels", "op": "lt", "value": 10}]});
        assert!(evaluate_json(cond, &sample_ctx()));
    }

    #[test]
    fn critic_rating_gte() {
        let cond = json!({"operator": "and", "rules": [{"field": "critic_rating", "op": "gte", "value": 8.0}]});
        assert!(evaluate_json(cond, &sample_ctx()));
    }

    #[test]
    fn critic_rating_above_lt_no_match() {
        let cond = json!({"operator": "and", "rules": [{"field": "critic_rating_above", "op": "lt", "value": 8.0}]});
        assert!(!evaluate_json(cond, &sample_ctx()));
    }

    #[test]
    fn numeric_comparison_on_text_field_returns_false() {
        let cond = json!({"operator": "and", "rules": [{"field": "video_codec", "op": "gte", "value": 5}]});
        assert!(!evaluate_json(cond, &sample_ctx()));
    }

    #[test]
    fn numeric_comparison_null_field_returns_false() {
        let ctx = MediaFilterContext {
            audio_channels: None,
            ..sample_ctx()
        };
        let cond = json!({"operator": "and", "rules": [{"field": "audio_channels", "op": "gte", "value": 6}]});
        assert!(!evaluate_json(cond, &ctx));
    }

    #[test]
    fn numeric_value_as_string_parses() {
        let cond = json!({"operator": "and", "rules": [{"field": "audio_channels", "op": "eq", "value": "8"}]});
        assert!(evaluate_json(cond, &sample_ctx()));
    }

    // ---- exists operator ----

    #[test]
    fn exists_true_for_present_field() {
        let cond = json!({"operator": "and", "rules": [{"field": "critic_rating", "op": "exists", "value": true}]});
        assert!(evaluate_json(cond, &sample_ctx()));
    }

    #[test]
    fn exists_false_for_present_field_no_match() {
        let cond = json!({"operator": "and", "rules": [{"field": "critic_rating", "op": "exists", "value": false}]});
        assert!(!evaluate_json(cond, &sample_ctx()));
    }

    #[test]
    fn exists_true_for_absent_field_no_match() {
        let ctx = MediaFilterContext {
            content_rating: None,
            ..sample_ctx()
        };
        let cond = json!({"operator": "and", "rules": [{"field": "content_rating", "op": "exists", "value": true}]});
        assert!(!evaluate_json(cond, &ctx));
    }

    #[test]
    fn exists_false_for_absent_field_matches() {
        let ctx = MediaFilterContext {
            content_rating: None,
            ..sample_ctx()
        };
        let cond = json!({"operator": "and", "rules": [{"field": "content_rating", "op": "exists", "value": false}]});
        assert!(evaluate_json(cond, &ctx));
    }

    #[test]
    fn exists_boolean_field_true() {
        let cond = json!({"operator": "and", "rules": [{"field": "has_dolby_vision", "op": "exists", "value": true}]});
        assert!(evaluate_json(cond, &sample_ctx()));
    }

    #[test]
    fn exists_defaults_to_true_without_value() {
        let cond = json!({"operator": "and", "rules": [{"field": "has_dolby_vision", "op": "exists"}]});
        assert!(evaluate_json(cond, &sample_ctx()));
    }

    // ---- matches (regex) operator ----

    #[test]
    fn matches_regex_case_sensitive() {
        let cond = json!({"operator": "and", "rules": [{"field": "edition", "op": "matches", "value": "^Ext"}]});
        assert!(evaluate_json(cond, &sample_ctx()));
    }

    #[test]
    fn matches_regex_case_insensitive_flag() {
        let cond = json!({"operator": "and", "rules": [{"field": "edition", "op": "matches", "value": "(?i)^ext"}]});
        assert!(evaluate_json(cond, &sample_ctx()));
    }

    #[test]
    fn matches_regex_no_match() {
        let cond = json!({"operator": "and", "rules": [{"field": "edition", "op": "matches", "value": "^Remux"}]});
        assert!(!evaluate_json(cond, &sample_ctx()));
    }

    #[test]
    fn matches_invalid_regex_returns_false() {
        let cond = json!({"operator": "and", "rules": [{"field": "edition", "op": "matches", "value": "[invalid"}]});
        assert!(!evaluate_json(cond, &sample_ctx()));
    }

    // ---- logical operators ----

    #[test]
    fn and_all_true() {
        let cond = json!({"operator": "and", "rules": [
            {"field": "video_resolution", "op": "eq", "value": "4K"},
            {"field": "video_codec", "op": "eq", "value": "HEVC"}
        ]});
        assert!(evaluate_json(cond, &sample_ctx()));
    }

    #[test]
    fn and_one_false() {
        let cond = json!({"operator": "and", "rules": [
            {"field": "video_resolution", "op": "eq", "value": "4K"},
            {"field": "video_codec", "op": "eq", "value": "H.264"}
        ]});
        assert!(!evaluate_json(cond, &sample_ctx()));
    }

    #[test]
    fn or_any_true() {
        let cond = json!({"operator": "or", "rules": [
            {"field": "video_codec", "op": "eq", "value": "H.264"},
            {"field": "video_codec", "op": "eq", "value": "HEVC"}
        ]});
        assert!(evaluate_json(cond, &sample_ctx()));
    }

    #[test]
    fn or_all_false() {
        let cond = json!({"operator": "or", "rules": [
            {"field": "video_codec", "op": "eq", "value": "H.264"},
            {"field": "video_codec", "op": "eq", "value": "AV1"}
        ]});
        assert!(!evaluate_json(cond, &sample_ctx()));
    }

    // ---- nested groups ----

    #[test]
    fn nested_or_inside_and() {
        let cond = json!({"operator": "and", "rules": [
            {"field": "video_resolution", "op": "eq", "value": "4K"},
            {"operator": "or", "rules": [
                {"field": "video_dynamic_range", "op": "eq", "value": "hdr10"},
                {"field": "video_dynamic_range", "op": "eq", "value": "dolby_vision_p7"}
            ]}
        ]});
        assert!(evaluate_json(cond, &sample_ctx()));
    }

    #[test]
    fn nested_and_inside_or() {
        let cond = json!({"operator": "or", "rules": [
            {"field": "video_codec", "op": "eq", "value": "AV1"},
            {"operator": "and", "rules": [
                {"field": "video_resolution", "op": "eq", "value": "4K"},
                {"field": "has_dolby_vision", "op": "eq", "value": true}
            ]}
        ]});
        assert!(evaluate_json(cond, &sample_ctx()));
    }

    #[test]
    fn deeply_nested_groups() {
        let cond = json!({"operator": "and", "rules": [
            {"operator": "or", "rules": [
                {"operator": "and", "rules": [
                    {"field": "video_resolution", "op": "eq", "value": "4K"},
                    {"field": "video_dynamic_range", "op": "neq", "value": "sdr"}
                ]}
            ]}
        ]});
        assert!(evaluate_json(cond, &sample_ctx()));
    }

    // ---- empty rules ----

    #[test]
    fn empty_rules_array_matches_all() {
        let cond = json!({"operator": "and", "rules": []});
        assert!(evaluate_json(cond, &sample_ctx()));
    }

    // ---- malformed conditions ----

    #[test]
    fn missing_field_key_returns_false() {
        let cond = json!({"operator": "and", "rules": [{"op": "eq", "value": "4K"}]});
        assert!(!evaluate_json(cond, &sample_ctx()));
    }

    #[test]
    fn missing_op_key_returns_false() {
        let cond = json!({"operator": "and", "rules": [{"field": "video_resolution", "value": "4K"}]});
        assert!(!evaluate_json(cond, &sample_ctx()));
    }

    #[test]
    fn unknown_field_returns_false() {
        let cond = json!({"operator": "and", "rules": [{"field": "nonexistent_field", "op": "eq", "value": "x"}]});
        assert!(!evaluate_json(cond, &sample_ctx()));
    }

    #[test]
    fn unknown_operator_returns_false() {
        let cond = json!({"operator": "and", "rules": [{"field": "video_codec", "op": "frobnicate", "value": "HEVC"}]});
        assert!(!evaluate_json(cond, &sample_ctx()));
    }

    #[test]
    fn unknown_logical_operator_defaults_to_and() {
        let cond = json!({"operator": "xor", "rules": [
            {"field": "video_resolution", "op": "eq", "value": "4K"}
        ]});
        assert!(evaluate_json(cond, &sample_ctx()));
    }

    // ---- validation ----

    #[test]
    fn validate_valid_structure() {
        let cond = json!({"operator": "and", "rules": [
            {"field": "video_resolution", "op": "eq", "value": "4K"},
            {"operator": "or", "rules": [{"field": "genre", "op": "in", "values": ["Action"]}]}
        ]});
        assert!(validate_structure(&cond).is_ok());
    }

    #[test]
    fn validate_empty_ok() {
        assert!(validate_structure(&json!({})).is_ok());
        assert!(validate_structure(&Value::Null).is_ok());
    }

    #[test]
    fn validate_missing_operator() {
        let cond = json!({"rules": [{"field": "x", "op": "eq", "value": "y"}]});
        assert!(validate_structure(&cond).is_err());
    }

    #[test]
    fn validate_missing_rules() {
        let cond = json!({"operator": "and"});
        assert!(validate_structure(&cond).is_err());
    }

    #[test]
    fn validate_missing_field_in_leaf() {
        let cond = json!({"operator": "and", "rules": [{"op": "eq", "value": "x"}]});
        assert!(validate_structure(&cond).is_err());
    }

    #[test]
    fn validate_invalid_op() {
        let cond = json!({"operator": "and", "rules": [{"field": "x", "op": "invalidop", "value": "y"}]});
        assert!(validate_structure(&cond).is_err());
    }

    #[test]
    fn validate_in_requires_values() {
        let cond = json!({"operator": "and", "rules": [{"field": "x", "op": "in", "value": "y"}]});
        assert!(validate_structure(&cond).is_err());
    }

    #[test]
    fn validate_in_with_values_ok() {
        let cond = json!({"operator": "and", "rules": [{"field": "x", "op": "in", "values": ["y"]}]});
        assert!(validate_structure(&cond).is_ok());
    }

    #[test]
    fn validate_non_object() {
        assert!(validate_structure(&json!("string")).is_err());
        assert!(validate_structure(&json!(42)).is_err());
    }

    // ---- default context (empty) ----

    #[test]
    fn default_context_no_fields() {
        let ctx = MediaFilterContext::default();
        let cond = json!({"operator": "and", "rules": [{"field": "video_resolution", "op": "eq", "value": "4K"}]});
        assert!(!evaluate_json(cond, &ctx));
    }

    #[test]
    fn default_context_exists_false_matches() {
        let ctx = MediaFilterContext::default();
        let cond = json!({"operator": "and", "rules": [{"field": "critic_rating", "op": "exists", "value": false}]});
        assert!(evaluate_json(cond, &ctx));
    }
}

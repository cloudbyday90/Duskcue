// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use serde::{Deserialize, Serialize};
use uuid::Uuid;
use validator::Validate;

use super::types::{DirectFallbackResponse, StartPlaybackRequest};

#[derive(Debug, Clone, Deserialize, Validate)]
pub struct PreparePlaybackRequest {
    pub capability_protocol: Option<u8>,
    #[serde(flatten)]
    #[validate(nested)]
    pub selection: StartPlaybackRequest,
}

#[derive(Debug, Clone, Validate)]
pub struct SourceCapabilitiesRequest {
    pub version: u8,
    #[validate(length(max = 64))]
    pub source_revision: String,
    #[validate(nested)]
    pub observations: Vec<CapabilityObservationRequest>,
}

impl<'de> Deserialize<'de> for SourceCapabilitiesRequest {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = serde_json::Value::deserialize(deserializer)?;
        if value.get("version").and_then(serde_json::Value::as_u64) != Some(1) {
            return Ok(Self {
                version: 0,
                source_revision: String::new(),
                observations: Vec::new(),
            });
        }
        if value
            .get("observations")
            .and_then(serde_json::Value::as_array)
            .is_some_and(|values| values.len() > 3)
            || value
                .get("source_revision")
                .and_then(serde_json::Value::as_str)
                .is_some_and(|value| value.len() > 64)
        {
            return Err(serde::de::Error::custom(
                "source capability report exceeds its bounds",
            ));
        }
        #[derive(Deserialize)]
        struct Supported {
            source_revision: String,
            observations: Vec<CapabilityObservationRequest>,
        }
        let report: Supported = serde_json::from_value(value).map_err(serde::de::Error::custom)?;
        Ok(Self {
            version: 1,
            source_revision: report.source_revision,
            observations: report.observations,
        })
    }
}

#[derive(Debug, Clone, Deserialize, Validate)]
pub struct CapabilityObservationRequest {
    #[validate(length(max = 32))]
    pub candidate_id: String,
    #[validate(length(max = 16))]
    pub status: String,
    pub supported: Option<bool>,
    pub smooth: Option<bool>,
    pub power_efficient: Option<bool>,
    pub delivery_supported: Option<bool>,
}

impl CapabilityObservationRequest {
    pub(super) fn admitted(&self) -> bool {
        self.status == "reported"
            && self.supported == Some(true)
            && self.smooth == Some(true)
            && self.delivery_supported == Some(true)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SourceVideoConfigurationResponse {
    #[serde(rename = "contentType")]
    pub content_type: String,
    pub width: u32,
    pub height: u32,
    pub bitrate: u64,
    pub framerate: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SourceAudioConfigurationResponse {
    #[serde(rename = "contentType")]
    pub content_type: String,
    pub channels: String,
    pub bitrate: u64,
    pub samplerate: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SourceConfigurationResponse {
    #[serde(rename = "type")]
    pub media_type: &'static str,
    pub video: SourceVideoConfigurationResponse,
    pub audio: SourceAudioConfigurationResponse,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SourceCandidateResponse {
    pub id: &'static str,
    pub delivery: &'static str,
    pub configuration: SourceConfigurationResponse,
}

#[derive(Debug, Clone, Serialize)]
pub struct PreparePlaybackResponse {
    pub protocol: u8,
    pub source_revision: String,
    pub candidates: Vec<SourceCandidateResponse>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SourceRouteResponse {
    pub candidate_id: String,
    pub source_revision: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct CopyFallbackResponse {
    pub generation: Uuid,
    #[serde(flatten)]
    pub descriptor: DirectFallbackResponse,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CopyFallbackRequest {
    pub generation: Uuid,
    pub supported: bool,
    pub smooth: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn absent_and_unknown_versions_preserve_legacy_request_admission() {
        let legacy: StartPlaybackRequest =
            serde_json::from_value(json!({"media_item_id":Uuid::now_v7()})).unwrap();
        assert!(legacy.source_capabilities.is_none());
        let unknown: StartPlaybackRequest =
            serde_json::from_value(json!({"media_item_id":Uuid::now_v7(),
            "source_capabilities":{"version":2,"different_future_shape":true}}))
            .unwrap();
        assert_ne!(unknown.source_capabilities.unwrap().version, 1);
    }

    #[test]
    fn supported_reports_have_finite_bounded_typed_observations() {
        let observation = json!({"candidate_id":"native_file","status":"reported","supported":true,"smooth":true,"delivery_supported":true});
        assert!(serde_json::from_value::<SourceCapabilitiesRequest>(json!({"version":1,"source_revision":"a".repeat(64),
            "observations":[observation.clone(),observation.clone(),observation.clone(),observation]})).is_err());
        assert!(
            serde_json::from_value::<SourceCapabilitiesRequest>(
                json!({"version":1,"source_revision":"a".repeat(64),
            "observations":[{"candidate_id":"native_file","status":"reported","supported":"true"}]})
            )
            .is_err()
        );
    }
}

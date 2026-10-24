// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use axum::extract::rejection::JsonRejection;
use axum::http::StatusCode;
use thiserror::Error;

use crate::error::{AppError, FieldError};

use super::VALIDATE_CONNECTION_PATH;

#[derive(Debug, Error)]
pub enum LiveTvError {
    #[error("invalid Live TV connection configuration")]
    InvalidConfiguration { fields: Vec<FieldError> },
}

impl LiveTvError {
    pub fn fields(&self) -> &[FieldError] {
        match self {
            Self::InvalidConfiguration { fields } => fields,
        }
    }
}

impl From<LiveTvError> for AppError {
    fn from(error: LiveTvError) -> Self {
        let LiveTvError::InvalidConfiguration { fields } = error;
        Self::Validation {
            errors: fields,
            instance: Some(VALIDATE_CONNECTION_PATH.into()),
        }
    }
}

pub(super) fn body_rejection(rejection: JsonRejection) -> AppError {
    match rejection.status() {
        StatusCode::PAYLOAD_TOO_LARGE => AppError::PayloadTooLarge,
        StatusCode::UNSUPPORTED_MEDIA_TYPE => AppError::UnsupportedMediaType,
        StatusCode::UNPROCESSABLE_ENTITY => AppError::Validation {
            errors: vec![FieldError {
                field: "body".into(),
                code: "invalid_body".into(),
                message: "Connection configuration must contain only playlist_url and guide_url string fields".into(),
            }],
            instance: Some(VALIDATE_CONNECTION_PATH.into()),
        },
        _ => AppError::BadRequest("Connection configuration must be valid JSON".into()),
    }
}

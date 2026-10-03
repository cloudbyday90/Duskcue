use thiserror::Error;

use crate::error::{AppError, FieldError};

#[derive(Error, Debug)]
pub enum FireError {
    #[error("invalid Fire catalog request: {0}")]
    InvalidRequest(&'static str),
    #[error("Fire catalog identity or revision conflict")]
    Conflict,
    #[error("Fire catalog mapping is unavailable")]
    Unavailable,
    #[error(transparent)]
    Tv(#[from] super::super::error::TvError),
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

impl From<FireError> for AppError {
    fn from(error: FireError) -> Self {
        match error {
            FireError::InvalidRequest(message) => Self::Validation {
                errors: vec![FieldError {
                    field: "catalog".into(),
                    code: "invalid".into(),
                    message: message.into(),
                }],
                instance: None,
            },
            FireError::Conflict => {
                Self::Conflict("Fire catalog identity or revision conflict".into())
            }
            FireError::Unavailable => Self::NotFound("Fire catalog mapping is unavailable".into()),
            FireError::Tv(error) => Self::Tv(error),
            FireError::Database(error) => Self::Internal(anyhow::Error::new(error)),
        }
    }
}

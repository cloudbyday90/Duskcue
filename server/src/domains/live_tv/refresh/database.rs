// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::error::RefreshError;
use crate::{
    domains::live_tv::feed::imports::{ImportError, storage},
    error::AppError,
};
use sqlx::{PgConnection, PgPool};
use std::ops::AsyncFnOnce;

pub(super) async fn write<T, F>(pool: &PgPool, work: F) -> Result<T, AppError>
where
    F: for<'a> AsyncFnOnce(&'a mut PgConnection) -> Result<T, AppError>,
{
    let mut rejection = None;
    let result = storage::write(pool, async |conn| match work(conn).await {
        Ok(value) => Ok(value),
        Err(error) => {
            rejection = Some(error);
            Err(ImportError::Authority)
        }
    })
    .await;
    match result {
        Ok(value) => Ok(value),
        Err(ImportError::Authority) if rejection.is_some() => {
            Err(rejection.ok_or(RefreshError::Unavailable)?)
        }
        Err(ImportError::Indeterminate) => Err(RefreshError::Uncertain.into()),
        Err(ImportError::Cleanup) => Err(RefreshError::Cleanup.into()),
        _ => Err(RefreshError::Unavailable.into()),
    }
}

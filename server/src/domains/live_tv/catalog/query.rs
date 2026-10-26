// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::{error::CatalogError, limits, types::Window};
use chrono::{DateTime, Utc};
use uuid::Uuid;

pub(crate) fn limit(limit: Option<u32>) -> Result<u32, CatalogError> {
    let value = limit.unwrap_or(20);
    if (1..=100).contains(&value) {
        Ok(value)
    } else {
        Err(CatalogError::Invalid)
    }
}

pub(crate) fn id(id: Uuid) -> Result<Uuid, CatalogError> {
    if id.get_version_num() == 7 {
        Ok(id)
    } else {
        Err(CatalogError::Invalid)
    }
}

pub(crate) fn cursor(value: Option<&str>) -> Result<(), CatalogError> {
    if value.is_some_and(|v| v.is_empty() || v.len() > limits::CURSOR) {
        Err(CatalogError::Invalid)
    } else {
        Ok(())
    }
}

pub(crate) fn window(from: &str, to: &str) -> Result<Window, CatalogError> {
    let instant = |value: &str| {
        if value.len() > 64
            || value.split_once('.').is_some_and(|(_, fraction)| {
                fraction.bytes().take_while(|b| b.is_ascii_digit()).count() > 6
            })
        {
            return Err(CatalogError::Invalid);
        }
        let time = DateTime::parse_from_rfc3339(value).map_err(|_| CatalogError::Invalid)?;
        if time.timestamp_subsec_nanos() >= 1_000_000_000
            || time.timestamp_subsec_nanos() % 1000 != 0
        {
            return Err(CatalogError::Invalid);
        }
        Ok(time.with_timezone(&Utc))
    };
    let result = Window {
        from: instant(from)?,
        to: instant(to)?,
    };
    if result.from >= result.to || result.to - result.from > chrono::Duration::hours(24) {
        return Err(CatalogError::Invalid);
    }
    Ok(result)
}

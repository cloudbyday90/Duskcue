// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use chrono::{DateTime, NaiveDateTime, Utc};

pub(super) const UNCERTAIN: i32 = 1;
pub(super) const CONFLICT: i32 = 2;
pub(super) const OVERLAP: i32 = 4;
pub(super) const CLUMP: i32 = 8;

pub(super) fn instant(raw: Option<&str>) -> Option<DateTime<Utc>> {
    let mut words = raw?.split_whitespace();
    let date = words.next()?;
    if date.len() != 14 || !date.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let zone = words.next();
    if words.next().is_some() {
        return None;
    }
    let instant = match zone {
        None => NaiveDateTime::parse_from_str(date, "%Y%m%d%H%M%S")
            .ok()
            .map(|v| v.and_utc()),
        Some(zone)
            if zone.len() == 5
                && matches!(zone.as_bytes()[0], b'+' | b'-')
                && zone.as_bytes()[1..].iter().all(u8::is_ascii_digit) =>
        {
            DateTime::parse_from_str(&format!("{date} {zone}"), "%Y%m%d%H%M%S %z")
                .ok()
                .map(|v| v.with_timezone(&Utc))
        }
        _ => None,
    };
    instant.filter(|value| value.timestamp_subsec_nanos() < 1_000_000_000)
}

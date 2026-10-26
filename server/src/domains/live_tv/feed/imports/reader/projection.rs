// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::super::{limits, model::SealedRow};
use super::stored::*;
use crate::domains::live_tv::catalog::{error::CatalogError, types::*};

fn text(value: &str, max: usize) -> Result<(), CatalogError> {
    limits::text(value, max).map_err(|_| CatalogError::Unavailable)
}

fn optional(value: Option<&str>) -> Result<(), CatalogError> {
    if let Some(value) = value {
        text(value, 4096)?;
    }
    Ok(())
}

fn variants(values: Vec<TextMetadata>, max: usize) -> Result<Vec<TextResponse>, CatalogError> {
    if values.len() > max {
        return Err(CatalogError::Unavailable);
    }
    values
        .into_iter()
        .map(|value| {
            text(&value.value, 4096)?;
            if let Some(language) = &value.lang {
                text(language, 64)?;
            }
            Ok(TextResponse {
                language: value.lang,
                text: value.value,
            })
        })
        .collect()
}

pub(super) fn channel(
    id: uuid::Uuid,
    raw: &str,
    guide: Option<GuideResponse>,
) -> Result<ChannelResponse, CatalogError> {
    let value: ChannelMetadata =
        serde_json::from_str(raw).map_err(|_| CatalogError::Unavailable)?;
    text(&value.name, 4096)?;
    for value in [&value.tvg_name, &value.group, &value.number] {
        optional(value.as_deref())?;
    }
    Ok(ChannelResponse {
        id,
        name: value.name,
        tvg_name: value.tvg_name,
        group: value.group,
        number: value.number,
        guide,
    })
}

pub(super) fn guide(raw: &str) -> Result<GuideResponse, CatalogError> {
    let value: GuideMetadata = serde_json::from_str(raw).map_err(|_| CatalogError::Unavailable)?;
    if value.declarations.len() > 8 {
        return Err(CatalogError::Unavailable);
    }
    let declarations = value
        .declarations
        .into_iter()
        .map(|value| {
            Ok(DeclarationResponse {
                display_names: variants(value.display_names, 4)?,
            })
        })
        .collect::<Result<_, CatalogError>>()?;
    Ok(GuideResponse {
        undeclared: value.undeclared,
        ambiguous: value.ambiguous,
        declarations,
    })
}

pub(super) fn programme(row: &SealedRow, raw: &str) -> Result<ProgrammeResponse, CatalogError> {
    let value: ProgrammeMetadata =
        serde_json::from_str(raw).map_err(|_| CatalogError::Unavailable)?;
    if !(0..=15).contains(&row.flags) || value.ratings.len() > 8 {
        return Err(CatalogError::Unavailable);
    }
    for value in [&value.start, &value.stop, &value.clumpidx] {
        optional(value.as_deref())?;
    }
    let ratings = value
        .ratings
        .into_iter()
        .map(|value| {
            optional(value.system.as_deref())?;
            text(&value.value, 4096)?;
            Ok(RatingResponse {
                system: value.system,
                value: value.value,
            })
        })
        .collect::<Result<_, CatalogError>>()?;
    Ok(ProgrammeResponse {
        id: row.id,
        start_raw: value.start,
        stop_raw: value.stop,
        clump_index_raw: value.clumpidx,
        starts_at: row.start,
        stops_at: row.stop,
        titles: variants(value.titles, 4)?,
        subtitles: variants(value.subtitles, 4)?,
        descriptions: variants(value.descriptions, 4)?,
        categories: variants(value.categories, 8)?,
        ratings,
        quality: QualityResponse {
            uncertain: row.flags & 1 != 0,
            conflict: row.flags & 2 != 0,
            overlap: row.flags & 4 != 0,
            clump: row.flags & 8 != 0,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unexposed_supported_fields_never_escape_projection() {
        let row = super::super::super::codec::empty(uuid::Uuid::now_v7());
        let raw = r#"{"start":null,"stop":null,"display-name":[{"value":"private-hidden"}],"icon":"private-url","title":[{"lang":"unknown_tag","value":"<b>plain</b>"}],"ratings":[{"system":null,"value":""}]}"#;
        let result = programme(&row, raw).unwrap();
        let encoded = serde_json::to_string(&result).unwrap();
        assert!(!encoded.contains("private-hidden") && !encoded.contains("private-url"));
        assert!(encoded.contains("unknown_tag") && encoded.contains("<b>plain</b>"));
        assert_eq!(result.ratings[0].system, None);
        assert_eq!(result.starts_at, None);
    }

    #[test]
    fn projection_rejects_exposed_type_and_count_confusion() {
        assert!(channel(uuid::Uuid::now_v7(), r#"{"name":7}"#, None).is_err());
        assert!(guide(r#"{"undeclared":true,"ambiguous":false,"declarations":[{"display-name":[{"lang":7,"value":"a"}]}]}"#).is_err());
        let row = super::super::super::codec::empty(uuid::Uuid::now_v7());
        let raw=serde_json::json!({"title":[{"value":"a"},{"value":"b"},{"value":"c"},{"value":"d"},{"value":"e"}]}).to_string();
        assert!(programme(&row, &raw).is_err());
    }
}

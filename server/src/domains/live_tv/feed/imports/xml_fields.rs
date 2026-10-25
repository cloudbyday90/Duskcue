// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::{
    ImportError, limits,
    model::{Guide, Programme},
    time,
};
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashSet};
use uuid::Uuid;

pub(super) struct Record {
    pub key: String,
    pub programme: bool,
    pub metadata: Value,
}

impl Record {
    pub fn new(name: &str, attrs: &BTreeMap<String, String>) -> Result<Self, ImportError> {
        let programme = name == "programme";
        let key = attrs
            .get(if programme { "channel" } else { "id" })
            .filter(|s| !s.is_empty())
            .ok_or(ImportError::Format)?
            .clone();
        limits::text(&key, 512)?;
        let metadata = if programme {
            json!({"start":attrs.get("start"),"stop":attrs.get("stop"),"clumpidx":attrs.get("clumpidx")})
        } else {
            json!({})
        };
        limits::json(&metadata)?;
        Ok(Self {
            key,
            programme,
            metadata,
        })
    }

    pub fn field(&mut self, name: &str, value: Value, max: usize) -> Result<(), ImportError> {
        let object = self.metadata.as_object_mut().ok_or(ImportError::Format)?;
        let values = object
            .entry(name)
            .or_insert_with(|| json!([]))
            .as_array_mut()
            .ok_or(ImportError::Format)?;
        if !values.contains(&value) {
            if values.len() >= max {
                return Err(ImportError::Limit);
            }
            values.push(value);
        }
        limits::json(&self.metadata)?;
        Ok(())
    }
}

pub(super) fn save(
    record: Record,
    guides: &mut BTreeMap<String, Guide>,
    dedup: &mut BTreeMap<String, HashSet<Vec<u8>>>,
    work: &mut limits::Work,
) -> Result<(), ImportError> {
    let Some(guide) = guides.get_mut(&record.key) else {
        return Ok(());
    };
    if !record.programme {
        let declarations = guide.metadata["declarations"]
            .as_array_mut()
            .ok_or(ImportError::Format)?;
        if !declarations.contains(&record.metadata) {
            if declarations.len() >= 8 {
                return Err(ImportError::Limit);
            }
            declarations.push(record.metadata);
        }
        guide.metadata["undeclared"] = json!(false);
        guide.metadata["ambiguous"] = json!(
            guide.metadata["declarations"]
                .as_array()
                .is_some_and(|v| v.len() > 1)
        );
        work.payload(limits::json(&guide.metadata)?.len(), 0)?;
    } else {
        let payload = limits::json(&record.metadata)?;
        let hash = ring::digest::digest(&ring::digest::SHA256, payload.as_bytes());
        if !dedup
            .entry(record.key)
            .or_default()
            .insert(hash.as_ref().to_vec())
        {
            return Ok(());
        }
        let mut start = time::instant(record.metadata["start"].as_str());
        let mut stop = time::instant(record.metadata["stop"].as_str());
        let mut flags = 0;
        if start.is_none() || stop.is_none() {
            flags |= time::UNCERTAIN;
        }
        if start.zip(stop).is_some_and(|(a, b)| a >= b) {
            start = None;
            stop = None;
            flags |= time::UNCERTAIN;
        }
        if !record.metadata["clumpidx"].is_null() {
            flags |= time::CLUMP;
        }
        work.payload(payload.len(), 0)?;
        guide.programmes.push(Programme {
            id: Uuid::now_v7(),
            start,
            stop,
            flags,
            metadata: record.metadata,
        });
    }
    Ok(())
}

pub(super) fn ambiguity(guide: &mut Guide, work: &limits::Work) -> Result<(), ImportError> {
    guide.programmes.sort_by_key(|p| (p.start, p.stop));
    let mut group = 0;
    let mut max_end = None;
    let mut component = 0;
    for i in 0..guide.programmes.len() {
        work.check()?;
        if i == 0 || guide.programmes[i].start != guide.programmes[i - 1].start {
            group = i;
        } else if guide.programmes[i].start.is_some() {
            guide.programmes[group].flags |= time::CONFLICT;
            guide.programmes[i].flags |= time::CONFLICT;
        }
        let p = &guide.programmes[i];
        if let Some((start, stop)) = p.start.zip(p.stop) {
            if max_end.is_some_and(|end| start < end) {
                guide.programmes[component].flags |= time::OVERLAP;
                guide.programmes[i].flags |= time::OVERLAP;
                max_end = max_end.map(|v| std::cmp::max(v, stop));
            } else {
                component = i;
                max_end = Some(stop);
            }
        }
    }
    Ok(())
}

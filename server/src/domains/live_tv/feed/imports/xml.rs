// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::{
    ImportError,
    limits::{self, Work},
    model::{Channel, Guide},
    xml_fields::{self, Record},
};
use quick_xml_import::{
    Reader, XmlVersion,
    events::{BytesStart, Event},
};
use serde_json::json;
use std::collections::{BTreeMap, HashSet};
use uuid::Uuid;

struct Frame {
    name: String,
    attrs: BTreeMap<String, String>,
    text: String,
    retain: bool,
    preserve: bool,
    lang: Option<String>,
}

pub(super) fn parse(
    input: &str,
    channels: &[Channel],
    work: &mut Work,
) -> Result<BTreeMap<String, Guide>, ImportError> {
    if input.len() > limits::XML {
        return Err(ImportError::Limit);
    }
    let input = input.strip_prefix('\u{feff}').unwrap_or(input);
    let mut guides = BTreeMap::new();
    for key in channels.iter().filter_map(|c| c.guide.as_ref()) {
        guides.entry(key.clone()).or_insert_with(|| Guide {
            id: Uuid::now_v7(),
            metadata: json!({"key":key,"undeclared":true,"ambiguous":false,"declarations":[]}),
            programmes: Vec::new(),
        });
    }
    let mut reader = Reader::from_str(input);
    reader.config_mut().check_comments = true;
    reader.config_mut().expand_empty_elements = true;
    let mut stack: Vec<Frame> = Vec::new();
    let mut record: Option<Record> = None;
    let mut seen_root = false;
    let mut closed_root = false;
    let mut declaration = false;
    let mut events = 0;
    let mut declarations = 0;
    let mut programmes = 0;
    let mut dedup = BTreeMap::<String, HashSet<Vec<u8>>>::new();
    loop {
        work.check()?;
        events += 1;
        if events > limits::EVENTS {
            return Err(ImportError::Limit);
        }
        let before = reader.buffer_position();
        let event = reader.read_event().map_err(|_| ImportError::Format)?;
        if reader.buffer_position().saturating_sub(before) > 65536 {
            return Err(ImportError::Limit);
        }
        match event {
            Event::Start(start) => {
                let name = start.name().as_ref().to_owned();
                if !valid_name(&name) || closed_root || stack.len() >= 32 {
                    return Err(ImportError::Format);
                }
                let attrs = attributes(&start)?;
                if attrs.contains_key("xmlns") {
                    return Err(ImportError::Format);
                }
                if stack.is_empty() {
                    if seen_root || name != "tv" || attrs.contains_key("xmlns") {
                        return Err(ImportError::Format);
                    }
                    seen_root = true;
                }
                let parent = stack.last();
                let preserve = match attrs.get("xml:space").map(String::as_str) {
                    Some("preserve") => true,
                    Some("default") => false,
                    Some(_) => return Err(ImportError::Format),
                    None => parent.is_some_and(|v| v.preserve),
                };
                let lang = attrs
                    .get("lang")
                    .or_else(|| attrs.get("xml:lang"))
                    .cloned()
                    .or_else(|| parent.and_then(|v| v.lang.clone()));
                if let Some(value) = &lang {
                    limits::text(value, 64)?;
                }
                if stack.len() == 1 && matches!(name.as_str(), "channel" | "programme") {
                    if name == "channel" {
                        declarations += 1;
                    } else {
                        programmes += 1;
                    }
                    if declarations > 20_000 || programmes > limits::PROGRAMMES {
                        return Err(ImportError::Limit);
                    }
                    let found = Record::new(&name, &attrs)?;
                    if guides.contains_key(&found.key) {
                        record = Some(found);
                    }
                }
                let retain = record.is_some()
                    && ((stack.len() == 2
                        && matches!(
                            name.as_str(),
                            "display-name" | "title" | "sub-title" | "desc" | "category"
                        ))
                        || (stack.len() == 3
                            && name == "value"
                            && parent.is_some_and(|v| v.name == "rating")));
                if parent.is_some_and(|v| v.retain) {
                    return Err(ImportError::Format);
                }
                stack.push(Frame {
                    name,
                    attrs,
                    text: String::new(),
                    retain,
                    preserve,
                    lang,
                });
            }
            Event::End(end) => {
                let frame = stack.pop().ok_or(ImportError::Format)?;
                if frame.name != end.name().as_ref() {
                    return Err(ImportError::Format);
                }
                if frame.retain {
                    let value = if frame.preserve {
                        frame.text
                    } else {
                        frame
                            .text
                            .split([' ', '\t', '\r', '\n'])
                            .filter(|part| !part.is_empty())
                            .collect::<Vec<_>>()
                            .join(" ")
                    };
                    if let Some(record) = record.as_mut() {
                        if frame.name == "value" {
                            let system = stack.last().and_then(|v| v.attrs.get("system"));
                            record.field("ratings", json!({"system":system,"value":value}), 8)?;
                        } else {
                            let max = if frame.name == "category" { 8 } else { 4 };
                            record.field(
                                &frame.name,
                                json!({"lang":frame.lang,"value":value}),
                                max,
                            )?;
                        }
                    }
                }
                if stack.len() == 1
                    && matches!(frame.name.as_str(), "channel" | "programme")
                    && let Some(record) = record.take()
                {
                    xml_fields::save(record, &mut guides, &mut dedup, work)?;
                }
                if stack.is_empty() {
                    closed_root = true;
                }
            }
            Event::Text(text) => {
                if text.contains("]]>") {
                    return Err(ImportError::Format);
                }
                append(&mut stack, &text.xml10_content())?;
            }
            Event::CData(text) => {
                if stack.is_empty() {
                    return Err(ImportError::Format);
                }
                append(&mut stack, &text.xml10_content())?;
            }
            Event::GeneralRef(reference) => {
                if stack.is_empty() {
                    return Err(ImportError::Format);
                }
                let raw = format!("&{};", &*reference);
                let value =
                    quick_xml_import::escape::unescape(&raw).map_err(|_| ImportError::Format)?;
                append(&mut stack, &value)?;
            }
            Event::Decl(decl) => {
                if seen_root
                    || declaration
                    || before != 0
                    || decl.version().map_err(|_| ImportError::Format)? != "1.0"
                {
                    return Err(ImportError::Format);
                }
                declaration = true;
                if let Some(encoding) = decl.encoding()
                    && !encoding
                        .map_err(|_| ImportError::Format)?
                        .eq_ignore_ascii_case("utf-8")
                {
                    return Err(ImportError::Format);
                }
                decl.standalone()
                    .transpose()
                    .map_err(|_| ImportError::Format)?;
                let raw = BytesStart::from_content(decl.to_string(), 3);
                let attrs = attributes(&raw)?;
                if attrs
                    .keys()
                    .any(|key| !matches!(key.as_str(), "version" | "encoding" | "standalone"))
                    || attrs
                        .get("standalone")
                        .is_some_and(|v| !matches!(v.as_str(), "yes" | "no"))
                {
                    return Err(ImportError::Format);
                }
            }
            Event::DocType(_) => return Err(ImportError::Format),
            Event::Comment(value) => {
                limits::text(&value, 65536)?;
                if value.ends_with('-') {
                    return Err(ImportError::Format);
                }
            }
            Event::PI(value) => {
                limits::text(&value, 65536)?;
                if !valid_name(value.target()) || value.target().eq_ignore_ascii_case("xml") {
                    return Err(ImportError::Format);
                }
            }
            Event::Eof => {
                if !seen_root || !closed_root || !stack.is_empty() {
                    return Err(ImportError::Format);
                }
                break;
            }
            Event::Empty(_) => return Err(ImportError::Format),
        }
    }
    for guide in guides.values_mut() {
        xml_fields::ambiguity(guide, work)?;
    }
    Ok(guides)
}

fn append(stack: &mut [Frame], value: &str) -> Result<(), ImportError> {
    if value.chars().any(|c| !limits::legal(c)) {
        return Err(ImportError::Format);
    }
    if stack.len() == 1 && !value.chars().all(|c| matches!(c, ' ' | '\t' | '\r' | '\n')) {
        return Err(ImportError::Format);
    }
    if let Some(frame) = stack.last_mut() {
        if frame.retain {
            if frame.text.len().saturating_add(value.len()) > limits::RECORD {
                return Err(ImportError::Limit);
            }
            frame.text.push_str(value);
        }
    } else if !value.chars().all(|c| matches!(c, ' ' | '\t' | '\r' | '\n')) {
        return Err(ImportError::Format);
    }
    Ok(())
}

fn attributes(start: &BytesStart<'_>) -> Result<BTreeMap<String, String>, ImportError> {
    let mut values = BTreeMap::new();
    for attr in start.attributes() {
        let attr = attr.map_err(|_| ImportError::Format)?;
        if !valid_name(attr.key.as_ref()) || attr.value.contains('<') {
            return Err(ImportError::Format);
        }
        let value = attr
            .normalized_value(XmlVersion::Implicit1_0)
            .map_err(|_| ImportError::Format)?
            .into_owned();
        if value.chars().any(|c| !limits::legal(c)) {
            return Err(ImportError::Format);
        }
        if values.insert(attr.key.as_ref().to_owned(), value).is_some() {
            return Err(ImportError::Format);
        }
        if values.len() > 64 {
            return Err(ImportError::Limit);
        }
    }
    Ok(values)
}

fn valid_name(value: &str) -> bool {
    let first = |c: char| matches!(c,':'|'_'|'A'..='Z'|'a'..='z'|'\u{c0}'..='\u{d6}'|'\u{d8}'..='\u{f6}'|'\u{f8}'..='\u{2ff}'|'\u{370}'..='\u{37d}'|'\u{37f}'..='\u{1fff}'|'\u{200c}'..='\u{200d}'|'\u{2070}'..='\u{218f}'|'\u{2c00}'..='\u{2fef}'|'\u{3001}'..='\u{d7ff}'|'\u{f900}'..='\u{fdcf}'|'\u{fdf0}'..='\u{fffd}'|'\u{10000}'..='\u{effff}');
    let mut chars = value.chars();
    chars.next().is_some_and(first) && chars.all(|c| {
        first(c)
            || matches!(c,'-'|'.'|'0'..='9'|'\u{b7}'|'\u{300}'..='\u{36f}'|'\u{203f}'..='\u{2040}')
    })
}

// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::error::AssignmentError;
use axum::http::{HeaderMap, header};

pub(crate) enum Condition {
    Create,
    Match(Vec<Vec<u8>>),
}

pub(crate) fn condition(headers: &HeaderMap, create: bool) -> Result<Condition, AssignmentError> {
    let (required, other) = if create {
        (header::IF_NONE_MATCH, header::IF_MATCH)
    } else {
        (header::IF_MATCH, header::IF_NONE_MATCH)
    };
    if headers.contains_key(other) {
        return Err(AssignmentError::InvalidRequest);
    }
    let mut bytes = Vec::new();
    for value in headers.get_all(required) {
        if bytes.len() + value.as_bytes().len() + usize::from(!bytes.is_empty()) > 1024 {
            return Err(AssignmentError::InvalidRequest);
        }
        if !bytes.is_empty() {
            bytes.push(b',');
        }
        bytes.extend_from_slice(value.as_bytes());
    }
    if bytes.is_empty() {
        return Err(AssignmentError::ConditionRequired);
    }
    let bytes = bytes.trim_ascii();
    if create {
        return if bytes == b"*" {
            Ok(Condition::Create)
        } else {
            Err(AssignmentError::InvalidRequest)
        };
    }
    if bytes == b"*" {
        return Err(AssignmentError::ConditionRequired);
    }
    let mut rest = bytes;
    let mut strong = Vec::new();
    let mut parsed = false;
    while !rest.is_empty() {
        rest = rest.trim_ascii_start();
        if rest.first() == Some(&b',') {
            rest = &rest[1..];
            continue;
        }
        let weak = rest.starts_with(b"W/");
        if weak {
            rest = &rest[2..];
        }
        if rest.first() != Some(&b'"') {
            return Err(AssignmentError::InvalidRequest);
        }
        let end = rest[1..]
            .iter()
            .position(|v| *v == b'"')
            .ok_or(AssignmentError::InvalidRequest)?
            + 1;
        if rest[1..end].iter().any(|v| *v < 0x21 || *v == 0x7f) {
            return Err(AssignmentError::InvalidRequest);
        }
        parsed = true;
        if !weak {
            strong.push(rest[..=end].to_vec());
        }
        rest = rest[end + 1..].trim_ascii_start();
        if !rest.is_empty() {
            if rest[0] != b',' {
                return Err(AssignmentError::InvalidRequest);
            }
            rest = &rest[1..];
        }
    }
    if !parsed {
        return Err(AssignmentError::InvalidRequest);
    }
    Ok(Condition::Match(strong))
}

impl Condition {
    pub fn check(&self, current: Option<&str>) -> Result<(), AssignmentError> {
        match (self, current) {
            (Self::Create, None) => Ok(()),
            (Self::Match(tags), Some(current))
                if tags.iter().any(|tag| tag == current.as_bytes()) =>
            {
                Ok(())
            }
            _ => Err(AssignmentError::Stale),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    #[test]
    fn conditional_writes_require_strong_bounded_versions() {
        let mut headers = HeaderMap::new();
        assert!(matches!(
            condition(&headers, false),
            Err(AssignmentError::ConditionRequired)
        ));
        headers.insert(header::IF_MATCH, HeaderValue::from_static("*"));
        assert!(matches!(
            condition(&headers, false),
            Err(AssignmentError::ConditionRequired)
        ));
        headers.insert(header::IF_MATCH, HeaderValue::from_static("W/\"v1\""));
        assert!(matches!(
            condition(&headers, false).unwrap().check(Some("\"v1\"")),
            Err(AssignmentError::Stale)
        ));
        headers.insert(
            header::IF_MATCH,
            HeaderValue::from_static(" , \"comma,value\", W/\"weak\", \"v1\", "),
        );
        let parsed = condition(&headers, false).unwrap();
        assert!(parsed.check(Some("\"v1\"")).is_ok());
        assert!(parsed.check(Some("\"comma,value\"")).is_ok());
        assert!(parsed.check(Some("\"weak\"")).is_err());
        for value in [
            "*,\"v1\"",
            "\"v1\" garbage",
            "\"unclosed",
            "W/v1",
            "\"space inside\"",
        ] {
            headers.insert(header::IF_MATCH, HeaderValue::from_str(value).unwrap());
            assert!(matches!(
                condition(&headers, false),
                Err(AssignmentError::InvalidRequest)
            ));
        }
        headers.insert(
            header::IF_MATCH,
            HeaderValue::from_str(&format!("\"{}\"", "x".repeat(1023))).unwrap(),
        );
        assert!(condition(&headers, false).is_err());
        headers.clear();
        headers.insert(header::IF_NONE_MATCH, HeaderValue::from_static("*"));
        let create = condition(&headers, true).unwrap();
        assert!(create.check(None).is_ok());
        assert!(create.check(Some("\"existing\"")).is_err());
        headers.append(header::IF_NONE_MATCH, HeaderValue::from_static("*"));
        assert!(condition(&headers, true).is_err());
    }
}

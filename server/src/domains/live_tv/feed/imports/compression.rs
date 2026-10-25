// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::{
    ImportError,
    limits::{self, Work},
};
use std::io::Read;

pub(super) fn decode(bytes: Vec<u8>, work: &Work) -> Result<String, ImportError> {
    work.check()?;
    if bytes.len() > limits::GUIDE {
        return Err(ImportError::Limit);
    }
    if bytes.first() == Some(&0x1f) && !bytes.starts_with(&[0x1f, 0x8b]) {
        return Err(ImportError::Format);
    }
    let plain = if bytes.starts_with(&[0x1f, 0x8b]) {
        let mut rest = bytes.as_slice();
        let mut result = Vec::new();
        let mut members = 0;
        while !rest.is_empty() {
            work.check()?;
            members += 1;
            if members > 8 {
                return Err(ImportError::Limit);
            }
            if !rest.starts_with(&[0x1f, 0x8b]) {
                return Err(ImportError::Format);
            }
            let mut decoder = flate2::bufread::GzDecoder::new(rest);
            let mut buffer = [0; 16384];
            loop {
                work.check()?;
                let n = decoder.read(&mut buffer).map_err(|_| ImportError::Format)?;
                if n == 0 {
                    break;
                }
                if result.len().saturating_add(n) > limits::XML {
                    return Err(ImportError::Limit);
                }
                result.extend_from_slice(&buffer[..n]);
            }
            let next = decoder.into_inner();
            if next.len() >= rest.len() {
                return Err(ImportError::Format);
            }
            rest = next;
        }
        result
    } else {
        bytes
    };
    String::from_utf8(plain).map_err(|_| ImportError::Format)
}

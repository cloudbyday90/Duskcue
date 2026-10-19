// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::PlaybackError;

#[derive(Debug, PartialEq, Eq)]
pub struct RangeSpec {
    pub start: u64,
    pub end: u64,
    pub total: u64,
}

impl RangeSpec {
    pub fn parse(header: Option<&str>, size: u64) -> Result<Option<Self>, PlaybackError> {
        let Some(header) = header else {
            return Ok(None);
        };
        let invalid =
            || PlaybackError::InvalidByteRange("expected a satisfiable single byte range".into());
        if header.len() > 128 {
            return Err(invalid());
        }
        let (unit, spec) = header.split_once('=').ok_or_else(invalid)?;
        if !unit.eq_ignore_ascii_case("bytes") {
            return Ok(None);
        }
        if size == 0 {
            return Err(invalid());
        }
        let (first, last) = spec.split_once('-').ok_or_else(invalid)?;
        let number = |value: &str| -> Result<u64, PlaybackError> {
            if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
                return Err(invalid());
            }
            value.parse().map_err(|_| invalid())
        };
        let (start, end) = if first.is_empty() {
            let suffix = number(last)?;
            if suffix == 0 {
                return Err(invalid());
            }
            (size.saturating_sub(suffix), size - 1)
        } else {
            let start = number(first)?;
            let end = if last.is_empty() {
                size - 1
            } else {
                number(last)?
            };
            if start >= size || start > end {
                return Err(invalid());
            }
            (start, end.min(size - 1))
        };
        Ok(Some(Self {
            start,
            end,
            total: size,
        }))
    }

    pub fn content_length(&self) -> u64 {
        self.end - self.start + 1
    }

    pub fn content_range_header(&self) -> String {
        format!("bytes {}-{}/{}", self.start, self.end, self.total)
    }
}

#[cfg(test)]
mod tests {
    use super::RangeSpec;

    #[test]
    fn normalizes_single_ranges_without_expanding_the_representation() {
        for (header, start, end) in [
            ("bytes=0-0", 0, 0),
            ("bytes=3-7", 3, 7),
            ("bytes=3-", 3, 9),
            ("bytes=-3", 7, 9),
            ("bytes=-30", 0, 9),
            ("bytes=3-90", 3, 9),
            ("BYTES=3-7", 3, 7),
        ] {
            let range = RangeSpec::parse(Some(header), 10).unwrap().unwrap();
            assert_eq!(
                range,
                RangeSpec {
                    start,
                    end,
                    total: 10
                }
            );
            assert_eq!(range.content_length(), end - start + 1);
            assert_eq!(
                range.content_range_header(),
                format!("bytes {start}-{end}/10")
            );
        }
    }

    #[test]
    fn rejects_zero_suffix_unsatisfiable_multipart_and_malformed_ranges() {
        for header in [
            "bytes=-0",
            "bytes=10-",
            "bytes=8-3",
            "bytes=-",
            "bytes=",
            "bytes=0-1,4-5",
            "bytes=0--1",
            "bytes=+1-2",
            "bytes= 1-2",
            "bytes=1-2 ",
            "bytes=18446744073709551616-",
        ] {
            assert!(RangeSpec::parse(Some(header), 10).is_err(), "{header}");
        }
    }

    #[test]
    fn handles_empty_and_maximum_sizes_without_arithmetic_overflow() {
        assert_eq!(RangeSpec::parse(None, 0).unwrap(), None);
        assert_eq!(RangeSpec::parse(Some("items=0-1"), 10).unwrap(), None);
        assert_eq!(RangeSpec::parse(Some("items=0-1"), 0).unwrap(), None);
        for header in ["bytes=0-", "bytes=0-0", "bytes=-1"] {
            assert!(RangeSpec::parse(Some(header), 0).is_err());
        }
        let maximum = RangeSpec::parse(Some("bytes=0-"), u64::MAX)
            .unwrap()
            .unwrap();
        assert_eq!(maximum.content_length(), u64::MAX);
        let suffix = RangeSpec::parse(Some("bytes=-1"), u64::MAX)
            .unwrap()
            .unwrap();
        assert_eq!(suffix.start, u64::MAX - 1);
        assert_eq!(suffix.content_length(), 1);
    }
}

// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

fn hex(value: &str, minimum: usize, maximum: usize) -> bool {
    (minimum..=maximum).contains(&value.len()) && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn decimal(value: &str, minimum: u8, maximum: u8) -> bool {
    value
        .parse::<u8>()
        .is_ok_and(|value| (minimum..=maximum).contains(&value))
}

pub(super) fn valid_codec(value: &str, codec: &str) -> bool {
    if value.len() > 128
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'.')
    {
        return false;
    }
    let fields: Vec<_> = value.split('.').collect();
    match codec {
        "h264" => {
            fields.len() == 2
                && ["avc1", "avc3"].contains(&fields[0])
                && hex(fields[1], 6, 6)
                && u8::from_str_radix(&fields[1][..2], 16)
                    .is_ok_and(|value| [66, 77, 88, 100, 110].contains(&value))
                && u8::from_str_radix(&fields[1][2..4], 16).is_ok_and(|value| value & 3 == 0)
                && u8::from_str_radix(&fields[1][4..], 16).is_ok_and(|value| value > 0)
        }
        "hevc" => {
            if !(4..=10).contains(&fields.len())
                || !["hvc1", "hev1"].contains(&fields[0])
                || !["1", "2"].contains(&fields[1])
                || !hex(fields[2], 1, 8)
            {
                return false;
            }
            let tier_level = fields[3];
            tier_level.len() >= 2
                && [b'L', b'H'].contains(&tier_level.as_bytes()[0])
                && decimal(&tier_level[1..], 1, 255)
                && fields[4..].iter().all(|value| hex(value, 2, 2))
        }
        "av1" => {
            if fields.len() != 10
                || fields[0] != "av01"
                || fields[1] != "0"
                || fields[2].len() != 3
                || !["08", "10"].contains(&fields[3])
                || fields[4] != "0"
                || !["110", "111", "112"].contains(&fields[5])
                || !["0", "1"].contains(&fields[9])
            {
                return false;
            }
            let level = &fields[2][..2];
            let tier = fields[2].as_bytes()[2];
            decimal(level, 0, 23)
                && [b'M', b'H'].contains(&tier)
                && (tier == b'M' || decimal(level, 8, 23))
                && fields[6..9]
                    .iter()
                    .all(|value| value.len() == 2 && decimal(value, 1, 22) && *value != "02")
        }
        "aac" => value == "mp4a.40.2",
        "opus" => value == "opus",
        _ => false,
    }
}

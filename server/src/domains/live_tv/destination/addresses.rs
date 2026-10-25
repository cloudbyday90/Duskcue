// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use std::net::IpAddr;

pub(super) fn allowed(ip: IpAddr) -> bool {
    match ip.to_canonical() {
        IpAddr::V4(ip) => {
            let value = u32::from(ip);
            ![
                (0x00000000, 8),
                (0xa9fe0000, 16),
                (0xe0000000, 4),
                (0xf0000000, 4),
                (0xc0000000, 24),
                (0xc0000200, 24),
                (0xc0586300, 24),
                (0xc6120000, 15),
                (0xc6336400, 24),
                (0xcb007100, 24),
                (0x646464c8, 32),
                (0xa83f8110, 32),
            ]
            .iter()
            .any(|&(prefix, bits)| value >> (32 - bits) == prefix >> (32 - bits))
        }
        IpAddr::V6(ip) => {
            let value = u128::from(ip);
            if ip.is_loopback() {
                return true;
            }
            if value == 0xfd000ec2000000000000000000000254 {
                return false;
            }
            if value >> 121 == 0x7e {
                return true;
            }
            value >> 125 == 1
                && ![
                    (0x20010000000000000000000000000000_u128, 23),
                    (0x20010db8000000000000000000000000, 32),
                    (0x20020000000000000000000000000000, 16),
                    (0x3fff0000000000000000000000000000, 20),
                ]
                .iter()
                .any(|&(prefix, bits)| value >> (128 - bits) == prefix >> (128 - bits))
        }
    }
}

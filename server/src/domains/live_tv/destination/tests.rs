// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::*;
use crate::{
    domains::live_tv::assignment::secrets::{self, FeedField},
    services::encryption::{EncryptionKey, context::RecordContext},
};
use uuid::Uuid;

#[test]
fn address_policy_covers_local_public_special_and_mapped_forms() {
    for raw in [
        "127.0.0.1",
        "10.0.0.1",
        "172.16.0.1",
        "192.168.1.1",
        "100.64.0.1",
        "8.8.8.8",
        "::1",
        "fc00::1",
        "fd12::1",
        "2001:4860::8888",
        "::ffff:127.0.0.1",
    ] {
        assert!(
            addresses::allowed(raw.parse().unwrap()),
            "allowed fixture refused: {raw}"
        );
    }
    for raw in [
        "0.1.2.3",
        "169.254.0.1",
        "169.254.169.254",
        "224.0.0.0",
        "239.255.255.255",
        "240.0.0.0",
        "255.255.255.255",
        "192.0.0.9",
        "192.0.2.1",
        "192.88.99.1",
        "198.18.0.1",
        "198.19.255.255",
        "198.51.100.1",
        "203.0.113.1",
        "100.100.100.200",
        "168.63.129.16",
        "::",
        "fe80::1",
        "ff02::1",
        "64:ff9b::1",
        "2001::1",
        "2001:1ff:ffff::1",
        "2001:db8::1",
        "2002::1",
        "3fff:f::1",
        "fd00:ec2::254",
        "::ffff:169.254.169.254",
    ] {
        assert!(
            !addresses::allowed(raw.parse().unwrap()),
            "forbidden fixture allowed: {raw}"
        );
    }
    for (network, bits) in [
        (0x00000000_u32, 8),
        (0xa9fe0000, 16),
        (0xe0000000, 4),
        (0xf0000000, 4),
        (0xc0000000, 24),
        (0xc0000200, 24),
        (0xc0586300, 24),
        (0xc6120000, 15),
        (0xc6336400, 24),
        (0xcb007100, 24),
    ] {
        let end = network | (u32::MAX >> bits);
        assert!(!addresses::allowed(IpAddr::V4(network.into())));
        assert!(!addresses::allowed(IpAddr::V4(end.into())));
    }
}

#[test]
fn approval_matches_origin_and_literal_identity_without_resolving() {
    let url = source("https://FEED.invalid:443/private?token=secret").unwrap();
    assert!(Approval::new("https://feed.invalid/", &["127.0.0.1".into()], &url).is_ok());
    for origin in [
        "http://feed.invalid",
        "https://other.invalid",
        "https://feed.invalid:444",
        "https://feed.invalid/path",
        "https://feed.invalid?token=secret",
        "https://feed.invalid#frag",
        "https://user@feed.invalid",
        "https://feed.invalid:0",
    ] {
        assert!(Approval::new(origin, &["127.0.0.1".into()], &url).is_err());
    }
    let literal = source("http://[::ffff:127.0.0.1]/feed").unwrap();
    assert!(Approval::new("http://[::ffff:127.0.0.1]", &["127.0.0.1".into()], &literal).is_ok());
    assert!(Approval::new("http://[::ffff:127.0.0.1]", &["127.0.0.2".into()], &literal).is_err());
}

#[test]
fn approval_rejects_unbounded_duplicate_and_nonliteral_pins() {
    let url = source("https://feed.invalid/").unwrap();
    for pins in [
        vec![],
        vec!["127.0.0.1".into(); 9],
        vec!["127.0.0.1".into(), "::ffff:127.0.0.1".into()],
        vec!["localhost".into()],
        vec!["127.0.0.1:80".into()],
        vec!["127.0.0.0/8".into()],
        vec!["fe80::1%1".into()],
        vec![" 127.0.0.1".into()],
    ] {
        assert!(Approval::new("https://feed.invalid", &pins, &url).is_err());
    }
}

#[test]
fn approval_encryption_binds_household_assignment_feed_and_purpose() {
    let (key, _) = EncryptionKey::generate();
    let user = Uuid::now_v7();
    let id = Uuid::now_v7();
    let url = source("https://feed.invalid/private?token=secret").unwrap();
    let approval = Approval::new("https://feed.invalid", &["127.0.0.1".into()], &url).unwrap();
    let sealed = approval.seal(&key, user, id, FeedField::Playlist).unwrap();
    assert_ne!(
        sealed,
        approval.seal(&key, user, id, FeedField::Playlist).unwrap()
    );
    assert!(Approval::open(&key, user, id, FeedField::Playlist, &sealed, &url).is_ok());
    assert!(Approval::open(&key, Uuid::now_v7(), id, FeedField::Playlist, &sealed, &url).is_err());
    assert!(
        Approval::open(
            &key,
            user,
            Uuid::now_v7(),
            FeedField::Playlist,
            &sealed,
            &url
        )
        .is_err()
    );
    assert!(Approval::open(&key, user, id, FeedField::Guide, &sealed, &url).is_err());
    assert!(secrets::open(&key, user, id, FeedField::Playlist, &sealed).is_err());
    let other = source("https://other.invalid/").unwrap();
    assert!(Approval::open(&key, user, id, FeedField::Playlist, &sealed, &other).is_err());
    let mut damaged = sealed.into_bytes();
    let i = damaged.len() - 5;
    damaged[i] = if damaged[i] == b'A' { b'B' } else { b'A' };
    assert!(
        Approval::open(
            &key,
            user,
            id,
            FeedField::Playlist,
            std::str::from_utf8(&damaged).unwrap(),
            &url
        )
        .is_err()
    );
    for payload in [
        r#"{"version":2,"origin":"https://feed.invalid","addresses":["127.0.0.1"]}"#,
        r#"{"version":1,"origin":"https://feed.invalid","addresses":["127.0.0.1"],"extra":true}"#,
        r#"{"version":1,"origin":"https://feed.invalid","addresses":["169.254.169.254"]}"#,
    ] {
        let envelope = key
            .seal_record(
                payload,
                RecordContext {
                    namespace: "live-tv-destination",
                    owner: user,
                    record: id,
                    field: "playlist",
                },
            )
            .unwrap();
        assert!(Approval::open(&key, user, id, FeedField::Playlist, &envelope, &url).is_err());
    }
}

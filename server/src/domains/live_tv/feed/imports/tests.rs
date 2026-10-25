// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::*;
use crate::services::encryption::EncryptionKey;
use std::{
    io::Write,
    time::{Duration, Instant},
};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

mod bounds;

fn work() -> limits::Work {
    limits::Work {
        cancel: CancellationToken::new(),
        deadline: Instant::now() + Duration::from_secs(60),
        plaintext: 0,
        ciphertext: 0,
    }
}
fn playlist() -> String {
    format!(
        "#EXTM3U\r\n#EXTINF:-1 tvg-id=\"shared\" group-title=\"News, local\",Channel\r\nhttps://fixture.invalid/prefix/proxy/ts/stream/{}/?token=private\r\n",
        Uuid::from_u128(1)
    )
}
fn channels() -> Vec<model::Channel> {
    m3u::parse(playlist().as_bytes(), Uuid::from_u128(2), &mut work()).unwrap()
}
fn gzip(bytes: &[u8]) -> Vec<u8> {
    let mut e = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
    e.write_all(bytes).unwrap();
    e.finish().unwrap()
}

#[test]
fn identity_is_scoped_stable_and_not_a_guide_id() {
    let assignment = Uuid::now_v7();
    let v4 = playlist().replace(
        &Uuid::from_u128(1).to_string(),
        "a1c42c1d-49ed-4a0e-8eb8-7928d83f82f2",
    );
    assert_eq!(
        m3u::parse(v4.as_bytes(), assignment, &mut work()).unwrap()[0]
            .id
            .get_version_num(),
        7
    );
    let first = m3u::parse(playlist().as_bytes(), assignment, &mut work()).unwrap();
    let rotated = playlist()
        .replace("prefix/", "other/")
        .replace("token=private", "token=rotated");
    let second = m3u::parse(rotated.as_bytes(), assignment, &mut work()).unwrap();
    assert_eq!(first[0].key, second[0].key);
    assert_ne!(first[0].id, second[0].id);
    assert_ne!(
        first[0].key,
        m3u::parse(playlist().as_bytes(), Uuid::now_v7(), &mut work()).unwrap()[0].key
    );
    assert_ne!(
        first[0].key,
        m3u::parse(
            playlist()
                .replace(
                    &Uuid::from_u128(1).to_string(),
                    &Uuid::from_u128(3).to_string()
                )
                .as_bytes(),
            assignment,
            &mut work()
        )
        .unwrap()[0]
            .key
    );
    let shared = format!(
        "{}#EXTINF:-1 tvg-id=\"shared\",Other\nhttp://fixture.invalid/proxy/ts/stream/{}\n",
        playlist(),
        Uuid::from_u128(4)
    );
    assert_eq!(
        m3u::parse(shared.as_bytes(), assignment, &mut work())
            .unwrap()
            .len(),
        2
    );
    assert!(
        m3u::parse(
            format!(
                "{}{}",
                playlist(),
                playlist().trim_start_matches("#EXTM3U\r\n")
            )
            .as_bytes(),
            assignment,
            &mut work()
        )
        .is_err()
    );
}

#[test]
fn playlist_refuses_partial_unsupported_and_ambiguous_syntax() {
    for input in [
        "",
        "<html>error</html>",
        "#EXTM3U \0\n",
        "#EXTM3U\n#EXTINF:-1,Missing\n",
        "#EXTM3U\nhttp://fixture.invalid/stream",
        "#EXTM3U\n#EXT-X-TARGETDURATION:10\n",
        "#EXTM3U\n#EXTINF:-1 tvg-id=\"x\" tvg-id=\"y\",Duplicate\nhttp://fixture.invalid/proxy/ts/stream/00000000-0000-0000-0000-000000000001\n",
    ] {
        assert!(
            m3u::parse(input.as_bytes(), Uuid::now_v7(), &mut work()).is_err(),
            "fixture was accepted"
        );
    }
    assert!(m3u::parse(b"\xff", Uuid::now_v7(), &mut work()).is_err());
    assert!(
        m3u::parse(b"#EXTM3U\n", Uuid::now_v7(), &mut work())
            .unwrap()
            .is_empty()
    );
    assert!(
        m3u::parse("\u{feff}#EXTM3U\n".as_bytes(), Uuid::now_v7(), &mut work())
            .unwrap()
            .is_empty()
    );
}

#[test]
fn xml_preserves_text_language_duplicates_and_guide_separation() {
    let document = "<?xml version=\"1.0\" encoding=\"UTF-8\"?><tv><channel id=\"shared\"><display-name lang=\"en\">A &amp; <![CDATA[B]]>&#x43;</display-name></channel><channel id=\"shared\"><display-name lang=\"en\">A &amp; BC</display-name></channel><programme channel=\"shared\" start=\"20261025120000\" stop=\"20261025130000 +0000\"><title lang=\"ja\">番組</title><rating system=\"unknown\"><value>Unmapped</value></rating><star-rating><value>5/5</value></star-rating></programme><programme channel=\"shared\" start=\"20261025120000\" stop=\"20261025130000 +0000\"><title lang=\"ja\">番組</title><rating system=\"unknown\"><value>Unmapped</value></rating></programme></tv>";
    let guides = xml::parse(document, &channels(), &mut work()).unwrap();
    let guide = &guides["shared"];
    assert_eq!(guide.metadata["declarations"].as_array().unwrap().len(), 1);
    assert_eq!(
        guide.metadata["declarations"][0]["display-name"][0]["value"],
        "A & BC"
    );
    assert_eq!(guide.programmes.len(), 1);
    assert_eq!(guide.programmes[0].flags, 0);
    assert_eq!(guide.programmes[0].metadata["title"][0]["lang"], "ja");
    assert_eq!(
        guide.programmes[0].metadata["ratings"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        guide.programmes[0].metadata["ratings"][0]["value"],
        "Unmapped"
    );
}

#[test]
fn malformed_xml_and_external_resources_are_refused_without_resolution() {
    for input in [
        "",
        "<tv>",
        "<tv></tv><tv/>",
        "<tv/>trailing",
        "<tv><a></tv>",
        "<!DOCTYPE tv SYSTEM 'file:///private'><tv/>",
        "<!DOCTYPE tv [<!ENTITY x 'private'>]><tv>&x;</tv>",
        "<tv>&unknown;</tv>",
        "<tv>&#0;</tv>",
        "<tv><a x='&#1;'/></tv>",
        "<tv><a x='1' x='2'/></tv>",
        "<tv><a x=no/></tv>",
        "<tv><!-- a--b --></tv>",
        "<tv><!--a---></tv>",
        "<tv><1bad/></tv>",
        "<?xml version='1.1'?><tv/>",
        "<?xml version='1.0' encoding='ISO-8859-1'?><tv/>",
        "<tv><![CDATA[bad]]></tv>bad",
        "<tv><a>]]></a></tv>",
        "<tv><?XML bad?></tv>",
    ] {
        assert!(
            xml::parse(input, &channels(), &mut work()).is_err(),
            "malformed fixture accepted: {input}"
        );
    }
    let external = "<tv source-info-url='http://127.0.0.1/private'><channel id='shared'><icon src='file:///private'/><url>https://fixture.invalid/private</url></channel></tv>";
    assert!(xml::parse(external, &channels(), &mut work()).is_ok());
}

#[test]
fn uncertainty_is_preserved_and_exact_duplicates_do_not_overlap() {
    assert_eq!(
        time::instant(Some("20261025120000")),
        time::instant(Some("20261025120000 +0000"))
    );
    for raw in [
        "2026",
        "202610251200",
        "20261025120000 UTC",
        "20261025120000 EST",
        "20261325120000",
        "20261025120000 +2500",
        "20161231235960",
        "20161231235960 +0000",
        "bad",
    ] {
        assert!(time::instant(Some(raw)).is_none());
    }
    let input = "<tv><programme channel='shared' start='2026'><title>Partial</title></programme><programme channel='shared' start='20261025120000' stop='20261025130000'><title>First</title></programme><programme channel='shared' start='20261025120000' stop='20261025130000' clumpidx='0/2'><title>Second</title></programme><programme channel='shared' start='20261025123000' stop='20261025133000'><title>Overlap</title></programme></tv>";
    let result = xml::parse(input, &channels(), &mut work()).unwrap();
    let guide = &result["shared"];
    assert_eq!(guide.metadata["undeclared"], true);
    assert_eq!(guide.programmes.len(), 4);
    assert!(
        guide
            .programmes
            .iter()
            .any(|p| p.flags & time::UNCERTAIN != 0 && p.stop.is_none())
    );
    assert_eq!(
        guide
            .programmes
            .iter()
            .filter(|p| p.flags & time::CONFLICT != 0)
            .count(),
        2
    );
    assert_eq!(
        guide
            .programmes
            .iter()
            .filter(|p| p.flags & time::OVERLAP != 0)
            .count(),
        3
    );
    assert!(guide.programmes.iter().any(|p| p.flags & time::CLUMP != 0));
}

#[test]
fn gzip_requires_complete_members_integrity_and_no_trailing_junk() {
    let input = b"<tv/>";
    let zipped = gzip(input);
    assert_eq!(
        compression::decode(zipped.clone(), &work()).unwrap(),
        "<tv/>"
    );
    assert_eq!(
        compression::decode(input.to_vec(), &work()).unwrap(),
        "<tv/>"
    );
    for end in [1, zipped.len() - 1, zipped.len() - 8] {
        assert!(compression::decode(zipped[..end].to_vec(), &work()).is_err());
    }
    let mut corrupt = zipped.clone();
    let index = corrupt.len() - 8;
    corrupt[index] ^= 1;
    assert!(compression::decode(corrupt, &work()).is_err());
    let mut junk = zipped.clone();
    junk.extend_from_slice(b"junk");
    assert!(compression::decode(junk, &work()).is_err());
    let mut multi = gzip(b"<tv>");
    multi.extend(gzip(b"</tv>"));
    assert_eq!(compression::decode(multi, &work()).unwrap(), "<tv></tv>");
    let mut eight = Vec::new();
    for _ in 0..8 {
        eight.extend(gzip(b""));
    }
    assert_eq!(compression::decode(eight.clone(), &work()).unwrap(), "");
    eight.extend(gzip(b""));
    assert_eq!(
        compression::decode(eight, &work()).unwrap_err(),
        ImportError::Limit
    );
}

#[test]
fn budgets_and_cooperative_cancellation_are_fail_closed() {
    let mut cancelled = work();
    cancelled.cancel.cancel();
    assert!(m3u::parse(playlist().as_bytes(), Uuid::now_v7(), &mut cancelled).is_err());
    assert!(compression::decode(gzip(b"<tv/>"), &cancelled).is_err());
    assert!(xml::parse("<tv/>", &channels(), &mut cancelled).is_err());
    let mut expired = work();
    expired.deadline = Instant::now();
    assert_eq!(expired.check(), Err(ImportError::Deadline));
    let mut budget = work();
    assert!(budget.payload(96 * limits::MIB, 0).is_ok());
    assert_eq!(budget.payload(1, 0), Err(ImportError::Limit));
    let deep = format!("<tv>{}{}</tv>", "<a>".repeat(32), "</a>".repeat(32));
    assert!(xml::parse(&deep, &channels(), &mut work()).is_err());
    let oversized = format!(
        "<tv><channel id='shared'><display-name>{}</display-name></channel></tv>",
        "x".repeat(4097)
    );
    assert_eq!(
        xml::parse(&oversized, &channels(), &mut work()).err(),
        Some(ImportError::Limit)
    );
}

#[test]
fn encrypted_rows_bind_source_epoch_indexed_columns_and_field_purpose() {
    let (key, _) = EncryptionKey::generate();
    let receipt = receipt::Receipt {
        fence: super::super::source::Fence {
            user: Uuid::now_v7(),
            assignment: Uuid::now_v7(),
            generation: 1,
        },
        attempt: Uuid::now_v7(),
        sequence: 1,
        snapshot: Uuid::now_v7(),
        imported_at: chrono::Utc::now(),
        initiator: None,
    };
    let mut row = codec::empty(Uuid::now_v7());
    row.guide = Some(Uuid::now_v7());
    row.flags = 1;
    let sealed = codec::seal(&key, receipt, "programme", &row, "metadata", "private").unwrap();
    assert_eq!(
        codec::open(&key, receipt, "programme", &row, "metadata", &sealed).unwrap(),
        "private"
    );
    assert!(codec::open(&key, receipt, "programme", &row, "stream", &sealed).is_err());
    row.flags = 0;
    assert!(codec::open(&key, receipt, "programme", &row, "metadata", &sealed).is_err());
    row.flags = 1;
    let mut rotated = receipt;
    rotated.fence.generation += 1;
    assert!(codec::open(&key, rotated, "programme", &row, "metadata", &sealed).is_err());
    rotated = receipt;
    rotated.attempt = Uuid::now_v7();
    assert!(codec::open(&key, rotated, "programme", &row, "metadata", &sealed).is_err());
    rotated = receipt;
    rotated.fence.user = Uuid::now_v7();
    assert!(codec::open(&key, rotated, "programme", &row, "metadata", &sealed).is_err());
    rotated = receipt;
    rotated.fence.assignment = Uuid::now_v7();
    assert!(codec::open(&key, rotated, "programme", &row, "metadata", &sealed).is_err());
    rotated = receipt;
    rotated.snapshot = Uuid::now_v7();
    assert!(codec::open(&key, rotated, "programme", &row, "metadata", &sealed).is_err());
    row.start = Some(chrono::Utc::now());
    assert!(codec::open(&key, receipt, "programme", &row, "metadata", &sealed).is_err());
    row.start = None;
    row.guide = Some(Uuid::now_v7());
    assert!(codec::open(&key, receipt, "programme", &row, "metadata", &sealed).is_err());
}

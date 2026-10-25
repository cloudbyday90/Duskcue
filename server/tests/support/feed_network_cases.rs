// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::{
    assignment,
    http::Case,
    peer::{self, Behavior, Peer, Security},
};
use anyhow::Context;
use duskcue::domains::live_tv::{
    assignment::secrets::FeedField,
    feed::{FeedError, FeedOwner},
};
use std::{net::IpAddr, time::Duration};

pub fn owner(case: &Case, fixture: bool) -> anyhow::Result<FeedOwner> {
    Ok(if fixture {
        FeedOwner::qualification_fixture(case.pool.clone(), case.state.encryption_key.clone())?
    } else {
        FeedOwner::new(case.pool.clone(), case.state.encryption_key.clone())?
    })
}

async fn response_case(
    case: &mut Case,
    bytes: Vec<u8>,
    close: bool,
    expected: Result<&[u8], FeedError>,
) -> anyhow::Result<()> {
    let peer = Peer::new(
        IpAddr::from([127, 0, 0, 1]),
        Security::Plain,
        if close {
            Behavior::Close(bytes)
        } else {
            Behavior::Reply(bytes)
        },
    )
    .await?;
    assignment::prepare(case, &peer, "http", "duskcue-feed.invalid").await?;
    let owner = owner(case, true)?;
    let result = owner
        .start(case.owner.user, FeedField::Playlist, 0)?
        .finish()
        .await;
    anyhow::ensure!(
        result.as_deref().map_err(|e| *e) == expected,
        "bounded response result mismatch: actual {:?}, expected {:?}",
        result.as_ref().map(Vec::len),
        expected.map(<[u8]>::len)
    );
    anyhow::ensure!(owner.idle(), "response leaked operation capacity");
    peer.check_request("duskcue-feed.invalid", false)?;
    owner.shutdown().await?;
    peer.finish(true).await
}

pub async fn run(case: &mut Case) -> anyhow::Result<()> {
    for (security, host, fixture, success) in [
        (Security::Tls, "duskcue-feed.invalid", true, true),
        (Security::Tls, "127.0.0.1", true, true),
        (Security::Tls, "wrong-name.invalid", true, false),
        (Security::Tls, "duskcue-feed.invalid", false, false),
        (Security::Expired, "duskcue-feed.invalid", true, false),
    ] {
        let peer = Peer::new(
            IpAddr::from([127, 0, 0, 1]),
            security,
            Behavior::Reply(peer::response(b"feed-body-private")),
        )
        .await?;
        assignment::prepare(case, &peer, "https", host).await?;
        let owner = owner(case, fixture)?;
        let result = owner
            .start(case.owner.user, FeedField::Playlist, 0)?
            .finish()
            .await;
        if success {
            anyhow::ensure!(
                result.as_deref() == Ok(b"feed-body-private".as_slice()),
                "valid TLS feed failed"
            );
            peer.check_request(host, host != "127.0.0.1")?;
        } else {
            anyhow::ensure!(
                result == Err(FeedError::Connection),
                "invalid certificate was accepted"
            );
        }
        owner.shutdown().await?;
        peer.finish(true).await?;
    }
    let ipv6 = Peer::new(
        "::1".parse()?,
        Security::Tls,
        Behavior::Reply(peer::response(b"feed-body-private")),
    )
    .await?;
    assignment::prepare(case, &ipv6, "https", "[::1]").await?;
    let owner6 = owner(case, true)?;
    anyhow::ensure!(
        owner6
            .start(case.owner.user, FeedField::Playlist, 0)?
            .finish()
            .await?
            == b"feed-body-private",
        "IPv6 TLS feed failed"
    );
    ipv6.check_request("[::1]", false)?;
    owner6.shutdown().await?;
    ipv6.finish(true).await?;
    println!("IPTV_FEED_STAGE valid_tls_and_certificate_refusals_passed");

    let wrong = Peer::new(
        IpAddr::from([127, 0, 0, 1]),
        Security::Plain,
        Behavior::HeadersStall,
    )
    .await?;
    assignment::prepare(case, &wrong, "http", "duskcue-feed.invalid").await?;
    assignment::patch(case,serde_json::json!({"operation":"approve_destination","feed":"playlist","expected_origin":wrong.origin("http","duskcue-feed.invalid"),"addresses":["127.0.0.2"]})).await?.expect(axum::http::StatusCode::OK)?;
    let wrong_owner = owner(case, true)?;
    anyhow::ensure!(
        wrong_owner
            .start(case.owner.user, FeedField::Playlist, 0)?
            .finish()
            .await
            == Err(FeedError::Connection),
        "wrong pin did not fail its selected connection"
    );
    wrong_owner.shutdown().await?;
    anyhow::ensure!(
        wrong.hits() == 0,
        "wrong pin fell back to another destination"
    );
    wrong.finish(false).await?;

    let boundary_body = vec![b'x'; 65536];
    let prefix =
        b"HTTP/1.1 200 OK\r\nContent-Length: 65536\r\nContent-Encoding: Identity\r\nX-Padding: ";
    let mut boundary = prefix.to_vec();
    boundary.extend(vec![b'x'; 32768 - prefix.len() - 4]);
    boundary.extend_from_slice(b"\r\n\r\n");
    boundary.extend_from_slice(&boundary_body);
    response_case(case, boundary, false, Ok(&boundary_body))
        .await
        .context("exact header and body ceilings")?;

    response_case(
        case,
        peer::response(b"feed-body-private"),
        false,
        Ok(b"feed-body-private"),
    )
    .await
    .context("plain fixed-length response")?;
    response_case(
        case,
        b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n3\r\nabc\r\n0\r\n\r\n".to_vec(),
        false,
        Ok(b"abc"),
    )
    .await
    .context("plain chunked response")?;
    let trap = Peer::new(
        IpAddr::from([127, 0, 0, 1]),
        Security::Plain,
        Behavior::HeadersStall,
    )
    .await?;
    let redirect = format!("HTTP/1.1 302 Found\r\nLocation: {}/private?credential=feed-query-private\r\nContent-Length: 0\r\n\r\n",trap.origin("http","127.0.0.1")).into_bytes();
    response_case(case, redirect, false, Err(FeedError::Response))
        .await
        .context("redirect refusal")?;
    anyhow::ensure!(trap.hits() == 0, "redirect destination was contacted");
    trap.finish(false).await?;
    for (index, bytes) in [
        b"HTTP/1.1 200 OK\r\nContent-Encoding: gzip\r\nContent-Length: 0\r\n\r\n".to_vec(),
        b"HTTP/1.1 200 OK\r\nContent-Length: 65537\r\n\r\n".to_vec(),
        b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nContent-Length: 0\r\n\r\n0\r\n\r\n"
            .to_vec(),
        b"HTTP/1.1 200 OK\r\nTransfer-Encoding: gzip, chunked\r\n\r\n0\r\n\r\n".to_vec(),
        b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nTransfer-Encoding: chunked\r\n\r\n0\r\n\r\n".to_vec(),
        b"HTTP/1.1 200 OK\r\nContent-Length: 1\r\nContent-Length: 2\r\n\r\nx".to_vec(),
        b"HTTP/1.1 200 OK\r\nContent-Length: feed-response-private\r\n\r\n".to_vec(),
        format!(
            "HTTP/1.1 200 OK\r\nX-Large: {}\r\nContent-Length: 0\r\n\r\n",
            "x".repeat(33000)
        )
        .into_bytes(),
        format!(
            "HTTP/1.1 200 OK\r\n{}Content-Length: 0\r\n\r\n",
            (0..65).map(|i| format!("X-{i}: x\r\n")).collect::<String>()
        )
        .into_bytes(),
        {
            let mut b = b"HTTP/1.1 200 OK\r\n\r\n".to_vec();
            b.extend(vec![b'x'; 65537]);
            b
        },
    ].into_iter().enumerate() {
        response_case(case, bytes, false, Err(FeedError::Response)).await.with_context(||format!("adverse response case {index}"))?;
    }
    response_case(
        case,
        b"HTTP/1.1 200 OK\r\nContent-Length: 10\r\n\r\nabc".to_vec(),
        true,
        Err(FeedError::Response),
    )
    .await?;
    for (security, behavior, deadline) in [
        (Security::StalledTls, Behavior::HeadersStall, 5),
        (Security::Plain, Behavior::HeadersStall, 5),
        (Security::Plain, Behavior::BodyStall, 5),
        (Security::Plain, Behavior::Drip, 15),
    ] {
        let tls = matches!(security, Security::StalledTls);
        let peer = Peer::new(IpAddr::from([127, 0, 0, 1]), security, behavior).await?;
        assignment::prepare(
            case,
            &peer,
            if tls { "https" } else { "http" },
            "duskcue-feed.invalid",
        )
        .await?;
        let owner = owner(case, true)?;
        let start = std::time::Instant::now();
        anyhow::ensure!(
            owner
                .start(case.owner.user, FeedField::Playlist, 0)?
                .finish()
                .await
                == Err(FeedError::Deadline),
            "stalled operation did not expire"
        );
        anyhow::ensure!(
            start.elapsed() >= Duration::from_secs(deadline)
                && start.elapsed() < Duration::from_secs(deadline + 3),
            "deadline outside qualification bound"
        );
        owner.shutdown().await?;
        peer.finish(true).await?;
    }
    Ok(())
}

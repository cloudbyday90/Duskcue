// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::{Source, TransportError, arguments, playlist};
use std::net::SocketAddr;

#[test]
fn literal_destinations_are_explicit_and_forbidden_addresses_are_canonicalized() {
    for (url, approved) in [
        ("http://169.254.169.254/secret", "169.254.169.254:80"),
        ("http://0xa9fea9fe/secret", "169.254.169.254:80"),
        (
            "http://[::ffff:169.254.169.254]/secret",
            "[::ffff:169.254.169.254]:80",
        ),
        ("http://100.100.100.200/", "100.100.100.200:80"),
        ("http://[fd00:ec2::254]/", "[fd00:ec2::254]:80"),
        ("http://0.0.0.0/", "0.0.0.0:80"),
        ("http://224.0.0.1/", "224.0.0.1:80"),
        ("http://[fe80::1]/", "[fe80::1]:80"),
        ("http://localhost/", "127.0.0.1:80"),
        ("http://127.0.0.1:81/", "127.0.0.1:80"),
        ("http://private:secret@127.0.0.1/", "127.0.0.1:80"),
    ] {
        assert!(matches!(
            Source::new(url, approved.parse().unwrap()),
            Err(TransportError::Destination)
        ));
    }
    for target in [
        "127.0.0.1:9191",
        "192.168.1.5:9191",
        "[::1]:9191",
        "[fd00::123]:9191",
    ] {
        assert!(
            Source::new(
                &format!("http://{target}/opaque?secret=fixture"),
                target.parse::<SocketAddr>().unwrap()
            )
            .is_ok()
        );
    }
    assert!(!TransportError::Connection.to_string().contains("fixture"));
}

#[test]
fn rolling_playlists_refuse_external_and_unbounded_publication() {
    let valid = "#EXTM3U\n#EXT-X-VERSION:7\n#EXT-X-TARGETDURATION:2\n#EXT-X-MEDIA-SEQUENCE:4\n#EXT-X-INDEPENDENT-SEGMENTS\n#EXT-X-MAP:URI=\"init.mp4\"\n#EXTINF:2.0,\nseg_00000004.m4s\n#EXTINF:2.0,\nseg_00000005.m4s\n";
    let parsed = playlist::parse(valid).unwrap().unwrap();
    assert_eq!(parsed.sequence, 4);
    assert_eq!(parsed.segments.len(), 2);
    for bad in [
        valid.replace("seg_00000004.m4s", "../secret"),
        valid.replace("init.mp4", "https://example.invalid/x"),
        format!("{valid}#EXT-X-PLAYLIST-TYPE:EVENT\n"),
        valid.replace("2.0,", "NaN,"),
    ] {
        assert!(playlist::parse(&bad).is_err());
    }
    assert!(playlist::parse(valid.trim_end()).unwrap().is_none());
}

#[test]
fn media_arguments_keep_http_and_event_semantics_out_of_the_worker() {
    let args = arguments::build(std::path::Path::new("owned"));
    for pair in [
        ["-i", "pipe:0"],
        ["-protocol_whitelist", "pipe"],
        ["-hls_list_size", "15"],
        ["-hls_delete_threshold", "2"],
    ] {
        assert!(args.windows(2).any(|window| window == pair));
    }
    assert!(
        !args
            .iter()
            .any(|arg| arg.contains("http") || arg == "event")
    );
    assert!(!args.iter().any(|arg| arg == "-hls_playlist_type"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cancel_during_headers_joins_the_connection_and_releases_capacity() {
    cancelled_connection("http").await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cancel_during_tls_connect_joins_the_connection_and_releases_capacity() {
    cancelled_connection("https").await;
}

async fn cancelled_connection(scheme: &'static str) {
    use tokio::io::AsyncReadExt;
    let root = std::env::temp_dir().join(format!("duskcue-transport-{}", uuid::Uuid::now_v7()));
    tokio::fs::create_dir(&root).await.unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (entered, ready) = tokio::sync::oneshot::channel();
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut bytes = [0; 4096];
        let count = socket.read(&mut bytes).await.unwrap();
        assert!(count > 0);
        if scheme == "https" {
            assert_eq!(bytes[0], 0x16);
        }
        let _ = entered.send(());
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            let mut drained = 0;
            loop {
                let count = socket.read(&mut bytes).await.unwrap();
                if count == 0 {
                    return 0;
                }
                drained += count;
                assert!(drained <= 8192);
            }
        })
        .await
        .unwrap()
    });
    let owner = super::TransportOwner::new(&root).await.unwrap();
    let mut session = owner
        .start(
            Source::new(
                &format!("{scheme}://{address}/private?secret=fixture"),
                address,
            )
            .unwrap(),
        )
        .unwrap();
    ready.await.unwrap();
    session.stop().await.unwrap();
    owner.shutdown().await.unwrap();
    assert_eq!(server.await.unwrap(), 0);
    assert!(owner.idle());
    assert!(
        tokio::fs::read_dir(&root)
            .await
            .unwrap()
            .next_entry()
            .await
            .unwrap()
            .is_none()
    );
    tokio::fs::remove_dir(&root).await.unwrap();
}

#[tokio::test]
async fn unsupported_runtime_is_rejected_before_resource_allocation() {
    let root = std::env::temp_dir().join(format!("duskcue-transport-{}", uuid::Uuid::now_v7()));
    tokio::fs::create_dir(&root).await.unwrap();
    assert!(matches!(
        super::TransportOwner::new(&root).await,
        Err(TransportError::Worker)
    ));
    assert!(
        tokio::fs::read_dir(&root)
            .await
            .unwrap()
            .next_entry()
            .await
            .unwrap()
            .is_none()
    );
    tokio::fs::remove_dir(&root).await.unwrap();
}

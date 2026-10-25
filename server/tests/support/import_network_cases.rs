// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::{
    http::Case,
    import_fixture as fixture,
    peer::{self, Behavior, Peer, Security},
};
use duskcue::domains::live_tv::feed::imports::{ImportError, ImportOwner};

pub async fn run(case: &mut Case) -> anyhow::Result<()> {
    preconnect(case).await?;
    for behavior in [Behavior::Reply(b"HTTP/1.1 302 Found\r\nLocation: http://127.0.0.1:9/redirect-private\r\nContent-Length: 0\r\n\r\n".to_vec()),Behavior::Reply(b"HTTP/1.1 200 OK\r\nContent-Encoding: gzip\r\nContent-Length: 0\r\n\r\n".to_vec()),Behavior::Reply(b"HTTP/1.1 200 OK\r\nContent-Length: 8388609\r\n\r\n".to_vec())] {
        let a=Peer::new("127.0.0.1".parse()?,Security::Plain,behavior).await?;
        let b=Peer::new("127.0.0.1".parse()?,Security::Tls,Behavior::Reply(peer::response(&fixture::guide()))).await?;
        fixture::prepare(case,&a,&b).await?;let owner=fixture::owner(case)?;
        anyhow::ensure!(owner.start(case.owner.user,0,0)?.finish().await==Err(ImportError::Network),"unqualified response accepted");
        anyhow::ensure!(b.hits()==0,"guide fetched after playlist transport refusal");
        owner.shutdown().await?;a.finish(true).await?;b.finish(false).await?;
    }
    for security in [Security::Expired, Security::Tls] {
        let a = Peer::new(
            "127.0.0.1".parse()?,
            Security::Plain,
            Behavior::Reply(peer::response(&fixture::playlist())),
        )
        .await?;
        let b = Peer::new(
            "127.0.0.1".parse()?,
            security,
            Behavior::Reply(peer::response(&fixture::guide())),
        )
        .await?;
        fixture::prepare(case, &a, &b).await?;
        let owner = ImportOwner::new(case.pool.clone(), case.state.encryption_key.clone())?;
        anyhow::ensure!(
            owner.start(case.owner.user, 0, 0)?.finish().await == Err(ImportError::Network),
            "public roots trusted fixture-only certificate"
        );
        owner.shutdown().await?;
        a.finish(true).await?;
        b.finish(true).await?;
    }
    for (security, behavior, phase) in [
        (Security::StalledTls, Behavior::HeadersStall, 2),
        (Security::Tls, Behavior::HeadersStall, 3),
        (Security::Tls, Behavior::BodyStall, 3),
    ] {
        let a = Peer::new(
            "127.0.0.1".parse()?,
            Security::Plain,
            Behavior::Reply(peer::response(&fixture::playlist())),
        )
        .await?;
        let b = Peer::new("127.0.0.1".parse()?, security, behavior).await?;
        fixture::prepare(case, &a, &b).await?;
        let owner = fixture::owner(case)?;
        let import = owner.start(case.owner.user, 0, 0)?;
        b.wait_phase(phase).await?;
        import.cancel();
        anyhow::ensure!(
            import.finish().await == Err(ImportError::Cancelled),
            "network phase cancellation did not finish"
        );
        owner.shutdown().await?;
        a.finish(true).await?;
        b.finish(true).await?;
    }
    let mut guide = String::from("<tv>");
    for i in 0..5000 {
        guide.push_str(&format!("<programme channel='shared-guide' start='2026'><title>Representative {i}</title></programme>"));
    }
    guide.push_str("</tv>");
    fixture::run(case, &fixture::playlist(), guide.as_bytes()).await??;
    anyhow::ensure!(
        fixture::status(case).await?.programme_count == 5000
            && fixture::count(case, "programmes").await? == 5000,
        "representative guide exceeded batching or old fixture limits"
    );
    println!("IPTV_IMPORT_STAGE http_tls_and_scale_passed");
    Ok(())
}

async fn preconnect(case: &mut Case) -> anyhow::Result<()> {
    let (a, b) = fixture::peers(&fixture::playlist(), &fixture::guide()).await?;
    fixture::prepare(case, &a, &b).await?;
    let owner = fixture::owner(case)?;
    let before = fixture::status(case).await?;
    anyhow::ensure!(
        owner.start(case.owner.user, 8, 0)?.finish().await == Err(ImportError::Authority),
        "out-of-range pin admitted"
    );
    sqlx::query("UPDATE live_tv_import_state SET attempt_sequence=$2 WHERE assignment_id=$1")
        .bind(before.assignment_id)
        .bind(i64::MAX)
        .execute(&case.pool)
        .await?;
    anyhow::ensure!(
        owner.start(case.owner.user, 0, 0)?.finish().await == Err(ImportError::Storage),
        "attempt overflow admitted"
    );
    anyhow::ensure!(
        a.hits() == 0
            && b.hits() == 0
            && fixture::status(case).await?.attempt_id == before.attempt_id,
        "failed admission reached peers or changed latest attempt"
    );
    sqlx::query("UPDATE live_tv_import_state SET attempt_sequence=$2 WHERE assignment_id=$1")
        .bind(before.assignment_id)
        .bind(before.attempt_sequence)
        .execute(&case.pool)
        .await?;
    owner.shutdown().await?;
    a.finish(false).await?;
    b.finish(false).await?;
    Ok(())
}

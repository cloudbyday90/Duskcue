// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::{assignment, catalog_fixture as catalog, database::Database, fixture, http};
use duskcue::{
    domains::{household_security::types::Scope, live_tv::feed::imports::ImportOwner},
    extractors::CredentialTransport,
    services::playback_admission::{AdmissionError as E, AdmissionOwner, Request, Subject},
};
use serde_json::json;
use sqlx::Row;
use uuid::Uuid;

async fn request(case: &http::Case, snapshot: Uuid) -> anyhow::Result<Request> {
    let row=sqlx::query("SELECT id,credential_version,profile_context_revision FROM user_sessions WHERE user_id=$1 AND active_profile_id=$2 ORDER BY created_at DESC LIMIT 1")
        .bind(case.owner.user).bind(case.standard).fetch_one(&case.pool).await?;
    let channel = sqlx::query_scalar(
        "SELECT id FROM live_tv_catalog_channels WHERE snapshot_id=$1 ORDER BY channel_key LIMIT 1",
    )
    .bind(snapshot)
    .fetch_one(&case.pool)
    .await?;
    Ok(Request {
        id: Uuid::now_v7(),
        actor: Scope {
            user: case.owner.user,
            profile: case.standard,
            session: row.try_get("id")?,
            version: row.try_get("credential_version")?,
            transport: CredentialTransport::Cookie,
            binding: None,
        },
        profile_revision: row.try_get("profile_context_revision")?,
        subject: Subject::Live { snapshot, channel },
    })
}

pub async fn run(db: &Database) -> anyhow::Result<()> {
    let f = catalog::setup(db).await?;
    let outcome=async {
        let case=&f.case;let r=request(case,f.snapshot).await?;
        let vod=fixture::Fixture::new(db).await?;
        sqlx::query("UPDATE users SET max_streams=2 WHERE id=$1").bind(r.actor.user).execute(&db.pool).await?;
        sqlx::query("UPDATE live_tv_assignments SET max_live_streams=1 WHERE household_user_id=$1").bind(r.actor.user).execute(&db.pool).await?;
        let owner=AdmissionOwner::new(db.pool.clone(),case.state.encryption_key.clone(),None).await?;
        let other=AdmissionOwner::new(db.pool.clone(),case.state.encryption_key.clone(),None).await?;
        fixture::start(&owner,r).await?;
        let mut limited=r;limited.id=Uuid::now_v7();
        assert_eq!(fixture::start(&other,limited).await.unwrap_err(),E::Capacity);
        sqlx::query("UPDATE users SET has_all_library_access=true WHERE id=$1").bind(r.actor.user).execute(&db.pool).await?;
        let mut mixed=r;mixed.id=Uuid::now_v7();mixed.subject=vod.subject;
        fixture::start(&other,mixed).await?;
        let mut excess=mixed;excess.id=Uuid::now_v7();
        assert_eq!(fixture::start(&other,excess).await.unwrap_err(),E::Capacity);
        let mut second=r;second.id=Uuid::now_v7();
        assert_eq!(fixture::start(&other,second).await.unwrap_err(),E::Capacity);
        let next=catalog::refresh(case).await??;assert_ne!(next,f.snapshot);
        assert!(owner.renew(r).await.is_ok());
        assert!(!fixture::start(&other,r).await?.fresh);
        let count=f.source.count();
        for _ in 0..3 {owner.authorize_use(r).await?;}
        assert_eq!(f.source.count(),count);
        sqlx::query("UPDATE live_tv_catalog_channels SET metadata=metadata||'x' WHERE snapshot_id=$1").bind(next).execute(&db.pool).await?;
        assert!(owner.renew(r).await.is_err());fixture::released(&db.pool,r.id).await?;
        fixture::close(&owner).await?;fixture::close(&other).await?;

        for change in ["remove","generation","manifest","access","kids","restored_access","restored_channel"] {
            let snapshot=catalog::refresh(case).await??;
            let r=request(case,snapshot).await?;
            let owner=AdmissionOwner::new(db.pool.clone(),case.state.encryption_key.clone(),None).await?;
            fixture::start(&owner,r).await?;
            if change=="restored_channel" {
                f.source.set(b"#EXTM3U\n".to_vec(),b"<tv/>".to_vec()).await?;
                catalog::refresh(case).await??;
                f.source.set(catalog::playlist(),catalog::guide()).await?;
                catalog::refresh(case).await??;
            }
            if change=="restored_access" {
                let mut tx=db.pool.begin().await?;
                sqlx::query("UPDATE live_tv_profile_access SET enabled=false WHERE household_user_id=$1").bind(r.actor.user).execute(&mut *tx).await?;
                sqlx::query("UPDATE live_tv_profile_access SET enabled=true WHERE household_user_id=$1").bind(r.actor.user).execute(&mut *tx).await?;
                tx.commit().await?;
            }
            let statement=match change {
                "remove"=>"DELETE FROM live_tv_catalog_channels WHERE household_user_id=$1",
                "generation"=>"UPDATE live_tv_assignments SET source_generation=source_generation+1 WHERE household_user_id=$1",
                "manifest"=>"UPDATE live_tv_catalog_snapshots SET manifest=manifest||'x' WHERE household_user_id=$1",
                "access"=>"UPDATE live_tv_profile_access SET enabled=false WHERE household_user_id=$1",
                "kids"=>"UPDATE user_profiles SET profile_type='kids' WHERE owner_user_id=$1",
                _=>"SELECT id FROM users WHERE id=$1",
            };
            sqlx::query(statement).bind(r.actor.user).execute(&db.pool).await?;
            assert!(owner.renew(r).await.is_err(),"live revocation {change}");
            fixture::released(&db.pool,r.id).await?;fixture::close(&owner).await?;
            if change=="generation" {
                case.admin_read().await?;
                let origin=f.source.origin();
                for feed in ["playlist","guide"] {assignment::patch(case,json!({"operation":"approve_destination","feed":feed,"expected_origin":origin,"addresses":["127.0.0.1"]})).await?.expect(axum::http::StatusCode::OK)?;}
            }
            sqlx::query("UPDATE live_tv_profile_access SET enabled=true WHERE household_user_id=$1").bind(r.actor.user).execute(&db.pool).await?;
            sqlx::query("UPDATE user_profiles SET profile_type='standard' WHERE owner_user_id=$1").bind(r.actor.user).execute(&db.pool).await?;
        }
        Ok::<_,anyhow::Error>(())
    }.await;
    f.source.finish().await?;
    outcome?;
    private(db).await?;
    Ok(())
}

async fn private(db: &Database) -> anyhow::Result<()> {
    let Some(path) = std::env::var_os("DUSKCUE_TEST_PRIVATE_FEEDS") else {
        return Ok(());
    };
    let outcome=async {
        let root=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).parent().ok_or_else(||anyhow::anyhow!("fixture root"))?.join(".cache").canonicalize()?;
        let path=std::path::PathBuf::from(path).canonicalize()?;
        anyhow::ensure!(path.starts_with(root) && std::fs::metadata(&path)?.len()<=4096,"private fixture bounds");
        let value:serde_json::Value=serde_json::from_slice(&std::fs::read(path)?)?;
        let mut case=assignment::setup(db).await?;
        let mut urls=Vec::new();
        for field in ["playlist","guide"] {
            let text=value[field].as_str().ok_or_else(||anyhow::anyhow!("private field"))?;
            let url=url::Url::parse(text)?;
            anyhow::ensure!(url.scheme()=="http" && url.fragment().is_none(),"private transport");
            let ip:url::Host<&str>=url.host().ok_or_else(||anyhow::anyhow!("private host"))?;
            let address=match ip {url::Host::Ipv4(ip)=>ip.to_string(),url::Host::Ipv6(ip)=>ip.to_string(),_=>anyhow::bail!("private pin")};
            urls.push((text.to_owned(),url.origin().ascii_serialization(),address));
        }
        assignment::patch(&case,json!({"operation":"rotate_credentials","playlist_url":urls[0].0,"guide_url":urls[1].0})).await?.expect(axum::http::StatusCode::OK)?;
        for (feed,(_,origin,address)) in ["playlist","guide"].into_iter().zip(&urls) {
            assignment::patch(&case,json!({"operation":"approve_destination","feed":feed,"expected_origin":origin,"addresses":[address]})).await?.expect(axum::http::StatusCode::OK)?;
        }
        sqlx::query("INSERT INTO user_capabilities(user_id,capability,is_granted) VALUES ($1,'can_use_live_tv',true)").bind(case.owner.user).execute(&db.pool).await?;
        case.owner_proof(true).await?;
        case.owner_change(json!({"operation":"set_profile_access","profile_id":case.standard,"enabled":true})).await?.expect(axum::http::StatusCode::OK)?;
        let importer=ImportOwner::new(db.pool.clone(),case.state.encryption_key.clone())?;
        let result=importer.start(case.owner.user,0,0)?.finish().await;
        importer.shutdown().await?;
        let snapshot=result?;
        let r=request(&case,snapshot).await?;
        let owner=AdmissionOwner::new(db.pool.clone(),case.state.encryption_key.clone(),None).await?;
        fixture::start(&owner,r).await?;owner.authorize_use(r).await?;owner.renew(r).await?;
        fixture::close(&owner).await?;
        Ok::<_,anyhow::Error>(())
    }.await;
    outcome.map_err(|_| {
        anyhow::anyhow!("private feed qualification failed; identifying details withheld")
    })?;
    println!("PRIVATE_FEED_QUALIFICATION passed=true stream_fetches=0");
    Ok(())
}

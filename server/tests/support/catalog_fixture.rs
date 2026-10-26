// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::{
    assignment, database, feeds,
    http::{self, Actor, Case},
};
use axum::http::StatusCode;
use duskcue::domains::live_tv::feed::imports::{ImportError, ImportOwner};
use serde_json::json;
use uuid::Uuid;

pub struct Fixture {
    pub case: Case,
    pub viewer: Actor,
    pub second: Uuid,
    pub source: feeds::Server,
    pub snapshot: Uuid,
}

pub fn playlist() -> Vec<u8> {
    format!("#EXTM3U\n#EXTINF:-1 tvg-id=\"shared-guide\" tvg-chno=\"007\" group-title=\"News<&>\" tvg-logo=\"http://127.0.0.1:9/nested-logo-private\",Channel-private\nhttps://stream.invalid/prefix/proxy/ts/stream/{}?token=stream-private\n#EXTINF:-1 tvg-id=\"shared-guide\",Other-private\nhttps://stream.invalid/proxy/ts/stream/{}?token=other-private\n#EXTINF:-1 tvg-id=\"undeclared-guide-private\",Undeclared-private\nhttps://stream.invalid/proxy/ts/stream/{}\n#EXTINF:-1,\nhttps://stream.invalid/proxy/ts/stream/{}\n",Uuid::from_u128(1),Uuid::from_u128(2),Uuid::from_u128(3),Uuid::from_u128(4)).into_bytes()
}

pub fn guide() -> Vec<u8> {
    br#"<tv><channel id="shared-guide"><display-name lang="en">Guide-private</display-name></channel><channel id="shared-guide"><display-name lang="ar">Guide-alt-private</display-name></channel>
<programme channel="shared-guide" start="20261025120000" stop="20261025130000 +0000"><title lang="en">Programme-private</title><title lang="ar">Alternative-private</title><rating system="raw-private"><value>Unmapped</value></rating></programme>
<programme channel="shared-guide" start="20261025120000" stop="20261025130000 +0000"><title lang="en">Programme-private</title><title lang="ar">Alternative-private</title><rating system="raw-private"><value>Unmapped</value></rating></programme>
<programme channel="shared-guide" start="20261025120000" stop="20261025130000" clumpidx="0/2"><title>Overlap-private</title><rating><value></value></rating></programme>
<programme channel="shared-guide" start="20261025121500" stop="20261025123000"><title lang="unknown_tag">Inside-private</title></programme>
<programme channel="shared-guide" start="20261025110000" stop="20261025120000"><title>Before-private</title></programme>
<programme channel="shared-guide" start="20261025130000" stop="20261025140000"><title>After-private</title></programme>
<programme channel="shared-guide" start="2026"><title>Partial-private</title></programme>
<programme channel="shared-guide" start="20261025120000"><title>Missing-stop-private</title></programme>
<programme channel="shared-guide" stop="20261025130000"><title>Missing-start-private</title></programme>
<programme channel="shared-guide" start="20261025130000" stop="20261025120000"><title>Reversed-private</title></programme></tv>"#.to_vec()
}

pub async fn setup(db: &database::Database) -> anyhow::Result<Fixture> {
    let mut case = assignment::setup(db).await?;
    case.owner.cookie = true;
    case.owner_proof(true).await?;
    let second = Uuid::now_v7();
    sqlx::query("INSERT INTO user_profiles(id,owner_user_id,name,profile_type) VALUES ($1,$2,'Second','standard')").bind(second).bind(case.owner.user).execute(&case.pool).await?;
    for profile in [case.standard, second] {
        case.owner_change(
            json!({"operation":"set_profile_access","profile_id":profile,"enabled":true}),
        )
        .await?
        .expect(StatusCode::OK)?;
    }
    sqlx::query("INSERT INTO user_capabilities(user_id,capability,is_granted) VALUES ($1,'can_use_live_tv',true)").bind(case.owner.user).execute(&case.pool).await?;
    let viewer = Actor {
        user: case.owner.user,
        profile: case.standard,
        token: http::session(&case.pool, case.owner.user, case.standard).await?,
        cookie: false,
    };
    case.secrets.extend([
        viewer.token.clone(),
        "stream-private".into(),
        "other-private".into(),
        "nested-logo-private".into(),
        "shared-guide".into(),
        "Channel-private".into(),
        "Programme-private".into(),
        "catalog-source-private".into(),
    ]);
    let source = feeds::Server::start(playlist(), guide()).await?;
    let outcome=async {
        let origin=source.origin();
        let playlist=format!("{origin}/playlist?credential=catalog-source-private");
        let guide=format!("{origin}/guide?credential=catalog-source-private");
        case.secrets.extend([playlist.clone(),guide.clone()]);
        assignment::patch(&case,json!({"operation":"rotate_credentials","playlist_url":playlist,"guide_url":guide})).await?.expect(StatusCode::OK)?;
        for feed in ["playlist","guide"] {assignment::patch(&case,json!({"operation":"approve_destination","feed":feed,"expected_origin":origin,"addresses":["127.0.0.1"]})).await?.expect(StatusCode::OK)?;}
        refresh(&case).await?.map_err(anyhow::Error::from)
    }.await;
    match outcome {
        Ok(snapshot) => Ok(Fixture {
            case,
            viewer,
            second,
            source,
            snapshot,
        }),
        Err(error) => {
            source.finish().await?;
            Err(error)
        }
    }
}

pub async fn refresh(case: &Case) -> anyhow::Result<Result<Uuid, ImportError>> {
    let owner =
        ImportOwner::qualification_fixture(case.pool.clone(), case.state.encryption_key.clone())?;
    let outcome = owner.start(case.owner.user, 0, 0)?.finish().await;
    anyhow::ensure!(
        owner.idle(),
        "catalog import fixture did not release ownership"
    );
    owner.shutdown().await?;
    Ok(outcome)
}

// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::{
    database::Database,
    fixture::{self, Fixture},
};
use duskcue::{
    extractors::CredentialTransport,
    services::playback_admission::{AdmissionError as E, ReservationState, operator},
};
use uuid::Uuid;

pub async fn run(db: &Database) -> anyhow::Result<()> {
    for _ in 0..4 {
        let f = Fixture::new(db).await?;
        sqlx::query("UPDATE users SET max_streams=1 WHERE id=$1")
            .bind(f.actor.user)
            .execute(&db.pool)
            .await?;
        let a = f.owner().await?;
        let b = f.owner().await?;
        let (first, second) = tokio::join!(
            fixture::start(&a, f.request()),
            fixture::start(&b, f.request())
        );
        assert_eq!(usize::from(first.is_ok()) + usize::from(second.is_ok()), 1);
        assert!(
            matches!(first, Ok(_) | Err(E::Capacity)) && matches!(second, Ok(_) | Err(E::Capacity))
        );
        fixture::close(&a).await?;
        fixture::close(&b).await?;
    }
    let f = Fixture::new(db).await?;
    let a = f.owner().await?;
    let b = f.owner().await?;
    let r1 = f.request();
    let r2 = f.request();
    let (x, y) = tokio::join!(fixture::start(&a, r1), fixture::start(&b, r2));
    assert!(x?.fresh && y?.fresh);
    let isolated = Fixture::new(db).await?;
    let isolated_owner = isolated.owner().await?;
    fixture::start(&isolated_owner, isolated.request()).await?;
    fixture::close(&isolated_owner).await?;
    assert_eq!(
        fixture::start(&a, f.request()).await.unwrap_err(),
        E::Capacity
    );
    let replay = fixture::start(&b, r1).await?;
    assert!(!replay.fresh && !replay.owned_here);
    assert_eq!(replay.instance, a.instance());
    let before = replay.deadline;
    assert_eq!(fixture::start(&a, r1).await?.deadline, before);
    let mut mismatch = r1;
    mismatch.subject = match r1.subject {
        duskcue::services::playback_admission::Subject::Vod { file, .. } => {
            duskcue::services::playback_admission::Subject::Vod {
                file,
                item: Uuid::now_v7(),
            }
        }
        _ => unreachable!(),
    };
    assert_eq!(fixture::start(&a, mismatch).await.unwrap_err(), E::Conflict);
    assert_eq!(a.authorize_use(mismatch).await.unwrap_err(), E::Conflict);
    assert!(a.authorize_use(r1).await.is_ok());
    sqlx::query("UPDATE users SET max_streams=0 WHERE id=$1")
        .bind(f.actor.user)
        .execute(&db.pool)
        .await?;
    assert!(a.renew(r1).await?.deadline > before);
    assert_eq!(
        fixture::start(&b, f.request()).await.unwrap_err(),
        E::Capacity
    );
    a.stop(r1.id)?;
    fixture::released(&db.pool, r1.id).await?;
    assert_eq!(
        fixture::start(&a, r1).await?.state,
        ReservationState::Released
    );
    sqlx::query("UPDATE users SET max_streams=-1 WHERE id=$1")
        .bind(f.actor.user)
        .execute(&db.pool)
        .await?;
    assert_eq!(
        fixture::start(&a, f.request()).await.unwrap_err(),
        E::Policy
    );
    fixture::close(&a).await?;
    fixture::close(&b).await?;

    let f = Fixture::new(db).await?;
    let owner = f.owner().await?;
    sqlx::query("UPDATE users SET max_streams=NULL WHERE id=$1")
        .bind(f.actor.user)
        .execute(&db.pool)
        .await?;
    for _ in 0..4 {
        fixture::start(&owner, f.request()).await?;
    }
    sqlx::query("UPDATE users SET streaming_policy_id=(SELECT id FROM streaming_policies WHERE name='Family') WHERE id=$1").bind(f.actor.user).execute(&db.pool).await?;
    assert_eq!(
        fixture::start(&owner, f.request()).await.unwrap_err(),
        E::Capacity
    );
    let mut legacy = f.request();
    legacy.actor.transport = CredentialTransport::Legacy;
    assert_eq!(
        fixture::start(&owner, legacy).await.unwrap_err(),
        E::Authority
    );
    fixture::close(&owner).await?;
    sqlx::query("UPDATE streaming_policies SET is_default=false")
        .execute(&db.pool)
        .await?;
    sqlx::query("UPDATE users SET streaming_policy_id=NULL WHERE id=$1")
        .bind(f.actor.user)
        .execute(&db.pool)
        .await?;
    let owner = f.owner().await?;
    assert_eq!(
        fixture::start(&owner, f.request()).await.unwrap_err(),
        E::Policy
    );
    sqlx::query("UPDATE streaming_policies SET is_default=true WHERE name IN ('Admin','Family')")
        .execute(&db.pool)
        .await?;
    assert_eq!(
        fixture::start(&owner, f.request()).await.unwrap_err(),
        E::Policy
    );
    sqlx::query("UPDATE streaming_policies SET is_default=false WHERE name='Family'")
        .execute(&db.pool)
        .await?;
    fixture::close(&owner).await?;

    for capability in [false, true] {
        let f = Fixture::new(db).await?;
        let owner = f.owner().await?;
        let r = f.request();
        fixture::start(&owner, r).await?;
        let mut tx = db.pool.begin().await?;
        if capability {
            sqlx::query("INSERT INTO user_capabilities(user_id,capability,is_granted) VALUES ($1,'play_media',false)").bind(f.actor.user).execute(&mut *tx).await?;
            sqlx::query("UPDATE user_capabilities SET is_granted=true WHERE user_id=$1")
                .bind(f.actor.user)
                .execute(&mut *tx)
                .await?;
        } else {
            sqlx::query("UPDATE users SET status='disabled' WHERE id=$1")
                .bind(f.actor.user)
                .execute(&mut *tx)
                .await?;
            sqlx::query("UPDATE users SET status='active' WHERE id=$1")
                .bind(f.actor.user)
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
        assert_ne!(fixture::state(&db.pool, r.id).await?, "held");
        assert!(owner.renew(r).await.is_err());
        fixture::released(&db.pool, r.id).await?;
        fixture::close(&owner).await?;
    }

    for revocation in [
        "expiry",
        "credential",
        "profile_roundtrip",
        "capability",
        "content",
        "account",
        "session",
        "profile_delete",
        "purge",
    ] {
        let f = Fixture::new(db).await?;
        let owner = f.owner().await?;
        let r = f.request();
        fixture::start(&owner, r).await?;
        if revocation == "profile_roundtrip" {
            sqlx::query("UPDATE user_sessions SET active_profile_id=NULL WHERE user_id=$1")
                .bind(f.actor.user)
                .execute(&db.pool)
                .await?;
            sqlx::query("UPDATE user_sessions SET active_profile_id=$2 WHERE user_id=$1")
                .bind(f.actor.user)
                .bind(f.actor.profile)
                .execute(&db.pool)
                .await?;
        }
        let sql = match revocation {
            "expiry" => {
                "UPDATE playback_reservations SET deadline=clock_timestamp()-interval '1 second' WHERE household_id=$1"
            }
            "credential" => {
                "UPDATE user_sessions SET credential_version=credential_version+1 WHERE user_id=$1"
            }
            "profile_roundtrip" => "SELECT id FROM user_sessions WHERE user_id=$1",
            "capability" => {
                "INSERT INTO user_capabilities(user_id,capability,is_granted) VALUES ($1,'play_media',false)"
            }
            "content" => "UPDATE users SET has_all_library_access=false WHERE id=$1",
            "session" => "DELETE FROM user_sessions WHERE user_id=$1",
            "profile_delete" => "DELETE FROM user_profiles WHERE owner_user_id=$1",
            "purge" => "DELETE FROM users WHERE id=$1",
            _ => "UPDATE users SET status='disabled' WHERE id=$1",
        };
        sqlx::query(sql)
            .bind(f.actor.user)
            .execute(&db.pool)
            .await?;
        assert!(owner.renew(r).await.is_err(), "revocation {revocation}");
        fixture::released(&db.pool, r.id).await?;
        fixture::close(&owner).await?;
    }

    let f = Fixture::new(db).await?;
    let owner = f.owner().await?;
    let r = f.request();
    fixture::start(&owner, r).await?;
    assert_eq!(
        operator::reconcile(&db.pool, owner.instance(), 1, true)
            .await
            .unwrap_err(),
        E::Conflict
    );
    let fence = operator::fence(&db.pool, owner.instance(), 1).await?;
    assert_eq!(fence, 2);
    assert_eq!(
        operator::fence(&db.pool, owner.instance(), 1)
            .await
            .unwrap_err(),
        E::Conflict
    );
    assert_eq!(
        operator::reconcile(&db.pool, owner.instance(), fence, false)
            .await
            .unwrap_err(),
        E::Cleanup
    );
    assert_eq!(
        operator::reconcile(&db.pool, owner.instance(), 1, true)
            .await
            .unwrap_err(),
        E::Conflict
    );
    assert_eq!(owner.renew(r).await.unwrap_err(), E::Closed);
    fixture::released(&db.pool, r.id).await?;
    fixture::close(&owner).await?;
    assert_eq!(
        operator::reconcile(&db.pool, owner.instance(), fence, true).await?,
        0
    );
    Ok(())
}

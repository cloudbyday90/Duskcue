// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::http::{self, Case};
use axum::http::StatusCode;
use duskcue::domains::live_tv::assignment::secrets::{self, FeedField};
use serde_json::json;
use sqlx::Row;
use uuid::Uuid;

pub async fn run(case: &mut Case) -> anyhow::Result<()> {
    let before = case.admin_read().await?;
    let id = Uuid::parse_str(
        before.value["assignment_id"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("assignment identity missing"))?,
    )?;
    let row =
        sqlx::query("SELECT playlist_secret,guide_secret FROM live_tv_assignments WHERE id=$1")
            .bind(id)
            .fetch_one(&case.pool)
            .await?;
    let playlist: String = row.get("playlist_secret");
    let guide: String = row.get("guide_secret");
    case.secrets.extend([playlist.clone(), guide.clone()]);
    anyhow::ensure!(
        playlist.starts_with("duskcue-secret:v1:")
            && guide.starts_with("duskcue-secret:v1:")
            && !playlist.contains(http::PLAYLIST),
        "feed secrets were not encrypted"
    );
    anyhow::ensure!(
        secrets::open(
            &case.state.encryption_key,
            case.owner.user,
            id,
            FeedField::Playlist,
            &playlist
        )? == http::PLAYLIST
            && secrets::open(
                &case.state.encryption_key,
                case.owner.user,
                id,
                FeedField::Guide,
                &guide
            )? == http::GUIDE,
        "stored feeds did not round trip exactly"
    );
    for (user, record, field, value) in [
        (case.other.user, id, FeedField::Playlist, &playlist),
        (
            case.owner.user,
            Uuid::now_v7(),
            FeedField::Playlist,
            &playlist,
        ),
        (case.owner.user, id, FeedField::Guide, &playlist),
        (case.owner.user, id, FeedField::Playlist, &guide),
    ] {
        anyhow::ensure!(
            secrets::open(&case.state.encryption_key, user, record, field, value).is_err(),
            "stored ciphertext accepted wrong household/record/field"
        );
    }
    let foreign=sqlx::query("INSERT INTO live_tv_profile_access(household_user_id,assignment_id,profile_id,enabled) VALUES ($1,$2,$3,true)").bind(case.owner.user).bind(id).bind(case.other.profile).execute(&case.pool).await;
    anyhow::ensure!(
        foreign.is_err(),
        "database allowed cross-household profile binding"
    );
    sqlx::query("CREATE FUNCTION fixture_assignment_failure() RETURNS TRIGGER LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'controlled assignment refusal'; END $$").execute(&case.pool).await?;
    sqlx::query("CREATE TRIGGER fixture_assignment_failure BEFORE INSERT ON live_tv_assignments FOR EACH ROW EXECUTE FUNCTION fixture_assignment_failure()").execute(&case.pool).await?;
    let etag = before.etag()?;
    let replace = json!({"operation":"replace_assignment","playlist_url":http::ROTATED_PLAYLIST,"guide_url":http::ROTATED_GUIDE});
    http::call(
        &case.app,
        Some(&case.admin),
        "PATCH",
        &case.admin_path(),
        Some(replace.clone()),
        &[("if-match", &etag)],
    )
    .await?
    .expect(StatusCode::SERVICE_UNAVAILABLE)?;
    anyhow::ensure!(
        case.admin_read().await?.value == before.value,
        "failed replacement partially mutated assignment"
    );
    case.owner_read().await?;
    let kept:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM live_tv_profile_access WHERE assignment_id=$1 AND profile_id=$2 AND enabled)").bind(id).bind(case.standard).fetch_one(&case.pool).await?;
    anyhow::ensure!(kept, "failed replacement removed profile policy");
    sqlx::query("DROP TRIGGER fixture_assignment_failure ON live_tv_assignments")
        .execute(&case.pool)
        .await?;
    sqlx::query("DROP FUNCTION fixture_assignment_failure()")
        .execute(&case.pool)
        .await?;

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let network_url = format!(
        "http://127.0.0.1:{}/feed?password=no-network-private",
        listener.local_addr()?.port()
    );
    case.secrets.push(network_url.clone());
    let rotated=http::call(&case.app,Some(&case.admin),"PATCH",&case.admin_path(),Some(json!({"operation":"rotate_credentials","playlist_url":network_url,"guide_url":http::ROTATED_GUIDE})),&[("if-match",&etag)]).await?.expect(StatusCode::OK)?;
    anyhow::ensure!(
        !rotated.value.to_string().contains("no-network-private"),
        "rotated URL appeared in metadata"
    );
    anyhow::ensure!(
        tokio::time::timeout(std::time::Duration::from_millis(100), listener.accept())
            .await
            .is_err(),
        "saving source performed upstream networking"
    );
    anyhow::ensure!(
        rotated.value["assignment_id"] == before.value["assignment_id"]
            && rotated.value["owner_max_live_streams"] == 2,
        "credential rotation discarded identity/policy"
    );
    http::call(&case.app, Some(&case.owner), "GET", http::OWNER, None, &[])
        .await?
        .expect(StatusCode::FORBIDDEN)?;
    let kept: bool = sqlx::query_scalar(
        "SELECT enabled FROM live_tv_profile_access WHERE assignment_id=$1 AND profile_id=$2",
    )
    .bind(id)
    .bind(case.standard)
    .fetch_one(&case.pool)
    .await?;
    anyhow::ensure!(kept, "credential rotation reset profile policy");
    case.owner_proof(false).await?;
    let etag = case.admin_read().await?.etag()?;
    let replacement = http::call(
        &case.app,
        Some(&case.admin),
        "PATCH",
        &case.admin_path(),
        Some(replace),
        &[("if-match", &etag)],
    )
    .await?
    .expect(StatusCode::OK)?;
    anyhow::ensure!(
        replacement.value["assignment_id"] != before.value["assignment_id"]
            && replacement.value["revision"] == 1
            && replacement.value["owner_max_live_streams"].is_null()
            && replacement.value["allow_owner_profile_access"] == true,
        "replacement failed to reset owner state or preserve administrator allowance"
    );
    let old_rows: i64 =
        sqlx::query_scalar("SELECT count(*) FROM live_tv_profile_access WHERE assignment_id=$1")
            .bind(id)
            .fetch_one(&case.pool)
            .await?;
    anyhow::ensure!(old_rows == 0, "replacement retained old profile bindings");
    case.owner_proof(false).await?;
    let profiles = http::call(
        &case.app,
        Some(&case.owner),
        "GET",
        &format!("{}/profiles", http::OWNER),
        None,
        &[],
    )
    .await?
    .expect(StatusCode::OK)?;
    anyhow::ensure!(
        profiles.value["items"]
            .as_array()
            .is_some_and(|v| v.iter().all(|v| v["enabled"] == false)),
        "replacement enabled profiles by default"
    );
    case.owner_change(
        json!({"operation":"set_profile_access","profile_id":case.standard,"enabled":true}),
    )
    .await?
    .expect(StatusCode::OK)?;
    let new_id = Uuid::parse_str(
        replacement.value["assignment_id"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("replacement identity missing"))?,
    )?;
    sqlx::query("DELETE FROM user_profiles WHERE id=$1")
        .bind(case.standard)
        .execute(&case.pool)
        .await?;
    let rows: i64 =
        sqlx::query_scalar("SELECT count(*) FROM live_tv_profile_access WHERE assignment_id=$1")
            .bind(new_id)
            .fetch_one(&case.pool)
            .await?;
    anyhow::ensure!(rows == 0, "deleted profile retained assignment policy");
    let etag = case.admin_read().await?.etag()?;
    http::call(
        &case.app,
        Some(&case.admin),
        "DELETE",
        &case.admin_path(),
        None,
        &[("if-match", &etag)],
    )
    .await?
    .expect(StatusCode::NO_CONTENT)?;
    http::call(&case.app, Some(&case.owner), "GET", http::OWNER, None, &[])
        .await?
        .expect(StatusCode::FORBIDDEN)?;
    case.owner_proof(false).await?;
    http::call(&case.app, Some(&case.owner), "GET", http::OWNER, None, &[])
        .await?
        .expect(StatusCode::NOT_FOUND)?;
    let rows: i64 =
        sqlx::query_scalar("SELECT count(*) FROM live_tv_assignments WHERE household_user_id=$1")
            .bind(case.owner.user)
            .fetch_one(&case.pool)
            .await?;
    anyhow::ensure!(rows == 0, "assignment deletion retained source secrets");
    let own_path = format!("/api/v1/live-tv/households/{}/assignment", case.admin.user);
    let own = http::call(
        &case.app,
        Some(&case.admin),
        "PUT",
        &own_path,
        Some(json!({"playlist_url":http::PLAYLIST,"guide_url":http::GUIDE})),
        &[("if-none-match", "*")],
    )
    .await?
    .expect(StatusCode::CREATED)?;
    http::call(
        &case.app,
        Some(&case.admin),
        "DELETE",
        &own_path,
        None,
        &[("if-match", &own.etag()?)],
    )
    .await?
    .expect(StatusCode::FORBIDDEN)?;
    case.admin_proof(false).await?;
    http::call(
        &case.app,
        Some(&case.admin),
        "DELETE",
        &own_path,
        None,
        &[("if-match", &own.etag()?)],
    )
    .await?
    .expect(StatusCode::NO_CONTENT)?;
    case.admin_proof(false).await?;
    sqlx::query("UPDATE users SET status='disabled' WHERE id=$1")
        .bind(case.owner.user)
        .execute(&case.pool)
        .await?;
    let inactive = http::call(
        &case.app,
        Some(&case.admin),
        "PUT",
        &case.admin_path(),
        Some(json!({"playlist_url":http::PLAYLIST,"guide_url":http::GUIDE})),
        &[("if-none-match", "*")],
    )
    .await?
    .expect(StatusCode::CREATED)?;
    http::call(
        &case.app,
        Some(&case.admin),
        "DELETE",
        &case.admin_path(),
        None,
        &[("if-match", &inactive.etag()?)],
    )
    .await?
    .expect(StatusCode::NO_CONTENT)?;
    sqlx::query("UPDATE users SET status='active' WHERE id=$1")
        .bind(case.owner.user)
        .execute(&case.pool)
        .await?;
    Ok(())
}

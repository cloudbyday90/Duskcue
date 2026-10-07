use axum::Router;
use axum::http::StatusCode;
use duskcue::state::AppState;
use serde_json::{Value, json};
use std::path::Path;
use uuid::Uuid;

use super::{insert_play, post, request};

pub(super) struct DeletedProfileFixture<'a> {
    pub(super) app: &'a Router,
    pub(super) pool: &'a sqlx::PgPool,
    pub(super) state: &'a AppState,
    pub(super) token: &'a str,
    pub(super) peer_token: &'a str,
    pub(super) user: Uuid,
    pub(super) item: Uuid,
    pub(super) library: Uuid,
    pub(super) file: Uuid,
    pub(super) source: &'a Path,
}

pub(super) async fn deleted_profile_cleanup(
    fixture: DeletedProfileFixture<'_>,
) -> anyhow::Result<()> {
    let before: i64 = sqlx::query_scalar(
        "SELECT COALESCE(sum(play_count),0)::bigint FROM user_item_data WHERE user_id=$1",
    )
    .bind(fixture.user)
    .fetch_one(fixture.pool)
    .await?;
    for failed_stop_first in [false, true] {
        let profile = Uuid::now_v7();
        sqlx::query("INSERT INTO user_profiles(id,owner_user_id,name,is_default) VALUES ($1,$2,'Deleted stop profile',false)")
            .bind(profile).bind(fixture.user).execute(fixture.pool).await?;
        let play = insert_play(
            fixture.pool,
            fixture.user,
            profile,
            fixture.item,
            fixture.library,
            fixture.file,
        )
        .await?;
        let transcode = fixture
            .state
            .transcode_manager
            .start_remux_session(
                fixture.file,
                fixture.user,
                fixture.source.to_owned(),
                "h264".into(),
                (320, 180),
                "aac".into(),
                Some(2),
                &fixture.state.bootstrap.data_dir,
            )
            .await?;
        sqlx::query("UPDATE play_sessions SET metadata=metadata || $2 WHERE id=$1")
            .bind(play)
            .bind(json!({"transcode_session_id":transcode.id}))
            .execute(fixture.pool)
            .await?;
        let mut frozen = None;
        let mut archive = None;
        if failed_stop_first {
            tokio::time::timeout(std::time::Duration::from_secs(10), async {
                loop {
                    if fixture
                        .state
                        .transcode_manager
                        .get_session(&transcode.id)
                        .is_some_and(|session| session.is_complete)
                    {
                        break;
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
                }
            })
            .await?;
            let root = fixture.state.bootstrap.data_dir.canonicalize()?;
            let actual = transcode.segment_dir.canonicalize()?;
            anyhow::ensure!(
                actual.starts_with(&root),
                "owned cache is outside its fixture root"
            );
            let saved = actual.with_file_name(format!("{}-preserved", transcode.id));
            anyhow::ensure!(
                saved.parent().unwrap().canonicalize()?.starts_with(&root),
                "saved cache is outside its fixture root"
            );
            tokio::time::timeout(std::time::Duration::from_secs(5), async {
                loop {
                    match tokio::fs::rename(&actual, &saved).await {
                        Ok(()) => break Ok::<_, std::io::Error>(()),
                        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => {
                            tokio::time::sleep(std::time::Duration::from_millis(20)).await
                        }
                        Err(error) => break Err(error),
                    }
                }
            })
            .await??;
            tokio::fs::write(&actual, b"owned cache failure fixture").await?;
            archive = Some(saved);
            assert_eq!(
                post(
                    fixture.app,
                    fixture.token,
                    "/api/v1/playback/stop",
                    json!({"session_id":play,"position_ms":7500})
                )
                .await?
                .0,
                StatusCode::INTERNAL_SERVER_ERROR
            );
            frozen = Some(
                sqlx::query_scalar::<_, Value>(
                    "SELECT metadata -> 'duskcue_stop_result_v1' FROM play_sessions WHERE id=$1",
                )
                .bind(play)
                .fetch_one(fixture.pool)
                .await?,
            );
            assert!(
                fixture
                    .state
                    .transcode_manager
                    .get_session(&transcode.id)
                    .is_some()
            );
        }
        assert_eq!(
            request(
                fixture.app,
                fixture.token,
                "DELETE",
                &format!("/api/v1/profiles/{profile}"),
                json!({})
            )
            .await?
            .0,
            StatusCode::OK
        );
        let current: Option<Uuid> =
            sqlx::query_scalar("SELECT profile_id FROM play_sessions WHERE id=$1")
                .bind(play)
                .fetch_one(fixture.pool)
                .await?;
        assert!(current.is_none());
        assert_eq!(
            post(
                fixture.app,
                fixture.peer_token,
                "/api/v1/playback/stop",
                json!({"session_id":play,"position_ms":7500}),
            )
            .await?
            .0,
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            request(
                fixture.app,
                fixture.token,
                "GET",
                &format!("/api/v1/transcode/{}/manifest.m3u8", transcode.id),
                json!({}),
            )
            .await?
            .0,
            StatusCode::NOT_FOUND
        );
        if let Some(saved) = archive {
            tokio::fs::remove_file(&transcode.segment_dir).await?;
            tokio::fs::rename(saved, &transcode.segment_dir).await?;
        }
        if !failed_stop_first {
            for path in ["/api/v1/playback/heartbeat", "/api/v1/playback/seek"] {
                assert_eq!(
                    post(
                        fixture.app,
                        fixture.token,
                        path,
                        json!({"session_id":play,"position_ms":1000})
                    )
                    .await?
                    .0,
                    StatusCode::NOT_FOUND
                );
            }
        }
        let stopped = post(
            fixture.app,
            fixture.token,
            "/api/v1/playback/stop",
            json!({"session_id":play,"position_ms":7500}),
        )
        .await?;
        assert_eq!(stopped.0, StatusCode::OK);
        if let Some(frozen) = frozen {
            for key in [
                "duration_seconds",
                "percent_complete",
                "is_watched",
                "play_count",
            ] {
                assert_eq!(stopped.1[key], frozen[key]);
            }
            assert_eq!(stopped.1["is_watched"], json!(true));
            assert_eq!(stopped.1["play_count"], json!(1));
        } else {
            assert_eq!(stopped.1["is_watched"], json!(false));
            assert_eq!(stopped.1["play_count"], json!(0));
        }
        assert_eq!(
            post(
                fixture.app,
                fixture.token,
                "/api/v1/playback/stop",
                json!({"session_id":play,"position_ms":1})
            )
            .await?,
            stopped
        );
        assert!(
            fixture
                .state
                .transcode_manager
                .get_session(&transcode.id)
                .is_none()
        );
        assert!(!tokio::fs::try_exists(&transcode.segment_dir).await?);
        let recreated: i64 =
            sqlx::query_scalar("SELECT count(*) FROM user_item_data WHERE profile_id=$1")
                .bind(profile)
                .fetch_one(fixture.pool)
                .await?;
        assert_eq!(recreated, 0);
        let events: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM play_events WHERE play_session_id=$1 AND event_type='stop'",
        )
        .bind(play)
        .fetch_one(fixture.pool)
        .await?;
        assert_eq!(events, 1);
    }
    let after: i64 = sqlx::query_scalar(
        "SELECT COALESCE(sum(play_count),0)::bigint FROM user_item_data WHERE user_id=$1",
    )
    .bind(fixture.user)
    .fetch_one(fixture.pool)
    .await?;
    assert_eq!(before, after);
    Ok(())
}

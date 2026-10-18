// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::{
    AppState, DirectEvidence, ProgressiveFixture, Router, StatusCode, Uuid, Value,
    direct_start_request, hint, json, parse_uuid, post, snapshot, stop,
};

pub(super) async fn legacy_bookkeeping(
    fixture: &ProgressiveFixture<'_>,
    state: &AppState,
    app: &Router,
    evidence: &mut DirectEvidence,
) -> anyhow::Result<()> {
    for version in [None, Some(2)] {
        let mut request = direct_start_request(evidence.owned.item, evidence.owned.file);
        if let Some(version) = version {
            request["direct_recovery_protocol"] = json!(version);
        } else {
            request
                .as_object_mut()
                .unwrap()
                .remove("direct_recovery_protocol");
        }
        let (status, start) = post(app, fixture.token, "/api/v1/playback/start", request).await?;
        anyhow::ensure!(
            status == StatusCode::OK
                && start["stream_decision"] == "direct_play"
                && start["stream_url"] == format!("/api/v1/stream/{}", evidence.owned.file)
                && start["direct_source_revision"].is_null()
                && start["direct_fallback"].is_null()
                && start["transcode_session_id"].is_null(),
            "legacy protocol {version:?} did not preserve native direct admission: {start}"
        );
        let play = parse_uuid(&start, "session_id")?;
        let initial = snapshot(fixture.pool, play).await?;
        for (position, requested) in [(1000, false), (1500, true)] {
            let revision = requested.then(Uuid::now_v7);
            let (status, response) = post(
                app,
                fixture.token,
                "/api/v1/playback/heartbeat",
                hint(play, revision, position, requested, true, "playing"),
            )
            .await?;
            anyhow::ensure!(
                status == StatusCode::OK
                    && response["position_ms"] == position
                    && response["direct_source_revision"].is_null()
                    && response["direct_fallback"].is_null()
                    && response["transcode_session_id"].is_null()
                    && response["recovery"].is_null()
                    && state.transcode_manager.active_session_count() == 0,
                "legacy protocol {version:?} heartbeat lost progress or allocated recovery: {response}"
            );
            let (metadata, events) = snapshot(fixture.pool, play).await?;
            anyhow::ensure!(
                metadata["current_position_ms"] == position
                    && metadata["current_state"] == "playing"
                    && metadata["last_heartbeat_at"].is_string()
                    && metadata.get("direct_source_context").is_none()
                    && metadata.get("direct_source_revision").is_none()
                    && metadata.get("recovery_budget").is_none()
                    && events > initial.1,
                "legacy protocol {version:?} did not commit ordinary checkpoint bookkeeping"
            );
            let checkpoint: (i32, Option<Uuid>) = sqlx::query_as(
                "SELECT resume_position_ms,last_played_media_file_id FROM user_item_data \
                 WHERE profile_id=$1 AND media_item_id=$2",
            )
            .bind(fixture.original_profile)
            .bind(evidence.owned.item)
            .fetch_one(fixture.pool)
            .await?;
            anyhow::ensure!(checkpoint == (position, Some(evidence.owned.file)));
        }
        stop(app, fixture.token, play).await?;
    }
    evidence.proof["legacy_client_bookkeeping_without_promotion"] = Value::Bool(true);
    Ok(())
}

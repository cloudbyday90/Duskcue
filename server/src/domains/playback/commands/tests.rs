// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::types::StartRequest;
use serde_json::json;
use uuid::Uuid;

mod actors;
mod adversity;
mod boundaries;
mod cases;
#[path = "../../../../tests/support/admission_commit_proxy.rs"]
#[allow(dead_code)]
mod commit_proxy;
#[path = "../../../../tests/support/retention_database.rs"]
#[allow(dead_code)]
mod database;
mod faults;
#[path = "../../../../tests/support/playback_admission_fixture.rs"]
#[allow(dead_code)]
mod fixture;
mod support;

#[test]
fn complete_intent_canonicalizes_objects_and_defaults_but_preserves_array_order() {
    let id = Uuid::now_v7();
    let item = Uuid::now_v7();
    let a=StartRequest{request_id:id,selection:json!({"media_item_id":item,"device_profile":{"video_codecs":["h264","hevc"],"max_resolution":"1080p"}})}.intent().unwrap();
    let b=StartRequest{request_id:id,selection:json!({"force_hls":false,"playback_mode":"interactive","media_item_id":item,"device_profile":{"max_resolution":"1080p","video_codecs":["h264","hevc"]}})}.intent().unwrap();
    assert_eq!(a.hash, b.hash);
    let c=StartRequest{request_id:id,selection:json!({"media_item_id":item,"device_profile":{"video_codecs":["hevc","h264"],"max_resolution":"1080p"}})}.intent().unwrap();
    assert_ne!(a.hash, c.hash);
}

#[test]
fn start_rejects_unsupported_and_unbounded_intent_before_registration() {
    let item = Uuid::now_v7();
    for selection in [
        json!({"media_item_id":item,"unknown":true}),
        json!({"media_item_id":item,"audio_stream_index":-1}),
        json!({"media_item_id":item,"max_streaming_bitrate":u64::MAX}),
        json!({"media_item_id":item,"source_capabilities":{"version":2}}),
        json!({"media_item_id":item,"device_profile":{"data":"x".repeat(32768)}}),
    ] {
        assert!(
            StartRequest {
                request_id: Uuid::now_v7(),
                selection
            }
            .intent()
            .is_err()
        );
    }
    assert!(
        StartRequest {
            request_id: Uuid::nil(),
            selection: json!({"media_item_id":item})
        }
        .intent()
        .is_err()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires owned disposable PostgreSQL18 and actual FFmpeg source fixtures"]
async fn command_protocol_retains_scope_readiness_delivery_and_confirmed_cleanup()
-> anyhow::Result<()> {
    tokio::time::timeout(std::time::Duration::from_secs(600), async {
        database::run(cases::direct).await?;
        println!("VOD_COMMAND_CASE direct passed");
        database::run(cases::hls).await?;
        println!("VOD_COMMAND_CASE hls passed");
        database::run(cases::policy).await?;
        println!("VOD_COMMAND_CASE policy passed");
        database::run(adversity::concurrency).await?;
        println!("VOD_COMMAND_CASE concurrency passed");
        database::run(adversity::transcode_cap).await?;
        println!("VOD_COMMAND_CASE transcode_cap passed");
        database::run(adversity::observer_loss).await?;
        println!("VOD_COMMAND_CASE observer_loss passed");
        database::run(adversity::cleanup_retry).await?;
        println!("VOD_COMMAND_CASE cleanup_retry passed");
        database::run(adversity::bodies).await?;
        println!("VOD_COMMAND_CASE bodies passed");
        database::run(actors::sessions).await?;
        println!("VOD_COMMAND_CASE sessions passed");
        database::run(actors::bearer).await?;
        println!("VOD_COMMAND_CASE bearer passed");
        database::run(actors::profile_cleanup).await?;
        println!("VOD_COMMAND_CASE profile_cleanup passed");
        database::run(actors::source_revocation).await?;
        println!("VOD_COMMAND_CASE source_revocation passed");
        database::run(actors::credential_revocation).await?;
        println!("VOD_COMMAND_CASE credential_revocation passed");
        database::run(actors::kids_local).await?;
        println!("VOD_COMMAND_CASE kids_local passed");
        database::run(boundaries::policies).await?;
        println!("VOD_COMMAND_CASE policy_writes passed");
        database::run(boundaries::ambient).await?;
        println!("VOD_COMMAND_CASE ambient passed");
        database::run(boundaries::retirement).await?;
        println!("VOD_COMMAND_CASE output_retirement passed");
        database::run(boundaries::foreign).await?;
        println!("VOD_COMMAND_CASE foreign_owner passed");
        for (needle, before, begin) in [
            (&b"INSERT INTO playback_vod_commands"[..], true, false),
            (&b"INSERT INTO playback_vod_commands"[..], false, false),
            (
                &b"UPDATE playback_vod_commands SET state='ready'"[..],
                true,
                false,
            ),
            (
                &b"UPDATE playback_vod_commands SET state='ready'"[..],
                false,
                false,
            ),
            (&b"SET state='released'"[..], true, false),
            (&b"SET state='released'"[..], false, false),
            (&b"unused"[..], false, true),
        ] {
            database::run(async |db| faults::wire(db, needle, before, begin).await).await?;
            println!("VOD_COMMAND_CASE wire_fault passed");
        }
        database::run(faults::completion_loss).await?;
        println!("VOD_COMMAND_CASE sql_completion_loss passed");
        database::run(boundaries::expiry).await?;
        println!("VOD_COMMAND_CASE lease_expiry passed");
        database::run(boundaries::root_shutdown).await?;
        println!("VOD_COMMAND_CASE root_shutdown passed");
        Ok::<_, anyhow::Error>(())
    })
    .await?
}

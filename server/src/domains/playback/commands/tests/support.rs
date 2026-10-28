// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::{database, fixture};
use crate::{
    domains::{auth, playback},
    extractors::{AuthenticatedUser, CredentialTransport},
    services::playback_admission::{AdmissionOwner, Subject},
};
use anyhow::Context;
use serde_json::{Value, json};
use std::{path::PathBuf, sync::Arc, time::Duration};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

pub(super) struct Fixture {
    pub base: fixture::Fixture,
    pub owner: Arc<AdmissionOwner>,
    pub file: Uuid,
    pub item: Uuid,
    pub policy: Uuid,
    pub source: PathBuf,
    pub token: String,
    pub context: String,
    pub client: reqwest::Client,
    pub url: String,
    stop: CancellationToken,
    server: Option<tokio::task::JoinHandle<std::io::Result<()>>>,
}
impl Fixture {
    pub async fn new(db: &database::Database) -> anyhow::Result<Self> {
        Self::with_owner_pool(db, None).await
    }
    pub async fn with_owner_pool(
        db: &database::Database,
        owner_pool: Option<sqlx::PgPool>,
    ) -> anyhow::Result<Self> {
        Self::with_options(db, owner_pool, false).await
    }
    #[cfg(target_os = "linux")]
    pub async fn paced(db: &database::Database) -> anyhow::Result<Self> {
        Self::with_options(db, None, true).await
    }
    async fn with_options(
        db: &database::Database,
        owner_pool: Option<sqlx::PgPool>,
        paced: bool,
    ) -> anyhow::Result<Self> {
        database::upgrade(&db.pool).await?;
        sqlx::query("CREATE TABLE play_sessions_commands_test PARTITION OF play_sessions DEFAULT")
            .execute(&db.pool)
            .await?;
        sqlx::query("CREATE TABLE play_events_commands_test PARTITION OF play_events DEFAULT")
            .execute(&db.pool)
            .await?;
        let mut base = fixture::Fixture::new(db).await?;
        let Subject::Vod { file, item } = base.subject else {
            anyhow::bail!("VOD fixture required");
        };
        let policy = Uuid::now_v7();
        sqlx::query("INSERT INTO streaming_policies(id,name,max_streams,max_transcode_streams) VALUES ($1,$2,4,2)").bind(policy).bind(format!("command-{policy}")).execute(&db.pool).await?;
        sqlx::query("UPDATE users SET streaming_policy_id=$2 WHERE id=$1")
            .bind(base.actor.user)
            .bind(policy)
            .execute(&db.pool)
            .await?;
        let input = PathBuf::from(std::env::var(if paced {
            "DUSKCUE_TEST_CONFIGURATION_SOURCE"
        } else {
            "DUSKCUE_TEST_PLAYBACK_SOURCE"
        })?)
        .canonicalize()?;
        let metadata = tokio::fs::symlink_metadata(&input).await?;
        anyhow::ensure!(
            metadata.is_file()
                && !metadata.file_type().is_symlink()
                && metadata.len() > 0
                && metadata.len() <= 16 * 1024 * 1024
        );
        tokio::fs::create_dir_all(&base.state.bootstrap.data_dir).await?;
        let source = base.state.bootstrap.data_dir.join("source.mkv");
        tokio::fs::copy(input, &source).await?;
        let metadata = tokio::fs::metadata(&source).await?;
        let modified: chrono::DateTime<chrono::Utc> = metadata.modified()?.into();
        let streams = json!({"video":{"index":1,"width":320,"height":180,"bit_depth":8,"frame_rate":24,"profile":"High","level":31},"audio":[{"index":0,"codec":"aac","channels":2,"bitrate":96000,"disposition":{"default":1}}],"subtitles":[]});
        sqlx::query("UPDATE media_files SET file_path=$2,file_size=$3,updated_at=clock_timestamp(),file_modified_at=$4,container_format='matroska',video_codec='h264',video_resolution='480p',video_bitrate=300000,video_dynamic_range='sdr',audio_codec='aac',audio_channels=2,audio_bitrate=96000,runtime_seconds=8,additional_streams=$5 WHERE id=$1")
            .bind(file).bind(source.to_string_lossy().as_ref()).bind(i64::try_from(metadata.len())?).bind(modified).bind(streams).execute(&db.pool).await?;
        let token = auth::service::generate_session_token();
        let hash = ring::digest::digest(&ring::digest::SHA256, token.as_bytes())
            .as_ref()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        sqlx::query("UPDATE user_sessions SET token_hash=$2 WHERE id=$1")
            .bind(base.actor.session)
            .bind(hash)
            .execute(&db.pool)
            .await?;
        let user = AuthenticatedUser {
            user_id: base.actor.user,
            profile_id: base.actor.profile,
            session_id: base.actor.session,
            credential_version: 1,
            transport: CredentialTransport::Cookie,
            profile_selection_required: false,
            device_id: None,
            capabilities: vec!["play_media".into()],
            role: "member".into(),
            has_all_library_access: true,
            display_name: "Qualification".into(),
        };
        let context = auth::service::session_context::current(&db.pool, &user)
            .await?
            .context_id;
        if let Some(pool) = owner_pool {
            base.state.pool = pool;
        }
        #[cfg(target_os = "linux")]
        if paced {
            let source = source.canonicalize()?;
            let input = crate::services::transcoding::qualification::OwnedPacedInput::new(
                source.parent().unwrap(),
                &source,
            )
            .await?;
            base.state.transcode_manager = Arc::new(
                base.state
                    .transcode_manager
                    .as_ref()
                    .clone()
                    .with_qualification_input(input),
            );
        }
        let owner = AdmissionOwner::attach(&base.state).await?;
        let app = playback::commands::router(base.state.clone())
            .merge(playback::router(base.state.clone()))
            .with_state(base.state.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let url = format!("http://{}", listener.local_addr()?);
        let stop = CancellationToken::new();
        let shutdown = stop.clone();
        let server = tokio::spawn(async move {
            axum::serve(
                listener,
                app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
            )
            .with_graceful_shutdown(shutdown.cancelled_owned())
            .await
        });
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(20))
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .build()?;
        Ok(Self {
            base,
            owner,
            file,
            item,
            policy,
            source,
            token,
            context,
            client,
            url,
            stop,
            server: Some(server),
        })
    }
    pub fn start_body(&self, id: Uuid, hls: bool, encode: bool) -> Value {
        json!({"protocol":"duskcue-vod-command/2","request_id":id,"selection":{"media_item_id":self.item,"media_file_id":self.file,"force_hls":hls,"force_transcode":encode,"device_profile":{"video_codecs":["h264"],"audio_codecs":["aac"],"containers":["matroska","mp4"],"max_resolution":"1080p","max_bitrate_bps":20000000,"max_audio_channels":2}}})
    }
    pub async fn response(
        &self,
        method: reqwest::Method,
        path: &str,
        body: Option<Value>,
        bearer: bool,
    ) -> anyhow::Result<reqwest::Response> {
        let mut request = self
            .client
            .request(method, format!("{}{path}", self.url))
            .header("x-session-context", &self.context);
        request = if bearer {
            request.bearer_auth(&self.token)
        } else {
            request.header("cookie", format!("session={}", self.token))
        };
        if let Some(body) = body {
            request = request.json(&body);
        }
        Ok(request.send().await?)
    }
    pub async fn json(
        &self,
        method: reqwest::Method,
        path: &str,
        body: Option<Value>,
    ) -> anyhow::Result<(reqwest::StatusCode, Value)> {
        let response = self.response(method, path, body, false).await?;
        anyhow::ensure!(
            response
                .headers()
                .get("cache-control")
                .is_some_and(|v| v.to_str().is_ok_and(|v| v.contains("no-store")))
        );
        let status = response.status();
        let bytes = response.bytes().await?;
        anyhow::ensure!(bytes.len() <= 32768);
        let value = serde_json::from_slice(&bytes).context("bounded command JSON required")?;
        Ok((status, value))
    }
    pub async fn start(&self, id: Uuid, hls: bool, encode: bool) -> anyhow::Result<Value> {
        let (status, value) = self
            .json(
                reqwest::Method::POST,
                "/api/v1/playback-commands",
                Some(self.start_body(id, hls, encode)),
            )
            .await?;
        anyhow::ensure!(
            status == reqwest::StatusCode::ACCEPTED,
            "start refused: {value}"
        );
        anyhow::ensure!(value["protocol"] == "duskcue-vod-command/2");
        if value["status"] == "preparing" {
            anyhow::ensure!(value["stream_url"].is_null() && value["ready"].is_null());
        }
        Ok(value)
    }
    pub async fn ready(&self, id: Uuid) -> anyhow::Result<Value> {
        tokio::time::timeout(Duration::from_secs(20), async {
            loop {
                let (status, value) = self
                    .json(
                        reqwest::Method::GET,
                        &format!("/api/v1/playback-commands/{id}"),
                        None,
                    )
                    .await?;
                anyhow::ensure!(status.is_success(), "status refused: {value}");
                if value["status"] == "ready" {
                    return Ok::<_, anyhow::Error>(value);
                }
                anyhow::ensure!(
                    value["status"] == "preparing" || value["status"] == "uncertain",
                    "preparation stopped: {value}"
                );
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
        })
        .await?
    }
    pub async fn stop(&self, id: Uuid) -> anyhow::Result<Value> {
        let (status, _) = self
            .json(
                reqwest::Method::POST,
                &format!("/api/v1/playback-commands/{id}/stop"),
                Some(json!({})),
            )
            .await?;
        anyhow::ensure!(status == reqwest::StatusCode::ACCEPTED);
        tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                let state: String =
                    sqlx::query_scalar("SELECT state FROM playback_reservations WHERE id=$1")
                        .bind(id)
                        .fetch_one(&self.base.state.pool)
                        .await?;
                if state == "released" {
                    return Ok::<_, anyhow::Error>(json!(state));
                }
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
        })
        .await?
    }
    pub async fn close_http(&mut self) -> anyhow::Result<()> {
        self.stop.cancel();
        if let Some(server) = self.server.take() {
            server.await??;
        }
        Ok(())
    }
    pub async fn close(&mut self) -> anyhow::Result<()> {
        self.owner.close();
        self.base.state.transcode_manager.close();
        self.stop.cancel();
        tokio::time::timeout(Duration::from_secs(15), async {
            self.close_http().await?;
            let admission = self.owner.drain().await;
            let output = self.base.state.transcode_manager.drain().await;
            admission?;
            output?;
            Ok::<_, anyhow::Error>(())
        })
        .await?
    }
}
pub(super) async fn run(
    db: &database::Database,
    case: impl AsyncFnOnce(&Fixture) -> anyhow::Result<()>,
) -> anyhow::Result<()> {
    let mut fixture = Fixture::new(db).await?;
    let result = case(&fixture).await;
    let cleanup = fixture.close().await;
    result?;
    cleanup
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.owner.close();
        self.base.state.transcode_manager.close();
        self.stop.cancel();
    }
}

// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use duskcue::{
    domains::household_security::types::Scope,
    extractors::CredentialTransport,
    services::playback_admission::{AdmissionError, AdmissionOwner, Request, Subject},
    state::AppState,
};
use sqlx::PgPool;
use std::{sync::Arc, time::Duration};
use uuid::Uuid;

pub struct Fixture {
    pub state: AppState,
    pub actor: Scope,
    pub subject: Subject,
}
impl Fixture {
    pub async fn new(db: &super::database::Database) -> anyhow::Result<Self> {
        let state = db.state();
        let user = Uuid::now_v7();
        let profile = Uuid::now_v7();
        let session = Uuid::now_v7();
        let library = Uuid::now_v7();
        let item = Uuid::now_v7();
        let file = Uuid::now_v7();
        sqlx::query("INSERT INTO users(id,username,display_name,role,has_all_library_access,max_streams) VALUES ($1,$2,'Qualification','member',true,2)").bind(user).bind(format!("admission-{user}")).execute(&db.pool).await?;
        sqlx::query("INSERT INTO user_profiles(id,owner_user_id,name,is_default) VALUES ($1,$2,'Qualification',true)").bind(profile).bind(user).execute(&db.pool).await?;
        sqlx::query("INSERT INTO user_sessions(id,user_id,active_profile_id,token_hash,expires_at) VALUES ($1,$2,$3,$4,clock_timestamp()+interval '1 hour')").bind(session).bind(user).bind(profile).bind(format!("qualification-{session}")).execute(&db.pool).await?;
        sqlx::query("INSERT INTO libraries(id,name,slug,media_type,root_path) VALUES ($1,'Qualification',$2,'movies','fixture')").bind(library).bind(format!("admission-{library}")).execute(&db.pool).await?;
        sqlx::query("INSERT INTO media_items(id,library_id,type,title,sort_title,content_rating) VALUES ($1,$2,'movie','Qualification','Qualification','PG')").bind(item).bind(library).execute(&db.pool).await?;
        sqlx::query("INSERT INTO media_files(id,media_item_id,file_path,file_size,container_format,runtime_seconds) VALUES ($1,$2,$3,1,'mp4',20)").bind(file).bind(item).bind(format!("fixture-{file}")).execute(&db.pool).await?;
        Ok(Self {
            state,
            actor: Scope {
                user,
                profile,
                session,
                version: 1,
                transport: CredentialTransport::Bearer,
                binding: None,
            },
            subject: Subject::Vod { file, item },
        })
    }
    pub fn request(&self) -> Request {
        Request {
            id: Uuid::now_v7(),
            actor: self.actor,
            profile_revision: 1,
            subject: self.subject,
        }
    }
    pub async fn owner(&self) -> Result<Arc<AdmissionOwner>, AdmissionError> {
        AdmissionOwner::new(
            self.state.pool.clone(),
            self.state.encryption_key.clone(),
            None,
        )
        .await
    }
}

pub async fn start(
    owner: &Arc<AdmissionOwner>,
    request: Request,
) -> Result<duskcue::services::playback_admission::Receipt, AdmissionError> {
    owner
        .start(request, |cancel| async move {
            cancel.cancelled().await;
            Ok(())
        })
        .await
}
pub async fn state(pool: &PgPool, id: Uuid) -> anyhow::Result<String> {
    Ok(
        sqlx::query_scalar("SELECT state FROM playback_reservations WHERE id=$1")
            .bind(id)
            .fetch_one(pool)
            .await?,
    )
}
pub async fn released(pool: &PgPool, id: Uuid) -> anyhow::Result<()> {
    tokio::time::timeout(Duration::from_secs(8), async {
        while state(pool, id).await? != "released" {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        Ok::<_, anyhow::Error>(())
    })
    .await??;
    Ok(())
}
pub async fn close(owner: &AdmissionOwner) -> anyhow::Result<()> {
    owner.close();
    tokio::time::timeout(Duration::from_secs(12), owner.drain()).await??;
    Ok(())
}

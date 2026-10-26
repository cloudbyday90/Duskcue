// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

#[path = "management_http.rs"]
mod common;
pub use common::actors::session;
pub use common::*;

use axum::{Router, http::StatusCode};
use duskcue::state::AppState;
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

pub struct Case {
    pub app: Router,
    pub state: AppState,
    pub pool: PgPool,
    pub admin: Actor,
    pub owner: Actor,
    pub other: Actor,
    pub standard: Uuid,
    pub kids: Uuid,
    pub secrets: Vec<String>,
}

impl Case {
    pub fn admin_path(&self) -> String {
        format!("/api/v1/live-tv/households/{}/assignment", self.owner.user)
    }
    pub async fn admin_read(&self) -> anyhow::Result<Reply> {
        call(
            &self.app,
            Some(&self.admin),
            "GET",
            &self.admin_path(),
            None,
            &[],
        )
        .await?
        .expect(StatusCode::OK)
    }
    pub async fn owner_read(&self) -> anyhow::Result<Reply> {
        call(&self.app, Some(&self.owner), "GET", OWNER, None, &[])
            .await?
            .expect(StatusCode::OK)
    }
    pub async fn admin_proof(&mut self, first: bool) -> anyhow::Result<()> {
        unlock(
            &self.app,
            &self.state,
            &mut self.admin,
            first,
            &mut self.secrets,
        )
        .await
    }
    pub async fn owner_proof(&mut self, first: bool) -> anyhow::Result<()> {
        unlock(
            &self.app,
            &self.state,
            &mut self.owner,
            first,
            &mut self.secrets,
        )
        .await
    }
    pub async fn configure(
        &self,
        enabled: bool,
        cap: Option<i32>,
        profiles: bool,
        limit: bool,
    ) -> anyhow::Result<Reply> {
        let etag = self.admin_read().await?.etag()?;
        call(&self.app,Some(&self.admin),"PATCH",&self.admin_path(),Some(json!({"operation":"configure","enabled":enabled,"max_live_streams":cap,"allow_owner_profile_access":profiles,"allow_owner_live_limit":limit})),&[("if-match",&etag)]).await?.expect(StatusCode::OK)
    }
    pub async fn owner_change(&self, body: Value) -> anyhow::Result<Reply> {
        let etag = self.owner_read().await?.etag()?;
        call(
            &self.app,
            Some(&self.owner),
            "PATCH",
            OWNER,
            Some(body),
            &[("if-match", &etag)],
        )
        .await
    }
}

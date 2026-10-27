// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::{AdmissionError, Receipt, Request, Subject, authority, database, policy};
use crate::services::encryption::EncryptionKey;
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

pub(super) enum IdlePolicy {
    Fixed(Option<i32>),
    Runtime(std::sync::Arc<arc_swap::ArcSwap<crate::state::RuntimeConfig>>),
}
impl IdlePolicy {
    fn current(&self) -> Option<i32> {
        match self {
            Self::Fixed(value) => *value,
            Self::Runtime(config) => config.load().auth.session_idle_timeout_hours,
        }
    }
}

pub(super) struct Ledger {
    pub pool: PgPool,
    pub key: std::sync::Arc<EncryptionKey>,
    pub idle: IdlePolicy,
    pub instance: Uuid,
}

impl Ledger {
    pub async fn register(
        pool: PgPool,
        key: std::sync::Arc<EncryptionKey>,
        idle: IdlePolicy,
    ) -> Result<Self, AdmissionError> {
        let instance = Uuid::now_v7();
        database::transaction(&pool, async |conn| {
            sqlx::query("INSERT INTO playback_instances(id) VALUES ($1)")
                .bind(instance)
                .execute(conn)
                .await?;
            Ok(())
        })
        .await?;
        Ok(Self {
            pool,
            key,
            idle,
            instance,
        })
    }

    pub async fn claim(
        &self,
        request: Request,
        allow_insert: bool,
        binding: Option<super::vod::Binding>,
    ) -> Result<Receipt, AdmissionError> {
        let fingerprint = request.encode()?;
        database::transaction(&self.pool,async |conn| {
            database::instance(conn,self.instance,true).await?;
            policy::fence(conn).await?;
            database::household(conn,request.actor.user).await?;
            let now:DateTime<Utc>=sqlx::query_scalar("SELECT clock_timestamp()").fetch_one(&mut *conn).await?;
            if let Some(mut row)=database::row(conn,request.id).await? {
                if row.fingerprint!=fingerprint || row.household_id!=request.actor.user {return Err(AdmissionError::Conflict);}
                super::vod::matches(conn,request.id,binding).await?;
                if row.state=="held" && (row.deadline<=now || authority::verify(conn,&self.key,self.idle.current(),request,Some(&row)).await.is_err()) {
                    sqlx::query("UPDATE playback_reservations SET state='retiring' WHERE id=$1 AND state='held'").bind(row.id).execute(&mut *conn).await?;
                    row.state="retiring".into();
                }
                return row.receipt(now,false,self.instance);
            }
            if !allow_insert {super::retention::database::missing(conn,request.id).await?;return Err(AdmissionError::Indeterminate);}
            let cap=policy::ceiling(conn,request.actor.user).await?;
            if let Some(binding)=binding {
                if !matches!(request.subject, Subject::Vod{..}) {return Err(AdmissionError::Authority);}
                let policy=super::vod::policy(conn,request.actor.user).await?;
                if !policy.permits(binding) || policy.hash()?!=binding.policy {return Err(AdmissionError::Policy);}
                super::vod::ambient(conn,request,binding).await?;
                if binding.encodes {
                    let count:i64=sqlx::query_scalar("SELECT count(*) FROM playback_reservations r JOIN playback_vod_commands c ON c.reservation_id=r.id WHERE r.household_id=$1 AND r.state<>'released' AND c.encodes")
                        .bind(request.actor.user).fetch_one(&mut *conn).await?;
                    if !super::policy::allows(policy.transcode,count) {return Err(AdmissionError::Capacity);}
                }
            }
            let verified=authority::verify(conn,&self.key,self.idle.current(),request,None).await?;
            let (total,live):(i64,i64)=sqlx::query_as("SELECT count(*),count(*) FILTER (WHERE kind='live') FROM playback_reservations WHERE household_id=$1 AND state<>'released'")
                .bind(request.actor.user).fetch_one(&mut *conn).await?;
            let is_live=matches!(request.subject,Subject::Live{..});
            if !policy::allows(cap,total) || (is_live && !policy::allows(verified.cap,live)) {return Err(AdmissionError::Capacity);}
            sqlx::query("INSERT INTO playback_reservations(id,instance_id,household_id,fingerprint,kind,assignment_id,source_generation,channel_key) VALUES ($1,$2,$3,$4,$5,$6,$7,$8)")
                .bind(request.id).bind(self.instance).bind(request.actor.user).bind(fingerprint).bind(if is_live {"live"} else {"vod"})
                .bind(verified.assignment).bind(verified.generation).bind(verified.key).execute(&mut *conn).await?;
            if let Some(binding)=binding {super::vod::insert(conn,request.id,binding).await?;}
            database::row(conn,request.id).await?.ok_or(AdmissionError::Storage)?.receipt(now,true,self.instance)
        }).await
    }

    pub async fn check(
        &self,
        request: Request,
        renew: bool,
        binding: Option<super::vod::Binding>,
    ) -> Result<Receipt, AdmissionError> {
        database::transaction(&self.pool, async |conn| {
            self.check_on(conn, request, renew, binding).await
        })
        .await
    }

    pub(crate) async fn check_on(
        &self,
        conn: &mut sqlx::PgConnection,
        request: Request,
        renew: bool,
        binding: Option<super::vod::Binding>,
    ) -> Result<Receipt, AdmissionError> {
        let fingerprint = request.encode()?;
        database::instance(conn, self.instance, true).await?;
        policy::fence(conn).await?;
        database::household(conn, request.actor.user).await?;
        let row = database::row(conn, request.id)
            .await?
            .ok_or(AdmissionError::Retired)?;
        if row.instance_id != self.instance || row.fingerprint != fingerprint {
            return Err(AdmissionError::Conflict);
        }
        let now: DateTime<Utc> = sqlx::query_scalar("SELECT clock_timestamp()")
            .fetch_one(&mut *conn)
            .await?;
        if row.state != "held" || row.deadline <= now {
            return Err(AdmissionError::Retired);
        }
        policy::ceiling(conn, request.actor.user).await?;
        super::vod::matches(conn, request.id, binding).await?;
        if let Some(binding) = binding {
            super::vod::ambient(conn, request, binding).await?;
            if !super::vod::policy(conn, request.actor.user)
                .await?
                .permits(binding)
            {
                return Err(AdmissionError::Policy);
            }
        }
        authority::verify(conn, &self.key, self.idle.current(), request, Some(&row)).await?;
        if renew {
            sqlx::query("UPDATE playback_reservations SET deadline=clock_timestamp()+interval '60 seconds' WHERE id=$1 AND deadline>clock_timestamp() AND state='held'")
                    .bind(request.id).execute(&mut *conn).await?.rows_affected().eq(&1).then_some(()).ok_or(AdmissionError::Retired)?;
        }
        database::row(conn, request.id)
            .await?
            .ok_or(AdmissionError::Storage)?
            .receipt(now, false, self.instance)
    }

    pub async fn retire(&self, request: Request, release: bool) -> Result<(), AdmissionError> {
        let fingerprint = request.encode()?;
        database::transaction(&self.pool,async |conn| {
            database::instance(conn,self.instance,false).await?;
            sqlx::query("SELECT id FROM users WHERE id=$1 FOR NO KEY UPDATE").bind(request.actor.user).execute(&mut *conn).await?;
            let Some(row)=database::row(conn,request.id).await? else {return Ok(());};
            if row.instance_id!=self.instance || row.fingerprint!=fingerprint {return Err(AdmissionError::Conflict);}
            if release {
                sqlx::query("UPDATE playback_vod_commands SET resources_completed=true,state=CASE WHEN state='preparing' THEN 'failed' WHEN state='ready' THEN 'stopping' ELSE state END WHERE reservation_id=$1").bind(request.id).execute(&mut *conn).await?;
                sqlx::query("UPDATE playback_reservations SET state='released',released_at=COALESCE(released_at,clock_timestamp()) WHERE id=$1").bind(request.id).execute(conn).await?;
            } else {
                sqlx::query("UPDATE playback_reservations SET state='retiring' WHERE id=$1 AND state='held'").bind(request.id).execute(conn).await?;
            }
            Ok(())
        }).await
    }
}

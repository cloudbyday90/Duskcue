// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::{AdmissionError, Binding, Policy};
use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::PgConnection;
use uuid::Uuid;

pub(crate) mod retention;

#[derive(sqlx::FromRow)]
pub(crate) struct ControlRow {
    pub revision: i64,
    pub state: String,
    pub current_output: Option<Uuid>,
    pub pending_operation: Option<Uuid>,
    pub recovery_budget: Value,
    pub checkpoint_output: Option<Uuid>,
    pub checkpoint_sequence: i64,
    pub checkpoint_position: Option<i32>,
    pub playback_state: String,
}

#[derive(sqlx::FromRow)]
pub(crate) struct OutputRow {
    pub id: Uuid,
    pub generation_id: Option<Uuid>,
    pub delivery_mode: String,
    pub encodes: bool,
    pub configuration: Value,
    pub state: String,
    pub result: Option<Value>,
    pub resources_completed: bool,
}

#[derive(sqlx::FromRow)]
pub(crate) struct OperationRow {
    pub id: Uuid,
    pub reservation_id: Uuid,
    pub intent_hash: Vec<u8>,
    pub condition: String,
    pub action: String,
    pub candidate_output: Uuid,
    pub state: String,
    pub result: Option<Value>,
    pub cleanup_at: Option<DateTime<Utc>>,
}

pub(crate) async fn control(
    conn: &mut PgConnection,
    id: Uuid,
) -> Result<ControlRow, AdmissionError> {
    sqlx::query_as("SELECT revision,state,current_output,pending_operation,claim_encodes,recovery_budget,checkpoint_output,checkpoint_sequence,checkpoint_position,playback_state FROM playback_vod_controls WHERE reservation_id=$1 FOR UPDATE")
        .bind(id).fetch_optional(conn).await?.ok_or(AdmissionError::Indeterminate)
}

pub(crate) async fn output(
    conn: &mut PgConnection,
    viewer: Uuid,
    id: Uuid,
) -> Result<OutputRow, AdmissionError> {
    sqlx::query_as("SELECT id,operation_id,generation_id,delivery_mode,encodes,configuration,state,result,failure,resources_completed,cleanup_uncertain FROM playback_vod_outputs WHERE reservation_id=$1 AND id=$2 FOR UPDATE")
        .bind(viewer).bind(id).fetch_optional(conn).await?.ok_or(AdmissionError::Conflict)
}

pub(crate) async fn operation(
    conn: &mut PgConnection,
    id: Uuid,
) -> Result<Option<OperationRow>, AdmissionError> {
    Ok(sqlx::query_as("SELECT id,reservation_id,intent_hash,condition,action,expected_output,candidate_output,state,result,terminal_at,cleanup_at FROM playback_vod_operations WHERE id=$1 FOR UPDATE")
        .bind(id).fetch_optional(conn).await?)
}

pub(crate) async fn missing(conn: &mut PgConnection, id: Uuid) -> Result<(), AdmissionError> {
    let timing:i16=sqlx::query_scalar("SELECT playback_receipt_time_error($1,clock_timestamp(),cutoff) FROM playback_vod_operation_retention WHERE id").bind(id).fetch_one(conn).await?;
    match timing {
        0 => Ok(()),
        1 => Err(AdmissionError::Expired),
        2 => Err(AdmissionError::RequestTime),
        _ => Err(AdmissionError::Conflict),
    }
}

pub(crate) struct NewOutput<'a> {
    pub viewer: Uuid,
    pub id: Uuid,
    pub operation: Option<Uuid>,
    pub generation: Option<Uuid>,
    pub binding: Binding,
    pub configuration: &'a Value,
}

pub(crate) async fn insert_output(
    conn: &mut PgConnection,
    value: NewOutput<'_>,
) -> Result<(), AdmissionError> {
    sqlx::query("INSERT INTO playback_vod_outputs(id,reservation_id,operation_id,generation_id,source_hash,policy_hash,delivery_mode,encodes,source_4k,configuration) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)")
        .bind(value.id).bind(value.viewer).bind(value.operation).bind(value.generation)
        .bind(value.binding.source.as_slice()).bind(value.binding.policy.as_slice()).bind(value.binding.mode.as_str()).bind(value.binding.encodes).bind(value.binding.source_4k).bind(value.configuration).execute(conn).await?;
    Ok(())
}

pub(crate) async fn permitted(
    conn: &mut PgConnection,
    viewer: Uuid,
    policy: &Policy,
) -> Result<bool, AdmissionError> {
    Ok(sqlx::query_scalar("SELECT playback_vod_permitted($1,$2)")
        .bind(viewer)
        .bind(serde_json::to_value(policy).map_err(|_| AdmissionError::Policy)?)
        .fetch_one(conn)
        .await?)
}

pub(crate) async fn complete(conn: &mut PgConnection, viewer: Uuid) -> Result<(), AdmissionError> {
    sqlx::query("SELECT playback_vod_complete_outputs($1)")
        .bind(viewer)
        .execute(conn)
        .await?;
    Ok(())
}

// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::AdmissionError;
use sqlx::{PgConnection, Row};
use uuid::Uuid;

pub(super) async fn ceiling(
    conn: &mut PgConnection,
    user: Uuid,
) -> Result<Option<i32>, AdmissionError> {
    sqlx::query("LOCK TABLE streaming_policies IN SHARE MODE")
        .execute(&mut *conn)
        .await?;
    let row = sqlx::query(
        "SELECT max_streams,max_transcode_streams,streaming_policy_id FROM users WHERE id=$1",
    )
    .bind(user)
    .fetch_one(&mut *conn)
    .await?;
    let assigned: Option<Uuid> = row.try_get("streaming_policy_id")?;
    let user_limit: Option<i32> = row.try_get("max_streams")?;
    let user_transcode: Option<i32> = row.try_get("max_transcode_streams")?;
    if user_transcode.is_some_and(|v| v < 0) {
        return Err(AdmissionError::Policy);
    }
    let policies: Vec<(Option<i32>, Option<i32>)> = if let Some(id) = assigned {
        sqlx::query_as("SELECT max_streams,max_transcode_streams FROM streaming_policies WHERE id=$1 FOR SHARE").bind(id).fetch_all(conn).await?
    } else {
        sqlx::query_as("SELECT max_streams,max_transcode_streams FROM streaming_policies WHERE is_default ORDER BY id LIMIT 2 FOR SHARE").fetch_all(conn).await?
    };
    if policies.len() != 1 || user_limit.is_some_and(|n| n < 0) {
        return Err(AdmissionError::Policy);
    }
    let (total, transcode) = policies[0];
    if total.is_some_and(|n| n < 0) || transcode.is_some_and(|n| n < 0) {
        return Err(AdmissionError::Policy);
    }
    Ok(user_limit.or(total))
}

pub(super) fn allows(ceiling: Option<i32>, count: i64) -> bool {
    ceiling.is_none_or(|limit| count < i64::from(limit))
}

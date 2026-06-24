// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! Background Trakt sync worker — the scheduled iteration layer over the
//! bidirectional sync engine in
//! [`crate::domains::trakt::service::run_sync`].
//!
//! Implements the scheduled task defined in
//! [TRAKT.md](../../docs/design/TRAKT.md) §Scheduled Task:
//!
//! - **Name**: `trakt_sync`
//! - **Interval**: 1800s (30 min)
//! - **Timeout**: 30 min
//! - **Config**: `{}` (empty — per-user sync settings come from `trakt_accounts`)
//!
//! The worker iterates all `trakt_accounts` rows where `sync_enabled = true`
//! and calls [`run_sync`](crate::domains::trakt::service::run_sync) for each
//! user. `run_sync` handles the per-user in-memory lock
//! (`DashMap<Uuid, Instant>` with 15-min TTL), token refresh, pull → merge →
//! push, and `last_full_sync_at` write-back.
//!
//! ## Error classification
//!
//! Trakt API failures are classified as either *global* (abort the batch —
//! every subsequent user would fail identically) or *per-user* (log and
//! continue):
//!
//! | Variant | Class | Rationale |
//! |---|---|---|
//! | `NotConfigured` | Global | Admin hasn't set `client_id`/`client_secret`; affects all users |
//! | `RateLimited` | Global | Trakt API rate limit; retrying for the next user worsens it |
//! | `ServiceUnavailable` | Global | Trakt API down; affects all users |
//! | `Timeout` | Global | Trakt API unreachable; affects all users |
//! | `AccountNotLinked` | Per-user | Race: account unlinked between query and sync |
//! | `TokenExpired` | Per-user | Refresh token revoked; user must re-link |
//! | `SyncInProgress` | Per-user | Manual sync running for this user; skip gracefully |
//! | `Database` | Per-user | Transient DB error scoped to this user's queries |
//!
//! On a global failure the worker stops iterating and logs the abort. The
//! scheduler retries the whole task on the next interval (30 min).
//!
//! ## Deviation from TRAKT.md §Scheduled Task
//!
//! The design doc specifies the candidate query as
//! `WHERE sync_enabled = true AND token_expires_at > now()`. The
//! `token_expires_at > now()` guard is intentionally **not** applied here.
//! `token_expires_at` tracks the *access* token (90-day TTL), but
//! [`ensure_valid_token`](crate::domains::trakt::service::fn.ensure_valid_token)
//! refreshes expired access tokens using the long-lived *refresh* token. A
//! user whose access token has expired but whose refresh token is still valid
//! would be incorrectly skipped by the literal filter, permanently halting
//! their sync after the first 90-day access token lapsed. Filtering only on
//! `sync_enabled = true` lets the token-refresh machinery handle expiry; a
//! truly unrecoverable token surfaces as `TokenExpired` and is skipped
//! per-user.

use sqlx::Row;
use uuid::Uuid;

use crate::domains::trakt::service::run_sync;
use crate::domains::trakt::TraktError;
use crate::state::AppState;

pub async fn run_trakt_sync(state: &AppState, task_id: Uuid, config: serde_json::Value) {
    tracing::info!(task_id = %task_id, "Starting Trakt sync task");

    let user_ids: Vec<Uuid> = if let Some(id) = config
        .get("user_id")
        .and_then(|v| v.as_str())
        .and_then(|s| Uuid::parse_str(s).ok())
    {
        vec![id]
    } else {
        match fetch_sync_enabled_users(&state.pool).await {
            Ok(ids) => ids,
            Err(e) => {
                tracing::error!(
                    task_id = %task_id,
                    error = %e,
                    "Failed to fetch users with Trakt sync enabled"
                );
                return;
            }
        }
    };

    if user_ids.is_empty() {
        tracing::info!(task_id = %task_id, "No users with Trakt sync enabled, nothing to do");
        return;
    }

    tracing::info!(
        task_id = %task_id,
        user_count = user_ids.len(),
        "Syncing Trakt accounts"
    );

    let mut total = AggregateResult::default();
    for user_id in &user_ids {
        match run_sync(state, *user_id).await {
            Ok(summary) => {
                tracing::info!(
                    task_id = %task_id,
                    user_id = %user_id,
                    pulled_watched = summary.pulled_watched,
                    pulled_ratings = summary.pulled_ratings,
                    pulled_collection = summary.pulled_collection,
                    pushed_watched = summary.pushed_watched,
                    unmatched = summary.unmatched,
                    "User Trakt sync complete"
                );
                total.add_sync(&summary);
                total.succeeded += 1;
            }
            Err(e) => {
                total.failed += 1;
                if is_global_failure(&e) {
                    tracing::warn!(
                        task_id = %task_id,
                        user_id = %user_id,
                        error = %e,
                        succeeded_so_far = total.succeeded,
                        failed_so_far = total.failed,
                        "Global Trakt failure — aborting remaining users (will retry next interval)"
                    );
                    total.global_abort = true;
                    break;
                }
                let level = if matches!(e, TraktError::SyncInProgress | TraktError::AccountNotLinked) {
                    "skipped"
                } else {
                    "failed"
                };
                tracing::warn!(
                    task_id = %task_id,
                    user_id = %user_id,
                    error = %e,
                    outcome = level,
                    "User Trakt sync did not complete, continuing with next user"
                );
            }
        }
    }

    tracing::info!(
        task_id = %task_id,
        users = user_ids.len(),
        succeeded = total.succeeded,
        failed = total.failed,
        global_abort = total.global_abort,
        pulled_watched = total.pulled_watched,
        pulled_ratings = total.pulled_ratings,
        pulled_collection = total.pulled_collection,
        pushed_watched = total.pushed_watched,
        unmatched = total.unmatched,
        "Trakt sync task completed"
    );
}

async fn fetch_sync_enabled_users(pool: &sqlx::PgPool) -> Result<Vec<Uuid>, sqlx::Error> {
    let rows = sqlx::query(
        r#"
        SELECT user_id
        FROM trakt_accounts
        WHERE sync_enabled = true
        ORDER BY last_full_sync_at ASC NULLS FIRST
        "#,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows.iter().map(|r| r.get::<Uuid, _>("user_id")).collect())
}

fn is_global_failure(e: &TraktError) -> bool {
    matches!(
        e,
        TraktError::NotConfigured
            | TraktError::RateLimited { .. }
            | TraktError::ServiceUnavailable
            | TraktError::Timeout
    )
}

#[derive(Debug, Default)]
struct AggregateResult {
    succeeded: u64,
    failed: u64,
    global_abort: bool,
    pulled_watched: i64,
    pulled_ratings: i64,
    pulled_collection: i64,
    pushed_watched: i64,
    unmatched: i64,
}

impl AggregateResult {
    fn add_sync(&mut self, other: &crate::domains::trakt::types::SyncSummary) {
        self.pulled_watched += other.pulled_watched;
        self.pulled_ratings += other.pulled_ratings;
        self.pulled_collection += other.pulled_collection;
        self.pushed_watched += other.pushed_watched;
        self.unmatched += other.unmatched;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_global_failure_classifies_api_wide_errors() {
        assert!(is_global_failure(&TraktError::NotConfigured));
        assert!(is_global_failure(&TraktError::ServiceUnavailable));
        assert!(is_global_failure(&TraktError::Timeout));
        assert!(is_global_failure(&TraktError::RateLimited {
            retry_after_secs: None
        }));
        assert!(is_global_failure(&TraktError::RateLimited {
            retry_after_secs: Some(30)
        }));
    }

    #[test]
    fn test_is_global_failure_rejects_per_user_errors() {
        assert!(!is_global_failure(&TraktError::AccountNotLinked));
        assert!(!is_global_failure(&TraktError::TokenExpired));
        assert!(!is_global_failure(&TraktError::SyncInProgress));
        assert!(!is_global_failure(&TraktError::Database(sqlx::Error::PoolClosed)));
    }

    #[test]
    fn test_aggregate_result_accumulates_sync_summaries() {
        let mut agg = AggregateResult::default();
        let a = crate::domains::trakt::types::SyncSummary {
            completed: true,
            pulled_watched: 10,
            pulled_ratings: 3,
            pulled_collection: 5,
            pushed_watched: 2,
            unmatched: 1,
            last_full_sync_at: None,
        };
        let b = crate::domains::trakt::types::SyncSummary {
            pulled_watched: 20,
            pulled_ratings: 7,
            pulled_collection: 15,
            pushed_watched: 4,
            unmatched: 9,
            ..Default::default()
        };

        agg.add_sync(&a);
        agg.add_sync(&b);

        assert_eq!(agg.pulled_watched, 30);
        assert_eq!(agg.pulled_ratings, 10);
        assert_eq!(agg.pulled_collection, 20);
        assert_eq!(agg.pushed_watched, 6);
        assert_eq!(agg.unmatched, 10);
    }

    #[test]
    fn test_aggregate_result_default_is_zeroed() {
        let agg = AggregateResult::default();
        assert_eq!(agg.succeeded, 0);
        assert_eq!(agg.failed, 0);
        assert!(!agg.global_abort);
        assert_eq!(agg.pulled_watched, 0);
        assert_eq!(agg.pushed_watched, 0);
        assert_eq!(agg.unmatched, 0);
    }
}

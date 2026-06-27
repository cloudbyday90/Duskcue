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

use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use croner::Cron;
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use sqlx::Row;
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;
use uuid::Uuid;

type TaskExecutor = Arc<
    dyn Fn(
            sqlx::PgPool,
            Uuid,
            serde_json::Value,
            Duration,
            CancellationToken,
        ) -> tokio::task::JoinHandle<TaskRunOutcome>
        + Send
        + Sync,
>;

pub struct Scheduler {
    pool: sqlx::PgPool,
    executors: Vec<(String, TaskExecutor)>,
    active_tasks: Arc<DashMap<Uuid, CancellationToken>>,
    tick_interval: Duration,
}

#[derive(Debug)]
enum TaskRunOutcome {
    Success,
    Failure(String),
    Cancelled,
    Timeout,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduledTaskRow {
    pub id: Uuid,
    pub name: String,
    pub task_type: String,
    pub cron_expression: Option<String>,
    pub interval_seconds: Option<i32>,
    pub is_enabled: bool,
    pub timeout_seconds: i32,
    pub max_retries: i32,
    pub retry_delay_seconds: i32,
    pub state: String,
    pub consecutive_failures: i32,
    pub last_run_at: Option<DateTime<Utc>>,
    pub last_run_duration_ms: Option<i32>,
    pub last_run_result: Option<String>,
    pub last_error: Option<String>,
    pub next_run_at: Option<DateTime<Utc>>,
    pub config: serde_json::Value,
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduledTaskRunRow {
    pub id: Uuid,
    pub scheduled_task_id: Uuid,
    pub trigger_type: String,
    pub state: String,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub duration_ms: Option<i32>,
    pub result: Option<String>,
    pub error_message: Option<String>,
    pub error_details: Option<serde_json::Value>,
    pub stats: serde_json::Value,
}

#[derive(Debug, thiserror::Error)]
pub enum SchedulerError {
    #[error("Invalid cron expression: {0}")]
    InvalidCron(String),
    #[error("Task not found: {0}")]
    TaskNotFound(Uuid),
    #[error("Task already running: {0}")]
    AlreadyRunning(Uuid),
    #[error("Task executor not registered: {0}")]
    ExecutorNotRegistered(String),
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),
}

impl Scheduler {
    pub fn new(pool: sqlx::PgPool) -> Self {
        Self {
            pool,
            executors: Vec::new(),
            active_tasks: Arc::new(DashMap::new()),
            tick_interval: Duration::from_secs(30),
        }
    }

    pub fn with_tick_interval(mut self, interval: Duration) -> Self {
        self.tick_interval = interval;
        self
    }

    pub fn register_executor<F, Fut>(mut self, task_type: &str, handler: F) -> Self
    where
        F: Fn(sqlx::PgPool, Uuid, serde_json::Value) -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = ()> + Send + 'static,
    {
        let handler = Arc::new(handler);
        let wrapped: TaskExecutor = Arc::new(
            move |pool, task_id, config, timeout, cancellation| {
                let handler = Arc::clone(&handler);
                let pool_clone = pool.clone();
                tokio::spawn(async move {
                    tokio::select! {
                        _ = cancellation.cancelled() => TaskRunOutcome::Cancelled,
                        result = tokio::time::timeout(timeout, handler(pool_clone, task_id, config)) => {
                            match result {
                                Ok(()) => TaskRunOutcome::Success,
                                Err(_) => {
                                    tracing::warn!(task_id = %task_id, "Task timed out");
                                    TaskRunOutcome::Timeout
                                }
                            }
                        }
                    }
                })
            },
        );
        self.executors.push((task_type.to_string(), wrapped));
        self
    }

    pub fn register_fallible_executor<F, Fut, E>(mut self, task_type: &str, handler: F) -> Self
    where
        F: Fn(sqlx::PgPool, Uuid, serde_json::Value) -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = Result<(), E>> + Send + 'static,
        E: std::fmt::Display + Send + 'static,
    {
        let handler = Arc::new(handler);
        let wrapped: TaskExecutor = Arc::new(
            move |pool, task_id, config, timeout, cancellation| {
                let handler = Arc::clone(&handler);
                let pool_clone = pool.clone();
                tokio::spawn(async move {
                    tokio::select! {
                        _ = cancellation.cancelled() => TaskRunOutcome::Cancelled,
                        result = tokio::time::timeout(timeout, handler(pool_clone, task_id, config)) => {
                            match result {
                                Ok(Ok(())) => TaskRunOutcome::Success,
                                Ok(Err(err)) => TaskRunOutcome::Failure(err.to_string()),
                                Err(_) => {
                                    tracing::warn!(task_id = %task_id, "Task timed out");
                                    TaskRunOutcome::Timeout
                                }
                            }
                        }
                    }
                })
            },
        );
        self.executors.push((task_type.to_string(), wrapped));
        self
    }

    pub async fn start(self: &Arc<Self>, tracker: &TaskTracker, shutdown: CancellationToken) {
        tracing::info!("Scheduled task runner starting");

        let me = Arc::clone(self);
        tracker.spawn(async move {
            let mut interval = tokio::time::interval(me.tick_interval);
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        if let Err(e) = me.tick().await {
                            tracing::error!(error = %e, "Scheduler tick failed");
                        }
                    }
                    _ = shutdown.cancelled() => {
                        tracing::info!("Scheduled task runner shutting down");
                        break;
                    }
                }
            }
        });
    }

    async fn tick(&self) -> Result<(), SchedulerError> {
        let due_tasks = self.fetch_due_tasks().await?;

        for task in due_tasks {
            if let Err(e) = self.execute_task(&task, "scheduled").await {
                tracing::warn!(
                    task_id = %task.id,
                    task_type = %task.task_type,
                    error = %e,
                    "Failed to execute scheduled task"
                );
            }
        }

        Ok(())
    }

    async fn fetch_due_tasks(&self) -> Result<Vec<ScheduledTaskRow>, SchedulerError> {
        let rows = sqlx::query(
            r#"
            SELECT
                id, name, task_type, cron_expression, interval_seconds,
                is_enabled, timeout_seconds, max_retries, retry_delay_seconds,
                state, consecutive_failures,
                last_run_at, last_run_duration_ms, last_run_result, last_error,
                next_run_at, config, metadata
            FROM scheduled_tasks
            WHERE is_enabled = true
              AND state != 'running'
              AND next_run_at <= now()
            ORDER BY next_run_at ASC
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        let tasks: Vec<ScheduledTaskRow> = rows
            .iter()
            .map(|row| ScheduledTaskRow {
                id: row.get("id"),
                name: row.get("name"),
                task_type: row.get("task_type"),
                cron_expression: row.get("cron_expression"),
                interval_seconds: row.get("interval_seconds"),
                is_enabled: row.get("is_enabled"),
                timeout_seconds: row.get("timeout_seconds"),
                max_retries: row.get("max_retries"),
                retry_delay_seconds: row.get("retry_delay_seconds"),
                state: row.get("state"),
                consecutive_failures: row.get("consecutive_failures"),
                last_run_at: row.get("last_run_at"),
                last_run_duration_ms: row.get("last_run_duration_ms"),
                last_run_result: row.get("last_run_result"),
                last_error: row.get("last_error"),
                next_run_at: row.get("next_run_at"),
                config: row.get("config"),
                metadata: row.get("metadata"),
            })
            .collect();

        Ok(tasks)
    }

    async fn execute_task(
        &self,
        task: &ScheduledTaskRow,
        trigger_type: &str,
    ) -> Result<Uuid, SchedulerError> {
        let executor = self
            .executors
            .iter()
            .find(|(t, _)| t == &task.task_type)
            .map(|(_, e)| Arc::clone(e));

        let executor = match executor {
            Some(e) => e,
            None => {
                tracing::warn!(
                    task_type = %task.task_type,
                    "No executor registered for task type, skipping"
                );
                return Err(SchedulerError::ExecutorNotRegistered(
                    task.task_type.clone(),
                ));
            }
        };

        self.claim_task(task.id).await?;
        let run_id = match self.create_run(task.id, trigger_type).await {
            Ok(run_id) => run_id,
            Err(err) => {
                let _ = self.reset_claim_after_start_failure(task.id).await;
                return Err(err);
            }
        };

        let pool = self.pool.clone();
        let task_id = task.id;
        let task_name = task.name.clone();
        let task_type = task.task_type.clone();
        let config = task.config.clone();
        let timeout = Duration::from_secs(task.timeout_seconds.max(1) as u64);
        let max_retries = task.max_retries;
        let retry_delay = task.retry_delay_seconds as u64;
        let consecutive_failures = task.consecutive_failures;
        let active_tasks = self.active_tasks.clone();
        let cancellation = CancellationToken::new();
        active_tasks.insert(task_id, cancellation.clone());

        let executor2 = Arc::clone(&executor);
        tokio::spawn(async move {
            tracing::info!(
                task_id = %task_id,
                task_type = %task_type,
                run_id = %run_id,
                "Executing scheduled task"
            );

            let pool_clone = pool.clone();
            let handle = (executor2)(pool_clone, task_id, config, timeout, cancellation);

            match handle.await {
                Ok(TaskRunOutcome::Success) => {
                    on_task_success(&pool, task_id, run_id, &task_name).await;
                }
                Ok(TaskRunOutcome::Failure(error_message)) => {
                    on_task_failure(
                        &pool,
                        task_id,
                        run_id,
                        "failure",
                        &TaskFailureInfo {
                            task_name,
                            error_message,
                            max_retries,
                            retry_delay_secs: retry_delay,
                            consecutive_failures,
                        },
                    )
                    .await;
                }
                Ok(TaskRunOutcome::Cancelled) => {
                    on_task_cancelled(&pool, task_id, run_id, &task_name).await;
                }
                Ok(TaskRunOutcome::Timeout) => {
                    on_task_failure(
                        &pool,
                        task_id,
                        run_id,
                        "timeout",
                        &TaskFailureInfo {
                            task_name,
                            error_message: format!(
                                "Task timed out after {} seconds",
                                timeout.as_secs()
                            ),
                            max_retries,
                            retry_delay_secs: retry_delay,
                            consecutive_failures,
                        },
                    )
                    .await;
                }
                Err(e) => {
                    let err_msg = format!("Task panicked: {e}");
                    on_task_failure(
                        &pool,
                        task_id,
                        run_id,
                        "failure",
                        &TaskFailureInfo {
                            task_name,
                            error_message: err_msg,
                            max_retries,
                            retry_delay_secs: retry_delay,
                            consecutive_failures,
                        },
                    )
                    .await;
                }
            }
            active_tasks.remove(&task_id);
        });

        Ok(run_id)
    }

    pub async fn trigger_task(&self, task_id: Uuid) -> Result<Uuid, SchedulerError> {
        let task = self.get_task(task_id).await?;

        if task.state == "running" {
            return Err(SchedulerError::AlreadyRunning(task_id));
        }

        self.execute_task(&task, "manual").await
    }

    pub async fn cancel_task(&self, task_id: Uuid) -> Result<(), SchedulerError> {
        let task = self.get_task(task_id).await?;

        if task.state != "running" {
            return Ok(());
        }

        if let Some((_, cancellation)) = self.active_tasks.remove(&task_id) {
            cancellation.cancel();
        }

        sqlx::query(
            r#"
            UPDATE scheduled_task_runs
            SET state = 'cancelled',
                completed_at = now(),
                duration_ms = EXTRACT(EPOCH FROM (now() - started_at))::INT * 1000,
                result = 'cancelled',
                error_message = 'Cancelled by administrator'
            WHERE scheduled_task_id = $1 AND state = 'running'
            "#,
        )
        .bind(task_id)
        .execute(&self.pool)
        .await?;

        sqlx::query(
            r#"
            UPDATE scheduled_tasks
            SET state = 'idle',
                last_run_at = now(),
                last_run_duration_ms = (
                    SELECT duration_ms FROM scheduled_task_runs
                    WHERE scheduled_task_id = $1
                    ORDER BY started_at DESC LIMIT 1
                ),
                last_run_result = 'cancelled',
                last_error = 'Cancelled by administrator',
                updated_at = now()
            WHERE id = $1
            "#,
        )
        .bind(task_id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn list_tasks(&self) -> Result<Vec<ScheduledTaskRow>, SchedulerError> {
        let rows = sqlx::query(
            r#"
            SELECT
                id, name, task_type, cron_expression, interval_seconds,
                is_enabled, timeout_seconds, max_retries, retry_delay_seconds,
                state, consecutive_failures,
                last_run_at, last_run_duration_ms, last_run_result, last_error,
                next_run_at, config, metadata
            FROM scheduled_tasks
            ORDER BY name ASC
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        let tasks: Vec<ScheduledTaskRow> = rows
            .iter()
            .map(|row| ScheduledTaskRow {
                id: row.get("id"),
                name: row.get("name"),
                task_type: row.get("task_type"),
                cron_expression: row.get("cron_expression"),
                interval_seconds: row.get("interval_seconds"),
                is_enabled: row.get("is_enabled"),
                timeout_seconds: row.get("timeout_seconds"),
                max_retries: row.get("max_retries"),
                retry_delay_seconds: row.get("retry_delay_seconds"),
                state: row.get("state"),
                consecutive_failures: row.get("consecutive_failures"),
                last_run_at: row.get("last_run_at"),
                last_run_duration_ms: row.get("last_run_duration_ms"),
                last_run_result: row.get("last_run_result"),
                last_error: row.get("last_error"),
                next_run_at: row.get("next_run_at"),
                config: row.get("config"),
                metadata: row.get("metadata"),
            })
            .collect();

        Ok(tasks)
    }

    pub async fn get_task(&self, task_id: Uuid) -> Result<ScheduledTaskRow, SchedulerError> {
        let row = sqlx::query(
            r#"
            SELECT
                id, name, task_type, cron_expression, interval_seconds,
                is_enabled, timeout_seconds, max_retries, retry_delay_seconds,
                state, consecutive_failures,
                last_run_at, last_run_duration_ms, last_run_result, last_error,
                next_run_at, config, metadata
            FROM scheduled_tasks
            WHERE id = $1
            "#,
        )
        .bind(task_id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or(SchedulerError::TaskNotFound(task_id))?;

        Ok(ScheduledTaskRow {
            id: row.get("id"),
            name: row.get("name"),
            task_type: row.get("task_type"),
            cron_expression: row.get("cron_expression"),
            interval_seconds: row.get("interval_seconds"),
            is_enabled: row.get("is_enabled"),
            timeout_seconds: row.get("timeout_seconds"),
            max_retries: row.get("max_retries"),
            retry_delay_seconds: row.get("retry_delay_seconds"),
            state: row.get("state"),
            consecutive_failures: row.get("consecutive_failures"),
            last_run_at: row.get("last_run_at"),
            last_run_duration_ms: row.get("last_run_duration_ms"),
            last_run_result: row.get("last_run_result"),
            last_error: row.get("last_error"),
            next_run_at: row.get("next_run_at"),
            config: row.get("config"),
            metadata: row.get("metadata"),
        })
    }

    pub async fn list_task_runs(
        &self,
        task_id: Uuid,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<ScheduledTaskRunRow>, SchedulerError> {
        self.get_task(task_id).await?;

        let rows = sqlx::query(
            r#"
            SELECT
                id,
                scheduled_task_id,
                trigger_type,
                state,
                started_at,
                completed_at,
                duration_ms,
                result,
                error_message,
                error_details,
                stats
            FROM scheduled_task_runs
            WHERE scheduled_task_id = $1
            ORDER BY started_at DESC
            LIMIT $2 OFFSET $3
            "#,
        )
        .bind(task_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.iter().map(scheduled_task_run_from_row).collect())
    }

    async fn create_run(&self, task_id: Uuid, trigger_type: &str) -> Result<Uuid, SchedulerError> {
        let row = sqlx::query(
            r#"
            INSERT INTO scheduled_task_runs (scheduled_task_id, trigger_type, state, started_at)
            VALUES ($1, $2, 'running', now())
            RETURNING id
            "#,
        )
        .bind(task_id)
        .bind(trigger_type)
        .fetch_one(&self.pool)
        .await?;

        Ok(row.get("id"))
    }

    async fn claim_task(&self, task_id: Uuid) -> Result<(), SchedulerError> {
        let affected = sqlx::query(
            r#"
            UPDATE scheduled_tasks
            SET state = 'running', updated_at = now()
            WHERE id = $1 AND state != 'running'
            "#,
        )
        .bind(task_id)
        .execute(&self.pool)
        .await?
        .rows_affected();

        if affected == 0 {
            Err(SchedulerError::AlreadyRunning(task_id))
        } else {
            Ok(())
        }
    }

    async fn reset_claim_after_start_failure(&self, task_id: Uuid) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            UPDATE scheduled_tasks
            SET state = 'idle',
                last_error = 'Failed to create scheduled task run record',
                updated_at = now()
            WHERE id = $1 AND state = 'running'
            "#,
        )
        .bind(task_id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }
}

fn scheduled_task_run_from_row(row: &sqlx::postgres::PgRow) -> ScheduledTaskRunRow {
    ScheduledTaskRunRow {
        id: row.get("id"),
        scheduled_task_id: row.get("scheduled_task_id"),
        trigger_type: row.get("trigger_type"),
        state: row.get("state"),
        started_at: row.get("started_at"),
        completed_at: row.get("completed_at"),
        duration_ms: row.get("duration_ms"),
        result: row.get("result"),
        error_message: row.get("error_message"),
        error_details: row.get("error_details"),
        stats: row.get("stats"),
    }
}

async fn complete_run(
    pool: &sqlx::PgPool,
    run_id: Uuid,
    result: &str,
    error_message: Option<&str>,
    stats: Option<serde_json::Value>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        UPDATE scheduled_task_runs
        SET state = CASE
                WHEN $2 = 'success' THEN 'completed'
                WHEN $2 = 'cancelled' THEN 'cancelled'
                ELSE 'failed'
            END,
            completed_at = now(),
            duration_ms = EXTRACT(EPOCH FROM (now() - started_at))::INT * 1000,
            result = $2,
            error_message = $3,
            stats = COALESCE($4, stats)
        WHERE id = $1
          AND state = 'running'
        "#,
    )
    .bind(run_id)
    .bind(result)
    .bind(error_message)
    .bind(stats)
    .execute(pool)
    .await?;

    Ok(())
}

async fn on_task_success(pool: &sqlx::PgPool, task_id: Uuid, run_id: Uuid, task_name: &str) {
    let now = Utc::now();

    let task_row = sqlx::query(
        r#"
        SELECT cron_expression, interval_seconds FROM scheduled_tasks WHERE id = $1
        "#,
    )
    .bind(task_id)
    .fetch_optional(pool)
    .await;

    let next_run = match task_row {
        Ok(Some(row)) => {
            let cron_expr: Option<String> = row.get("cron_expression");
            let interval_secs: Option<i32> = row.get("interval_seconds");
            compute_next_run(cron_expr.as_deref(), interval_secs, &now)
        }
        _ => now + chrono::Duration::hours(1),
    };

    let result = sqlx::query(
        r#"
        UPDATE scheduled_tasks
        SET state = 'idle',
            consecutive_failures = 0,
            last_run_at = now(),
            last_run_duration_ms = EXTRACT(EPOCH FROM (now() - (
                SELECT started_at FROM scheduled_task_runs WHERE id = $3
            )))::INT * 1000,
            last_run_result = 'success',
            last_error = NULL,
            next_run_at = $2,
            updated_at = now()
        WHERE id = $1
          AND state = 'running'
        "#,
    )
    .bind(task_id)
    .bind(next_run)
    .bind(run_id)
    .execute(pool)
    .await;

    if let Err(e) = result {
        tracing::error!(task_id = %task_id, error = %e, "Failed to update task after success");
    } else {
        tracing::info!(task_id = %task_id, task_name = %task_name, "Scheduled task completed successfully");
    }

    let _ = complete_run(pool, run_id, "success", None, None).await;
}

struct TaskFailureInfo {
    task_name: String,
    error_message: String,
    max_retries: i32,
    retry_delay_secs: u64,
    consecutive_failures: i32,
}

async fn on_task_failure(
    pool: &sqlx::PgPool,
    task_id: Uuid,
    run_id: Uuid,
    result_kind: &str,
    info: &TaskFailureInfo,
) {
    let new_failures = info.consecutive_failures + 1;
    let should_disable = new_failures >= info.max_retries;

    let now = Utc::now();
    let next_run = if should_disable {
        None
    } else {
        Some(now + chrono::Duration::seconds(info.retry_delay_secs as i64))
    };

    let result = if should_disable {
        sqlx::query(
            r#"
            UPDATE scheduled_tasks
            SET state = 'idle',
                is_enabled = false,
                consecutive_failures = $2,
                last_run_at = now(),
                last_run_duration_ms = EXTRACT(EPOCH FROM (now() - (
                    SELECT started_at FROM scheduled_task_runs WHERE id = $4
                )))::INT * 1000,
                last_run_result = $5,
                last_error = $3,
                next_run_at = NULL,
                updated_at = now()
            WHERE id = $1
              AND state = 'running'
            "#,
        )
        .bind(task_id)
        .bind(new_failures)
        .bind(&info.error_message)
        .bind(run_id)
        .bind(result_kind)
        .execute(pool)
        .await
    } else {
        sqlx::query(
            r#"
            UPDATE scheduled_tasks
            SET state = 'idle',
                consecutive_failures = $2,
                last_run_at = now(),
                last_run_duration_ms = EXTRACT(EPOCH FROM (now() - (
                    SELECT started_at FROM scheduled_task_runs WHERE id = $4
                )))::INT * 1000,
                last_run_result = $6,
                last_error = $3,
                next_run_at = $5,
                updated_at = now()
            WHERE id = $1
              AND state = 'running'
            "#,
        )
        .bind(task_id)
        .bind(new_failures)
        .bind(&info.error_message)
        .bind(run_id)
        .bind(next_run)
        .bind(result_kind)
        .execute(pool)
        .await
    };

    if let Err(e) = result {
        tracing::error!(task_id = %task_id, error = %e, "Failed to update task after failure");
    } else if should_disable {
        tracing::error!(
            task_id = %task_id,
            task_name = %info.task_name,
            consecutive_failures = new_failures,
            "Scheduled task auto-disabled after exceeding max consecutive failures"
        );
    } else {
        tracing::warn!(
            task_id = %task_id,
            task_name = %info.task_name,
            consecutive_failures = new_failures,
            "Scheduled task failed, will retry"
        );
    }

    let _ = complete_run(pool, run_id, result_kind, Some(&info.error_message), None).await;
}

async fn on_task_cancelled(pool: &sqlx::PgPool, task_id: Uuid, run_id: Uuid, task_name: &str) {
    let result = sqlx::query(
        r#"
        UPDATE scheduled_tasks
        SET state = 'idle',
            last_run_at = now(),
            last_run_duration_ms = EXTRACT(EPOCH FROM (now() - (
                SELECT started_at FROM scheduled_task_runs WHERE id = $2
            )))::INT * 1000,
            last_run_result = 'cancelled',
            last_error = 'Cancelled by administrator',
            updated_at = now()
        WHERE id = $1
          AND state = 'running'
        "#,
    )
    .bind(task_id)
    .bind(run_id)
    .execute(pool)
    .await;

    if let Err(e) = result {
        tracing::error!(task_id = %task_id, error = %e, "Failed to update task after cancellation");
    } else {
        tracing::info!(task_id = %task_id, task_name = %task_name, "Scheduled task cancelled");
    }

    let _ = complete_run(
        pool,
        run_id,
        "cancelled",
        Some("Cancelled by administrator"),
        None,
    )
    .await;
}

fn compute_next_run(
    cron_expression: Option<&str>,
    interval_seconds: Option<i32>,
    from: &DateTime<Utc>,
) -> DateTime<Utc> {
    if let Some(expr) = cron_expression
        && let Ok(cron) = Cron::from_str(expr)
        && let Ok(next) = cron.find_next_occurrence(from, false)
    {
        return next;
    }

    if let Some(expr) = cron_expression {
        tracing::warn!(cron = %expr, "Failed to compute next cron fire time, defaulting to 1 hour");
    }

    if let Some(secs) = interval_seconds {
        return *from + chrono::Duration::seconds(secs as i64);
    }

    *from + chrono::Duration::hours(1)
}

pub async fn seed_default_tasks(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    let existing: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM scheduled_tasks")
        .fetch_one(pool)
        .await?;

    if existing > 0 {
        tracing::debug!("Scheduled tasks already seeded, skipping");
        return Ok(());
    }

    tracing::info!("Seeding default scheduled tasks");

    let defaults = [
        (
            "Library Scan",
            "library_scan",
            Some("0 3 * * *"),
            None::<i32>,
        ),
        (
            "Metadata Refresh",
            "metadata_refresh",
            Some("0 4 * * *"),
            None::<i32>,
        ),
        (
            "Database Maintenance",
            "database_maintenance",
            Some("0 5 * * 0"),
            None::<i32>,
        ),
        (
            "Database Backup",
            "backup_database",
            Some("0 3 * * *"),
            None::<i32>,
        ),
        (
            "Backup Verification",
            "backup_verification",
            Some("30 4 * * *"),
            None::<i32>,
        ),
        (
            "Backup Retention Cleanup",
            "backup_retention_cleanup",
            Some("0 5 * * 0"),
            None::<i32>,
        ),
        (
            "Reindex Maintenance",
            "reindex_maintenance",
            Some("0 2 * * 0"),
            None::<i32>,
        ),
        ("Session Cleanup", "session_cleanup", None, Some(3600)),
        (
            "Notification Cleanup",
            "notification_cleanup",
            Some("0 2 * * *"),
            None::<i32>,
        ),
        ("Disk Space Check", "disk_space_check", None, Some(1800)),
        (
            "Media Health Check",
            "media_health_check",
            Some("0 6 * * 0"),
            None::<i32>,
        ),
        (
            "Soft Delete Purge",
            "soft_delete_purge",
            Some("0 1 * * *"),
            None::<i32>,
        ),
        (
            "Subtitle Auto-Fetch",
            "subtitle_auto_fetch",
            None,
            Some(1800),
        ),
        (
            "Segment Analysis",
            "segment_analysis",
            Some("0 3 * * *"),
            None::<i32>,
        ),
        (
            "Storyboard Generation",
            "storyboard_generation",
            Some("0 4 * * *"),
            None::<i32>,
        ),
        ("Trakt Sync", "trakt_sync", None, Some(1800)),
        (
            "GeoIP Database Update",
            "geoip_database_update",
            Some("0 3 * * 1"),
            None::<i32>,
        ),
        (
            "Collection Sync",
            "collection_sync",
            Some("0 6 * * *"),
            None::<i32>,
        ),
        (
            "Overlay Application",
            "overlay_application",
            Some("0 5 * * *"),
            None::<i32>,
        ),
        (
            "Asset Directory Scan",
            "asset_directory_scan",
            Some("0 3 * * *"),
            None::<i32>,
        ),
    ];

    for (name, task_type, cron_expr, interval_secs) in &defaults {
        let now = Utc::now();
        let next_run = compute_next_run(*cron_expr, *interval_secs, &now);

        sqlx::query(
            r#"
            INSERT INTO scheduled_tasks (name, task_type, cron_expression, interval_seconds, next_run_at, config)
            VALUES ($1, $2, $3, $4, $5, '{}')
            ON CONFLICT (name) DO NOTHING
            "#,
        )
        .bind(name)
        .bind(task_type)
        .bind(*cron_expr)
        .bind(*interval_secs)
        .bind(next_run)
        .execute(pool)
        .await?;
    }

    tracing::info!("Default scheduled tasks seeded");
    Ok(())
}

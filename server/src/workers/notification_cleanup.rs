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

use sqlx::PgPool;
use uuid::Uuid;

pub async fn run_notification_cleanup(pool: &PgPool, task_id: Uuid, config: serde_json::Value) {
    let max_age_days = config
        .get("max_age_days")
        .and_then(|v| v.as_i64())
        .unwrap_or(90)
        .clamp(1, 3650);

    let result = sqlx::query(
        r#"
        DELETE FROM notifications
        WHERE (expires_at IS NOT NULL AND expires_at < now())
           OR created_at < now() - ($1::INT * INTERVAL '1 day')
        "#,
    )
    .bind(max_age_days as i32)
    .execute(pool)
    .await;

    match result {
        Ok(done) => {
            tracing::info!(
                task_id = %task_id,
                deleted = done.rows_affected(),
                max_age_days,
                "Notification cleanup completed"
            );
        }
        Err(e) => {
            tracing::error!(
                task_id = %task_id,
                max_age_days,
                error = %e,
                "Notification cleanup failed"
            );
        }
    }
}

-- Phase 13a Task 9 — add `backup_recovery_drill` to the task_type CHECK
-- constraint and seed the recovery-drill scheduled task for existing
-- deployments. Fresh installs receive the same row via
-- `seed_default_tasks` in `services/scheduler.rs`.

-- The original column-level CHECK constraint is named
-- `scheduled_tasks_task_type_check` (PostgreSQL default for column-level
-- constraints). We rebuild it with the new value appended.
ALTER TABLE scheduled_tasks
    DROP CONSTRAINT IF EXISTS scheduled_tasks_task_type_check;

ALTER TABLE scheduled_tasks
    ADD CONSTRAINT scheduled_tasks_task_type_check CHECK (task_type IN (
        'library_scan', 'metadata_refresh', 'database_maintenance',
        'partition_management', 'session_cleanup', 'trakt_sync',
        'backup_database', 'backup_verification', 'database_integrity_check',
        'backup_retention_cleanup', 'media_health_check', 'notification_cleanup',
        'trust_recalculation', 'soft_delete_purge', 'segment_analysis',
        'storyboard_generation', 'disk_space_check', 'reindex_maintenance',
        'analyze_parents', 'transcode_health_check', 'subtitle_ocr',
        'subtitle_voice_analysis', 'subtitle_auto_fetch',
        'overlay_application', 'overlay_cleanup',
        'collection_sync', 'collection_cleanup',
        'artwork_refresh', 'asset_directory_scan',
        'migration_cleanup', 'system_requirement_check',
        'geoip_database_update',
        'backup_recovery_drill'
    ));

INSERT INTO scheduled_tasks (
    id,
    name,
    task_type,
    cron_expression,
    is_enabled,
    timeout_seconds,
    max_retries,
    retry_delay_seconds,
    next_run_at,
    config
)
SELECT
    uuidv7(),
    'Backup Recovery Drill',
    'backup_recovery_drill',
    '0 7 * * 0',
    true,
    3600,
    3,
    900,
    -- Next Sunday 07:00 UTC. Generates 14 days of 07:00 candidates starting
    -- today, keeps only Sundays strictly later than now, picks the earliest.
    -- This is more precise than the `now() + INTERVAL '1 day'` placeholder
    -- used by other weekly seed migrations because a one-off drill run on
    -- the wrong weekday would surprise operators.
    COALESCE(
        (
            SELECT min(d)
            FROM generate_series(
                date_trunc('day', now()) + INTERVAL '7 hours',
                date_trunc('day', now()) + INTERVAL '14 days 7 hours',
                INTERVAL '1 day'
            ) AS d
            WHERE extract(dow from d) = 0 AND d > now()
        ),
        now() + INTERVAL '7 days'
    ),
    '{}'::JSONB
WHERE NOT EXISTS (SELECT 1 FROM scheduled_tasks WHERE task_type = 'backup_recovery_drill')
ON CONFLICT (name) DO NOTHING;

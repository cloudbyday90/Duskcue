-- Phase 16c Task 6 — add the offline download package worker task type and
-- seed a durable scheduled worker for existing deployments. Fresh installs
-- also receive this row via `seed_default_tasks`.

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
        'backup_recovery_drill',
        'download_package_worker'
    ));

INSERT INTO scheduled_tasks (
    id,
    name,
    task_type,
    interval_seconds,
    is_enabled,
    timeout_seconds,
    max_retries,
    retry_delay_seconds,
    next_run_at,
    config
)
SELECT
    uuidv7(),
    'Download Package Worker',
    'download_package_worker',
    60,
    true,
    7200,
    3,
    300,
    now() + INTERVAL '1 minute',
    jsonb_build_object(
        'max_jobs_per_run', 1,
        'max_retries', 2,
        'stale_preparing_minutes', 120,
        'failed_cleanup_hours', 24
    )
WHERE NOT EXISTS (SELECT 1 FROM scheduled_tasks WHERE task_type = 'download_package_worker')
ON CONFLICT (name) DO NOTHING;

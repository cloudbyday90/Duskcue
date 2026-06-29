ALTER TABLE migration_sources
    DROP CONSTRAINT IF EXISTS migration_sources_status_check;

ALTER TABLE migration_sources
    ADD CONSTRAINT migration_sources_status_check CHECK (status IN (
        'pending',
        'discovering',
        'matching',
        'importing',
        'completed',
        'failed',
        'cancelled'
    ));

CREATE INDEX IF NOT EXISTS idx_migration_sources_status
    ON migration_sources (status);

CREATE INDEX IF NOT EXISTS idx_migration_user_mapping_source
    ON migration_user_mapping (migration_source_id);

CREATE INDEX IF NOT EXISTS idx_migration_import_log_source
    ON migration_import_log (migration_source_id);

CREATE INDEX IF NOT EXISTS idx_migration_import_log_status
    ON migration_import_log (status);

CREATE INDEX IF NOT EXISTS idx_migration_import_log_matched_media
    ON migration_import_log (matched_media_item_id)
    WHERE matched_media_item_id IS NOT NULL;

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
    'Migration Cleanup',
    'migration_cleanup',
    '0 5 * * *',
    false,
    1800,
    3,
    900,
    CASE
        WHEN now() < date_trunc('day', now()) + INTERVAL '5 hours'
            THEN date_trunc('day', now()) + INTERVAL '5 hours'
        ELSE date_trunc('day', now()) + INTERVAL '1 day 5 hours'
    END,
    '{"delete_plex_uploads_after_hours":24,"delete_completed_sources_after_days":90,"delete_import_logs_after_days":90}'::JSONB
WHERE NOT EXISTS (SELECT 1 FROM scheduled_tasks WHERE task_type = 'migration_cleanup')
ON CONFLICT (name) DO NOTHING;

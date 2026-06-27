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
    'Database Backup',
    'backup_database',
    '0 3 * * *',
    true,
    7200,
    3,
    900,
    CASE
        WHEN now() < date_trunc('day', now()) + INTERVAL '3 hours'
            THEN date_trunc('day', now()) + INTERVAL '3 hours'
        ELSE date_trunc('day', now()) + INTERVAL '1 day 3 hours'
    END,
    '{}'::JSONB
WHERE NOT EXISTS (SELECT 1 FROM scheduled_tasks WHERE task_type = 'backup_database')
ON CONFLICT (name) DO NOTHING;

UPDATE scheduled_tasks
SET cron_expression = '0 3 * * *',
    interval_seconds = NULL,
    is_enabled = true,
    timeout_seconds = GREATEST(timeout_seconds, 7200),
    max_retries = GREATEST(max_retries, 3),
    retry_delay_seconds = GREATEST(retry_delay_seconds, 900),
    next_run_at = COALESCE(
        next_run_at,
        CASE
            WHEN now() < date_trunc('day', now()) + INTERVAL '3 hours'
                THEN date_trunc('day', now()) + INTERVAL '3 hours'
            ELSE date_trunc('day', now()) + INTERVAL '1 day 3 hours'
        END
    ),
    updated_at = now()
WHERE task_type = 'backup_database';

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
    'Backup Verification',
    'backup_verification',
    '30 4 * * *',
    true,
    1800,
    3,
    900,
    CASE
        WHEN now() < date_trunc('day', now()) + INTERVAL '4 hours 30 minutes'
            THEN date_trunc('day', now()) + INTERVAL '4 hours 30 minutes'
        ELSE date_trunc('day', now()) + INTERVAL '1 day 4 hours 30 minutes'
    END,
    '{}'::JSONB
WHERE NOT EXISTS (SELECT 1 FROM scheduled_tasks WHERE task_type = 'backup_verification')
ON CONFLICT (name) DO NOTHING;

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
    'Backup Retention Cleanup',
    'backup_retention_cleanup',
    '0 5 * * 0',
    true,
    1800,
    3,
    900,
    now() + INTERVAL '1 day',
    '{}'::JSONB
WHERE NOT EXISTS (SELECT 1 FROM scheduled_tasks WHERE task_type = 'backup_retention_cleanup')
ON CONFLICT (name) DO NOTHING;

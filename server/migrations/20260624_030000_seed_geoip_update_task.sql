INSERT INTO scheduled_tasks (id, name, task_type, cron_expression, is_enabled, timeout_seconds, max_retries, retry_delay_seconds, config)
SELECT
    uuidv7(),
    'GeoIP Database Update',
    'geoip_database_update',
    '0 3 * * 1',
    true,
    600,
    3,
    3600,
    '{}'
WHERE NOT EXISTS (SELECT 1 FROM scheduled_tasks WHERE task_type = 'geoip_database_update');

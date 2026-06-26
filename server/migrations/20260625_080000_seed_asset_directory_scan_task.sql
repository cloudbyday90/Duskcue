INSERT INTO scheduled_tasks (id, name, task_type, cron_expression, is_enabled, timeout_seconds, max_retries, retry_delay_seconds, config)
SELECT uuidv7(), 'Asset Directory Scan', 'asset_directory_scan', '0 3 * * *', true, 1800, 3, 300, '{"path":null,"lock_imported":true}'::JSONB
WHERE NOT EXISTS (SELECT 1 FROM scheduled_tasks WHERE task_type = 'asset_directory_scan');

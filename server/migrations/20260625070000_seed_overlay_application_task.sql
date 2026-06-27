INSERT INTO scheduled_tasks (id, name, task_type, cron_expression, is_enabled, timeout_seconds, max_retries, retry_delay_seconds, config)
SELECT uuidv7(), 'Overlay Application', 'overlay_application', '0 5 * * *', true, 7200, 3, 300, '{"reapply_all":false,"max_concurrent":2}'::JSONB
WHERE NOT EXISTS (SELECT 1 FROM scheduled_tasks WHERE task_type = 'overlay_application');

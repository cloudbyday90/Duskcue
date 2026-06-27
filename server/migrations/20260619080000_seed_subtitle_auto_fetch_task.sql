INSERT INTO scheduled_tasks (id, name, task_type, interval_seconds, is_enabled, timeout_seconds, max_retries, retry_delay_seconds, config)
SELECT uuidv7(), 'Subtitle Auto-Fetch', 'subtitle_auto_fetch', 1800, false, 1800, 3, 300, '{}'::JSONB
WHERE NOT EXISTS (SELECT 1 FROM scheduled_tasks WHERE task_type = 'subtitle_auto_fetch');

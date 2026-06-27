INSERT INTO scheduled_tasks (id, name, task_type, cron_expression, is_enabled, timeout_seconds, max_retries, retry_delay_seconds, config)
SELECT uuidv7(), 'Storyboard Generation', 'storyboard_generation', '0 4 * * *', true, 14400, 3, 300, '{"max_concurrent_analyses":1,"interval_mode":"adaptive"}'::JSONB
WHERE NOT EXISTS (SELECT 1 FROM scheduled_tasks WHERE task_type = 'storyboard_generation');

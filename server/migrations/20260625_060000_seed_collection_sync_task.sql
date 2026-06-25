INSERT INTO scheduled_tasks (id, name, task_type, cron_expression, is_enabled, timeout_seconds, max_retries, retry_delay_seconds, config)
SELECT uuidv7(), 'Collection Sync', 'collection_sync', '0 6 * * *', true, 7200, 3, 300, '{"sync_dynamic":true,"sync_external":true,"max_external_requests_per_minute":30}'::JSONB
WHERE NOT EXISTS (SELECT 1 FROM scheduled_tasks WHERE task_type = 'collection_sync');

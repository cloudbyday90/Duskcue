UPDATE scheduled_tasks
SET is_enabled = true,
    config = config || '{"delete_failed_temp_files_after_hours":24}'::JSONB,
    updated_at = now()
WHERE task_type = 'migration_cleanup';

INSERT INTO notification_types (id, name, category, priority, in_app_template)
VALUES
    (uuidv7(), 'migration_completed', 'task', 'medium', 'migration-completed'),
    (uuidv7(), 'migration_failed', 'task', 'high', 'migration-failed')
ON CONFLICT (name) DO UPDATE
SET category = EXCLUDED.category,
    priority = EXCLUDED.priority,
    in_app_template = EXCLUDED.in_app_template,
    is_enabled_by_default = true,
    updated_at = now();

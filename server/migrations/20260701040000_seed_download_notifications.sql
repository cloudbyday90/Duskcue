INSERT INTO notification_types (id, name, category, priority, in_app_template)
VALUES
    (uuidv7(), 'download_ready', 'media', 'medium', 'download-ready'),
    (uuidv7(), 'download_failed', 'task', 'high', 'download-failed')
ON CONFLICT (name) DO UPDATE
SET category = EXCLUDED.category,
    priority = EXCLUDED.priority,
    in_app_template = EXCLUDED.in_app_template,
    is_enabled_by_default = true,
    updated_at = now();

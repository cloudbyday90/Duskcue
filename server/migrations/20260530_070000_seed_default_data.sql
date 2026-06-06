INSERT INTO server_config (id, server_name, http_port)
SELECT uuidv7(), 'My Duskcue', 48027
WHERE NOT EXISTS (SELECT 1 FROM server_config);

INSERT INTO streaming_policies (id, name, description, max_streams, max_transcode_streams, allow_direct_play, allow_direct_stream, allow_transcode, allow_transcode_4k, is_default, is_system)
SELECT uuidv7(), 'Admin', 'Server admins — no restrictions', NULL, NULL, true, true, true, true, true, true
WHERE NOT EXISTS (SELECT 1 FROM streaming_policies WHERE name = 'Admin');

INSERT INTO streaming_policies (id, name, description, max_streams, max_transcode_streams, allow_direct_play, allow_direct_stream, allow_transcode, allow_transcode_4k, is_default, is_system)
SELECT uuidv7(), 'Family', 'Trusted family members', 3, 2, true, true, true, true, false, true
WHERE NOT EXISTS (SELECT 1 FROM streaming_policies WHERE name = 'Family');

INSERT INTO streaming_policies (id, name, description, max_streams, max_transcode_streams, allow_direct_play, allow_direct_stream, allow_transcode, allow_transcode_4k, auto_terminate_paused_minutes, is_default, is_system)
SELECT uuidv7(), 'Guest', 'Temporary/guest access', 1, NULL, true, true, false, false, 30, false, true
WHERE NOT EXISTS (SELECT 1 FROM streaming_policies WHERE name = 'Guest');

INSERT INTO streaming_policies (id, name, description, allow_direct_play, allow_direct_stream, allow_transcode, allow_transcode_4k, blocked_ip_ranges, is_default, is_system)
SELECT uuidv7(), 'Remote Only', 'Restrict to remote/WAN streaming', true, true, true, true, '["192.168.0.0/16", "10.0.0.0/8", "172.16.0.0/12"]'::JSONB, false, true
WHERE NOT EXISTS (SELECT 1 FROM streaming_policies WHERE name = 'Remote Only');

INSERT INTO streaming_policies (id, name, description, allow_direct_play, allow_direct_stream, allow_transcode, allow_transcode_4k, allowed_ip_ranges, is_default, is_system)
SELECT uuidv7(), 'LAN Only', 'Restrict to local network', true, true, true, true, '["192.168.0.0/16", "10.0.0.0/8", "172.16.0.0/12"]'::JSONB, false, true
WHERE NOT EXISTS (SELECT 1 FROM streaming_policies WHERE name = 'LAN Only');

INSERT INTO notification_types (id, name, category, priority, in_app_template)
VALUES
    (uuidv7(), 'new_media_added', 'media', 'low', '{{title}} was added to {{library}}'),
    (uuidv7(), 'library_scan_complete', 'media', 'low', 'Library scan completed: {{stats}}'),
    (uuidv7(), 'playback_started', 'media', 'low', '{{username}} started watching {{title}}'),
    (uuidv7(), 'classifarr_decision', 'media', 'low', 'Classifarr routed {{title}} to {{library}}'),
    (uuidv7(), 'server_alert', 'system', 'high', '{{message}}'),
    (uuidv7(), 'server_update', 'system', 'low', 'Duskcue {{version}} is available'),
    (uuidv7(), 'task_failed', 'system', 'high', 'Task {{task_name}} failed: {{error}}'),
    (uuidv7(), 'trust_alert', 'security', 'high', 'Suspicious activity detected for {{username}}: {{details}}'),
    (uuidv7(), 'new_login', 'security', 'medium', '{{username}} logged in from {{ip}} on {{device}}'),
    (uuidv7(), 'user_invited', 'user', 'low', 'Invitation {{action}} for {{email}}'),
    (uuidv7(), 'trakt_sync_error', 'user', 'medium', 'Trakt sync failed for {{username}}: {{error}}')
ON CONFLICT (name) DO NOTHING;

INSERT INTO scheduled_tasks (id, name, task_type, cron_expression, is_enabled, timeout_seconds, config)
VALUES
    (uuidv7(), 'Library Scan (Full)', 'library_scan', '0 3 * * *', true, 14400, '{"mode":"full"}'::JSONB),
    (uuidv7(), 'Library Scan (Quick)', 'library_scan', NULL, true, 1800, '{"mode":"quick"}'::JSONB),
    (uuidv7(), 'Metadata Refresh', 'metadata_refresh', NULL, true, 7200, '{}'::JSONB),
    (uuidv7(), 'Database Maintenance', 'database_maintenance', '0 4 * * 0', true, 3600, '{"operations":["vacuum","analyze","reindex"]}'::JSONB),
    (uuidv7(), 'Partition Management', 'partition_management', '0 0 1 * *', true, 600, '{"create_ahead_months":2}'::JSONB),
    (uuidv7(), 'Session Cleanup', 'session_cleanup', NULL, true, 300, '{}'::JSONB),
    (uuidv7(), 'Trakt Sync', 'trakt_sync', NULL, true, 1800, '{}'::JSONB),
    (uuidv7(), 'Database Backup', 'backup_database', '0 4 * * *', true, 7200, '{}'::JSONB),
    (uuidv7(), 'Media Health Check', 'media_health_check', '0 2 * * 0', true, 14400, '{}'::JSONB),
    (uuidv7(), 'Notification Cleanup', 'notification_cleanup', NULL, true, 300, '{"max_age_days":90}'::JSONB),
    (uuidv7(), 'Trust Score Recalculation', 'trust_recalculation', NULL, true, 300, '{}'::JSONB),
    (uuidv7(), 'Segment Analysis', 'segment_analysis', '0 3 * * *', true, 14400, '{"max_concurrent_analyses":1}'::JSONB),
    (uuidv7(), 'Storyboard Generation', 'storyboard_generation', '0 4 * * *', true, 14400, '{"max_concurrent_analyses":1,"interval_mode":"adaptive"}'::JSONB),
    (uuidv7(), 'Disk Space Check', 'disk_space_check', NULL, true, 60, '{"check_paths":true}'::JSONB),
    (uuidv7(), 'Reindex Maintenance', 'reindex_maintenance', '0 2 * * 0', true, 7200, '{"bloat_threshold_percent":30,"min_index_size_mb":10}'::JSONB),
    (uuidv7(), 'Analyze Parents', 'analyze_parents', '0 3 * * *', true, 300, '{}'::JSONB),
    (uuidv7(), 'Transcode Health Check', 'transcode_health_check', NULL, true, 30, '{"stale_session_timeout_secs":600}'::JSONB),
    (uuidv7(), 'System Requirement Check', 'system_requirement_check', NULL, true, 30, '{"check_os":true,"check_docker":true}'::JSONB)
ON CONFLICT (name) DO NOTHING;

UPDATE scheduled_tasks SET interval_seconds = 900 WHERE name = 'Library Scan (Quick)' AND interval_seconds IS NULL AND cron_expression IS NULL;
UPDATE scheduled_tasks SET interval_seconds = 21600 WHERE name = 'Metadata Refresh' AND interval_seconds IS NULL AND cron_expression IS NULL;
UPDATE scheduled_tasks SET interval_seconds = 3600 WHERE name = 'Session Cleanup' AND interval_seconds IS NULL AND cron_expression IS NULL;
UPDATE scheduled_tasks SET interval_seconds = 1800 WHERE name = 'Trakt Sync' AND interval_seconds IS NULL AND cron_expression IS NULL;
UPDATE scheduled_tasks SET interval_seconds = 86400 WHERE name = 'Notification Cleanup' AND interval_seconds IS NULL AND cron_expression IS NULL;
UPDATE scheduled_tasks SET interval_seconds = 3600 WHERE name = 'Trust Score Recalculation' AND interval_seconds IS NULL AND cron_expression IS NULL;
UPDATE scheduled_tasks SET interval_seconds = 1800 WHERE name = 'Disk Space Check' AND interval_seconds IS NULL AND cron_expression IS NULL;
UPDATE scheduled_tasks SET interval_seconds = 3600 WHERE name = 'Trust Score Recalculation' AND interval_seconds IS NULL;
UPDATE scheduled_tasks SET interval_seconds = 60 WHERE name = 'Transcode Health Check' AND interval_seconds IS NULL AND cron_expression IS NULL;
UPDATE scheduled_tasks SET interval_seconds = 86400 WHERE name = 'System Requirement Check' AND interval_seconds IS NULL AND cron_expression IS NULL;

CREATE TABLE IF NOT EXISTS server_config (
    id UUID DEFAULT uuidv7() PRIMARY KEY,
    created_at TIMESTAMPTZ GENERATED ALWAYS AS (uuid_extract_timestamp(id)) STORED,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),

    server_name TEXT NOT NULL DEFAULT 'My Duskcue',
    base_url TEXT,
    http_port INT NOT NULL DEFAULT 48027,
    https_port INT,
    ssl_certificate_path TEXT,
    ssl_private_key_path TEXT,

    network JSONB NOT NULL DEFAULT '{}',
    transcoding JSONB NOT NULL DEFAULT '{}',
    metadata JSONB NOT NULL DEFAULT '{}',
    auth JSONB NOT NULL DEFAULT '{}',
    security JSONB NOT NULL DEFAULT '{}',
    notifications JSONB NOT NULL DEFAULT '{}',
    backup JSONB NOT NULL DEFAULT '{}',
    integrations JSONB NOT NULL DEFAULT '{}',
    logging JSONB NOT NULL DEFAULT '{}',
    storage JSONB NOT NULL DEFAULT '{}',
    maintenance JSONB NOT NULL DEFAULT '{}',
    resource_limits JSONB NOT NULL DEFAULT '{}',
    cpu JSONB NOT NULL DEFAULT '{}',
    quality JSONB NOT NULL DEFAULT '{}',
    subtitles JSONB NOT NULL DEFAULT '{}',
    analytics JSONB NOT NULL DEFAULT '{}',

    schema_version INT NOT NULL DEFAULT 2
);

CREATE TABLE IF NOT EXISTS scheduled_tasks (
    id UUID DEFAULT uuidv7() PRIMARY KEY,
    created_at TIMESTAMPTZ GENERATED ALWAYS AS (uuid_extract_timestamp(id)) STORED,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),

    name TEXT NOT NULL UNIQUE,
    task_type TEXT NOT NULL CHECK (task_type IN (
        'library_scan', 'metadata_refresh', 'database_maintenance',
        'partition_management', 'session_cleanup', 'trakt_sync',
        'backup_database', 'backup_verification', 'database_integrity_check',
        'backup_retention_cleanup', 'media_health_check', 'notification_cleanup',
        'trust_recalculation', 'soft_delete_purge', 'segment_analysis',
        'storyboard_generation', 'disk_space_check', 'reindex_maintenance',
        'analyze_parents', 'transcode_health_check', 'subtitle_ocr',
        'subtitle_voice_analysis', 'subtitle_auto_fetch',
        'overlay_application', 'overlay_cleanup',
        'collection_sync', 'collection_cleanup',
        'artwork_refresh', 'asset_directory_scan',
        'migration_cleanup', 'system_requirement_check',
        'geoip_database_update'
    )),

    cron_expression TEXT,
    interval_seconds INT,
    is_enabled BOOLEAN NOT NULL DEFAULT true,

    timeout_seconds INT NOT NULL DEFAULT 3600,
    max_retries INT NOT NULL DEFAULT 3,
    retry_delay_seconds INT NOT NULL DEFAULT 300,

    state TEXT NOT NULL DEFAULT 'idle' CHECK (state IN ('idle', 'queued', 'running', 'completed', 'failed', 'cancelled')),
    consecutive_failures INT NOT NULL DEFAULT 0,

    last_run_at TIMESTAMPTZ,
    last_run_duration_ms INT,
    last_run_result TEXT CHECK (last_run_result IN ('success', 'failure', 'timeout', 'cancelled')),
    last_error TEXT,
    next_run_at TIMESTAMPTZ,

    config JSONB NOT NULL DEFAULT '{}',
    metadata JSONB NOT NULL DEFAULT '{}'
);

CREATE INDEX IF NOT EXISTS idx_scheduled_tasks_task_type ON scheduled_tasks (task_type);
CREATE INDEX IF NOT EXISTS idx_scheduled_tasks_state ON scheduled_tasks (state);
CREATE INDEX IF NOT EXISTS idx_scheduled_tasks_next_run_at ON scheduled_tasks (next_run_at) WHERE is_enabled = true;

CREATE TABLE IF NOT EXISTS scheduled_task_runs (
    id UUID DEFAULT uuidv7() PRIMARY KEY,
    created_at TIMESTAMPTZ GENERATED ALWAYS AS (uuid_extract_timestamp(id)) STORED,

    scheduled_task_id UUID NOT NULL REFERENCES scheduled_tasks(id) ON DELETE CASCADE,

    trigger_type TEXT NOT NULL CHECK (trigger_type IN ('scheduled', 'manual', 'retry')),
    state TEXT NOT NULL CHECK (state IN ('running', 'completed', 'failed', 'cancelled')),

    started_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    completed_at TIMESTAMPTZ,
    duration_ms INT,

    result TEXT CHECK (result IN ('success', 'failure', 'timeout', 'cancelled')),
    error_message TEXT,
    error_details JSONB,

    stats JSONB NOT NULL DEFAULT '{}'
);

CREATE INDEX IF NOT EXISTS idx_scheduled_task_runs_task_id ON scheduled_task_runs (scheduled_task_id);
CREATE INDEX IF NOT EXISTS idx_scheduled_task_runs_started_at ON scheduled_task_runs (started_at DESC);
CREATE INDEX IF NOT EXISTS idx_scheduled_task_runs_state ON scheduled_task_runs (state);
CREATE INDEX IF NOT EXISTS idx_scheduled_task_runs_failed ON scheduled_task_runs (result) WHERE result = 'failure';

CREATE TABLE IF NOT EXISTS notification_types (
    id UUID DEFAULT uuidv7() PRIMARY KEY,
    created_at TIMESTAMPTZ GENERATED ALWAYS AS (uuid_extract_timestamp(id)) STORED,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),

    name TEXT NOT NULL UNIQUE,
    category TEXT NOT NULL CHECK (category IN ('media', 'system', 'security', 'user', 'task')),
    priority TEXT NOT NULL DEFAULT 'low' CHECK (priority IN ('low', 'medium', 'high')),

    in_app_template TEXT NOT NULL,
    email_template TEXT,
    webhook_payload_template JSONB,

    is_enabled_by_default BOOLEAN NOT NULL DEFAULT true,

    metadata JSONB NOT NULL DEFAULT '{}'
);

CREATE TABLE IF NOT EXISTS notifications (
    id UUID DEFAULT uuidv7() PRIMARY KEY,
    created_at TIMESTAMPTZ GENERATED ALWAYS AS (uuid_extract_timestamp(id)) STORED,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),

    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    notification_type_id UUID NOT NULL REFERENCES notification_types(id) ON DELETE CASCADE,

    title TEXT NOT NULL,
    body TEXT NOT NULL,
    priority TEXT NOT NULL DEFAULT 'low' CHECK (priority IN ('low', 'medium', 'high')),

    link TEXT,

    is_read BOOLEAN NOT NULL DEFAULT false,
    read_at TIMESTAMPTZ,

    delivery_channels JSONB NOT NULL DEFAULT '["in_app"]',
    delivery_status JSONB NOT NULL DEFAULT '{}',

    related_item_type TEXT,
    related_item_id UUID,

    expires_at TIMESTAMPTZ,

    metadata JSONB NOT NULL DEFAULT '{}'
);

CREATE INDEX IF NOT EXISTS idx_notifications_user_id ON notifications (user_id);
CREATE INDEX IF NOT EXISTS idx_notifications_type ON notifications (notification_type_id);
CREATE INDEX IF NOT EXISTS idx_notifications_unread ON notifications (user_id, created_at DESC) WHERE is_read = false;
CREATE INDEX IF NOT EXISTS idx_notifications_created_at ON notifications (created_at DESC);
CREATE INDEX IF NOT EXISTS idx_notifications_expires ON notifications (expires_at) WHERE expires_at IS NOT NULL;

CREATE TABLE IF NOT EXISTS user_notification_preferences (
    id UUID DEFAULT uuidv7() PRIMARY KEY,
    created_at TIMESTAMPTZ GENERATED ALWAYS AS (uuid_extract_timestamp(id)) STORED,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),

    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    notification_type_id UUID NOT NULL REFERENCES notification_types(id) ON DELETE CASCADE,

    in_app_enabled BOOLEAN NOT NULL DEFAULT true,
    email_enabled BOOLEAN NOT NULL DEFAULT false,
    webhook_enabled BOOLEAN NOT NULL DEFAULT false,

    UNIQUE(user_id, notification_type_id)
);

CREATE INDEX IF NOT EXISTS idx_user_notification_prefs_user ON user_notification_preferences (user_id);

CREATE TABLE IF NOT EXISTS download_jobs (
    id UUID DEFAULT uuidv7() PRIMARY KEY,
    created_at TIMESTAMPTZ GENERATED ALWAYS AS (uuid_extract_timestamp(id)) STORED,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),

    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    user_session_id UUID REFERENCES user_sessions(id) ON DELETE SET NULL,
    device_identifier TEXT NOT NULL,
    device_name TEXT,
    client_platform TEXT NOT NULL CHECK (client_platform IN ('android', 'ios')),
    client_version TEXT,

    library_id UUID NOT NULL REFERENCES libraries(id) ON DELETE CASCADE,
    media_item_id UUID NOT NULL REFERENCES media_items(id) ON DELETE CASCADE,
    media_file_id UUID REFERENCES media_files(id) ON DELETE SET NULL,
    device_profile_id UUID REFERENCES device_profiles(id) ON DELETE SET NULL,

    status TEXT NOT NULL DEFAULT 'queued'
        CHECK (status IN ('queued', 'preparing', 'ready', 'failed', 'cancelled', 'expired', 'revoked')),
    package_format TEXT NOT NULL
        CHECK (package_format IN ('hls_fmp4', 'mp4')),
    package_strategy TEXT NOT NULL
        CHECK (package_strategy IN ('direct_copy', 'remux', 'transcode')),
    quality_mode TEXT NOT NULL
        CHECK (quality_mode IN ('auto', 'data_saver', 'standard', 'maximum', 'manual')),
    quality_label TEXT,

    selected_audio JSONB NOT NULL DEFAULT '{}',
    selected_subtitles JSONB NOT NULL DEFAULT '[]',
    selected_artwork JSONB NOT NULL DEFAULT '{}',

    progress_percent NUMERIC(5,2) NOT NULL DEFAULT 0
        CHECK (progress_percent >= 0 AND progress_percent <= 100),
    bytes_expected BIGINT CHECK (bytes_expected IS NULL OR bytes_expected >= 0),
    bytes_prepared BIGINT NOT NULL DEFAULT 0 CHECK (bytes_prepared >= 0),

    plan_revision TEXT NOT NULL,
    plan_hash TEXT NOT NULL,
    access_policy_snapshot JSONB NOT NULL DEFAULT '{}',

    failure_reason TEXT,
    failure_details JSONB NOT NULL DEFAULT '{}',
    retry_count INT NOT NULL DEFAULT 0 CHECK (retry_count >= 0),
    cancellation_requested BOOLEAN NOT NULL DEFAULT false,

    started_at TIMESTAMPTZ,
    completed_at TIMESTAMPTZ,
    expires_at TIMESTAMPTZ,
    cleanup_after_at TIMESTAMPTZ,

    metadata JSONB NOT NULL DEFAULT '{}'
);

CREATE INDEX IF NOT EXISTS idx_download_jobs_user_status
    ON download_jobs (user_id, status, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_download_jobs_worker_queue
    ON download_jobs (status, created_at)
    WHERE status IN ('queued', 'preparing');
CREATE INDEX IF NOT EXISTS idx_download_jobs_media_item
    ON download_jobs (media_item_id, user_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_download_jobs_device
    ON download_jobs (user_id, device_identifier, status);
CREATE INDEX IF NOT EXISTS idx_download_jobs_expiry_cleanup
    ON download_jobs (expires_at, cleanup_after_at)
    WHERE status IN ('ready', 'failed', 'cancelled', 'expired', 'revoked');
CREATE INDEX IF NOT EXISTS idx_download_jobs_policy_snapshot
    ON download_jobs USING GIN (access_policy_snapshot jsonb_path_ops);

CREATE TABLE IF NOT EXISTS download_packages (
    id UUID DEFAULT uuidv7() PRIMARY KEY,
    created_at TIMESTAMPTZ GENERATED ALWAYS AS (uuid_extract_timestamp(id)) STORED,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),

    download_job_id UUID NOT NULL UNIQUE REFERENCES download_jobs(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    user_session_id UUID REFERENCES user_sessions(id) ON DELETE SET NULL,
    device_identifier TEXT NOT NULL,

    library_id UUID NOT NULL REFERENCES libraries(id) ON DELETE CASCADE,
    media_item_id UUID NOT NULL REFERENCES media_items(id) ON DELETE CASCADE,
    media_file_id UUID REFERENCES media_files(id) ON DELETE SET NULL,

    status TEXT NOT NULL DEFAULT 'ready'
        CHECK (status IN ('ready', 'serving', 'expired', 'revoked', 'cleanup_pending', 'cleaned', 'failed')),
    package_format TEXT NOT NULL
        CHECK (package_format IN ('hls_fmp4', 'mp4')),
    manifest_version INT NOT NULL DEFAULT 1 CHECK (manifest_version > 0),
    manifest_relative_path TEXT NOT NULL,
    storage_key TEXT NOT NULL UNIQUE,

    total_bytes BIGINT NOT NULL DEFAULT 0 CHECK (total_bytes >= 0),
    file_count INT NOT NULL DEFAULT 0 CHECK (file_count >= 0),
    package_hash_sha256 TEXT,
    manifest_hash_sha256 TEXT,

    selected_audio JSONB NOT NULL DEFAULT '{}',
    selected_subtitles JSONB NOT NULL DEFAULT '[]',
    included_artwork JSONB NOT NULL DEFAULT '{}',
    included_storyboards JSONB NOT NULL DEFAULT '{}',
    sync_metadata JSONB NOT NULL DEFAULT '{}',
    access_policy_snapshot JSONB NOT NULL DEFAULT '{}',

    first_served_at TIMESTAMPTZ,
    last_served_at TIMESTAMPTZ,
    expires_at TIMESTAMPTZ,
    revoked_at TIMESTAMPTZ,
    cleanup_after_at TIMESTAMPTZ,

    metadata JSONB NOT NULL DEFAULT '{}'
);

CREATE INDEX IF NOT EXISTS idx_download_packages_user_inventory
    ON download_packages (user_id, status, media_item_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_download_packages_device_inventory
    ON download_packages (user_id, device_identifier, status, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_download_packages_expiry_cleanup
    ON download_packages (expires_at, cleanup_after_at)
    WHERE status IN ('ready', 'serving', 'expired', 'revoked', 'cleanup_pending');
CREATE INDEX IF NOT EXISTS idx_download_packages_storage_key
    ON download_packages (storage_key);
CREATE INDEX IF NOT EXISTS idx_download_packages_policy_snapshot
    ON download_packages USING GIN (access_policy_snapshot jsonb_path_ops);

CREATE TABLE IF NOT EXISTS download_package_files (
    id UUID DEFAULT uuidv7() PRIMARY KEY,
    created_at TIMESTAMPTZ GENERATED ALWAYS AS (uuid_extract_timestamp(id)) STORED,

    download_package_id UUID NOT NULL REFERENCES download_packages(id) ON DELETE CASCADE,

    relative_path TEXT NOT NULL,
    file_role TEXT NOT NULL
        CHECK (file_role IN ('manifest', 'init_segment', 'media_segment', 'subtitle', 'artwork', 'storyboard', 'mp4', 'checksum', 'metadata')),
    content_type TEXT,
    byte_size BIGINT NOT NULL CHECK (byte_size >= 0),
    checksum_sha256 TEXT NOT NULL,

    segment_index INT CHECK (segment_index IS NULL OR segment_index >= 0),
    track_type TEXT CHECK (track_type IS NULL OR track_type IN ('video', 'audio', 'subtitle', 'image', 'metadata')),
    track_identifier TEXT,
    is_required BOOLEAN NOT NULL DEFAULT true,

    metadata JSONB NOT NULL DEFAULT '{}',

    UNIQUE(download_package_id, relative_path)
);

CREATE INDEX IF NOT EXISTS idx_download_package_files_package
    ON download_package_files (download_package_id, file_role, segment_index);
CREATE INDEX IF NOT EXISTS idx_download_package_files_checksum
    ON download_package_files (checksum_sha256);

CREATE TABLE IF NOT EXISTS download_device_state (
    id UUID DEFAULT uuidv7() PRIMARY KEY,
    created_at TIMESTAMPTZ GENERATED ALWAYS AS (uuid_extract_timestamp(id)) STORED,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),

    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    user_session_id UUID REFERENCES user_sessions(id) ON DELETE SET NULL,
    download_package_id UUID NOT NULL REFERENCES download_packages(id) ON DELETE CASCADE,
    device_identifier TEXT NOT NULL,
    device_name TEXT,
    client_platform TEXT NOT NULL CHECK (client_platform IN ('android', 'ios')),
    client_version TEXT,

    local_status TEXT NOT NULL DEFAULT 'not_downloaded'
        CHECK (local_status IN ('not_downloaded', 'downloading', 'paused', 'playable', 'failed', 'expired', 'revoked', 'deleted', 'sync_pending')),
    bytes_downloaded BIGINT NOT NULL DEFAULT 0 CHECK (bytes_downloaded >= 0),
    files_verified INT NOT NULL DEFAULT 0 CHECK (files_verified >= 0),
    local_manifest_hash_sha256 TEXT,

    last_online_check_at TIMESTAMPTZ,
    last_download_progress_at TIMESTAMPTZ,
    last_played_at TIMESTAMPTZ,
    local_resume_position_ms BIGINT NOT NULL DEFAULT 0 CHECK (local_resume_position_ms >= 0),
    pending_sync JSONB NOT NULL DEFAULT '[]',
    sync_cursor TEXT,

    deletion_requested BOOLEAN NOT NULL DEFAULT false,
    deleted_at TIMESTAMPTZ,
    failure_reason TEXT,
    failure_details JSONB NOT NULL DEFAULT '{}',

    metadata JSONB NOT NULL DEFAULT '{}',

    UNIQUE(user_id, device_identifier, download_package_id)
);

CREATE INDEX IF NOT EXISTS idx_download_device_state_inventory
    ON download_device_state (user_id, device_identifier, local_status, updated_at DESC);
CREATE INDEX IF NOT EXISTS idx_download_device_state_package
    ON download_device_state (download_package_id);
CREATE INDEX IF NOT EXISTS idx_download_device_state_sync
    ON download_device_state (user_id, device_identifier, updated_at)
    WHERE local_status = 'sync_pending' OR jsonb_array_length(pending_sync) > 0;
CREATE INDEX IF NOT EXISTS idx_download_device_state_deleted
    ON download_device_state (deleted_at)
    WHERE deleted_at IS NOT NULL;

CREATE TABLE IF NOT EXISTS download_events (
    id UUID DEFAULT uuidv7() PRIMARY KEY,
    created_at TIMESTAMPTZ GENERATED ALWAYS AS (uuid_extract_timestamp(id)) STORED,

    user_id UUID REFERENCES users(id) ON DELETE SET NULL,
    user_session_id UUID REFERENCES user_sessions(id) ON DELETE SET NULL,
    download_job_id UUID REFERENCES download_jobs(id) ON DELETE SET NULL,
    download_package_id UUID REFERENCES download_packages(id) ON DELETE SET NULL,
    media_item_id UUID REFERENCES media_items(id) ON DELETE SET NULL,
    device_identifier TEXT,

    event_type TEXT NOT NULL CHECK (event_type IN (
        'job_created',
        'job_started',
        'job_ready',
        'job_failed',
        'job_cancelled',
        'package_served',
        'package_deleted',
        'package_expired',
        'package_revoked',
        'quota_denied',
        'policy_denied',
        'checksum_mismatch',
        'sync_submitted',
        'cleanup'
    )),
    reason TEXT,
    details JSONB NOT NULL DEFAULT '{}'
);

CREATE INDEX IF NOT EXISTS idx_download_events_user
    ON download_events (user_id, created_at DESC) WHERE user_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_download_events_job
    ON download_events (download_job_id, created_at DESC) WHERE download_job_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_download_events_package
    ON download_events (download_package_id, created_at DESC) WHERE download_package_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_download_events_type
    ON download_events (event_type, created_at DESC);

DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_trigger WHERE tgname = 'audit_download_jobs') THEN
        CREATE TRIGGER audit_download_jobs
            AFTER INSERT OR UPDATE OR DELETE ON download_jobs
            FOR EACH ROW EXECUTE FUNCTION audit_trigger_fn();
    END IF;

    IF NOT EXISTS (SELECT 1 FROM pg_trigger WHERE tgname = 'audit_download_packages') THEN
        CREATE TRIGGER audit_download_packages
            AFTER INSERT OR UPDATE OR DELETE ON download_packages
            FOR EACH ROW EXECUTE FUNCTION audit_trigger_fn();
    END IF;

    IF NOT EXISTS (SELECT 1 FROM pg_trigger WHERE tgname = 'audit_download_device_state') THEN
        CREATE TRIGGER audit_download_device_state
            AFTER INSERT OR UPDATE OR DELETE ON download_device_state
            FOR EACH ROW EXECUTE FUNCTION audit_trigger_fn();
    END IF;
END $$;

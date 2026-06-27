CREATE TABLE IF NOT EXISTS device_profiles (
    id UUID DEFAULT uuidv7() PRIMARY KEY,
    created_at TIMESTAMPTZ GENERATED ALWAYS AS (uuid_extract_timestamp(id)) STORED,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),

    device_identifier TEXT NOT NULL,

    platform TEXT NOT NULL,
    model TEXT,
    os_version TEXT,
    client_name TEXT,
    client_version TEXT,

    video_codecs JSONB NOT NULL DEFAULT '[]',
    audio_codecs JSONB NOT NULL DEFAULT '[]',
    subtitle_formats JSONB NOT NULL DEFAULT '[]',
    containers JSONB NOT NULL DEFAULT '[]',

    max_resolution TEXT,
    max_framerate INT,
    hdr_support JSONB NOT NULL DEFAULT '[]',

    max_audio_channels INT,
    spatial_audio BOOLEAN NOT NULL DEFAULT false,

    max_bitrate_bps BIGINT,

    allow_client_side_dv_fallback BOOLEAN NOT NULL DEFAULT true,

    profile_source TEXT NOT NULL DEFAULT 'client_report'
        CHECK (profile_source IN ('client_report', 'capability_wizard', 'known_device', 'manual')),

    wizard_completed_at TIMESTAMPTZ,

    metadata JSONB NOT NULL DEFAULT '{}'
);

CREATE UNIQUE INDEX IF NOT EXISTS device_profiles_identifier ON device_profiles (device_identifier);
CREATE INDEX IF NOT EXISTS device_profiles_platform ON device_profiles (platform);
CREATE INDEX IF NOT EXISTS device_profiles_source ON device_profiles (profile_source);

CREATE TABLE IF NOT EXISTS device_capability_tests (
    id UUID DEFAULT uuidv7() PRIMARY KEY,
    created_at TIMESTAMPTZ GENERATED ALWAYS AS (uuid_extract_timestamp(id)) STORED,

    device_profile_id UUID NOT NULL REFERENCES device_profiles(id) ON DELETE CASCADE,

    test_format TEXT NOT NULL,
    test_description TEXT NOT NULL,

    result TEXT NOT NULL CHECK (result IN ('success', 'failed', 'stuttered')),

    actual_codec TEXT,
    actual_resolution TEXT,
    actual_bit_depth INT,
    actual_dynamic_range TEXT,

    details JSONB NOT NULL DEFAULT '{}'
);

CREATE INDEX IF NOT EXISTS idx_device_capability_tests_profile ON device_capability_tests (device_profile_id);
CREATE INDEX IF NOT EXISTS idx_device_capability_tests_format ON device_capability_tests (test_format);

CREATE TABLE IF NOT EXISTS client_network_reports (
    id UUID DEFAULT uuidv7() PRIMARY KEY,
    created_at TIMESTAMPTZ GENERATED ALWAYS AS (uuid_extract_timestamp(id)) STORED,

    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    session_id UUID NOT NULL,

    report_type TEXT NOT NULL CHECK (report_type IN ('segment', 'probe')),

    segment_index INT,
    rung TEXT,

    payload_bytes BIGINT,
    download_start_ms BIGINT,
    download_end_ms BIGINT,
    throughput_bps BIGINT,

    buffer_seconds REAL,
    rebuffer_count INT,
    rebuffer_total_ms INT,

    estimated_throughput_bps BIGINT,
    network_tier TEXT CHECK (network_tier IN ('excellent', 'good', 'moderate', 'slow', 'very_slow', 'critical')),

    metadata JSONB NOT NULL DEFAULT '{}'
);

CREATE INDEX IF NOT EXISTS idx_client_network_reports_user ON client_network_reports (user_id);
CREATE INDEX IF NOT EXISTS idx_client_network_reports_session ON client_network_reports (session_id);
CREATE INDEX IF NOT EXISTS idx_client_network_reports_created ON client_network_reports (created_at DESC);
CREATE INDEX IF NOT EXISTS idx_client_network_reports_tier ON client_network_reports (network_tier);

CREATE TABLE IF NOT EXISTS qoe_reports (
    id UUID DEFAULT uuidv7() PRIMARY KEY,
    created_at TIMESTAMPTZ GENERATED ALWAYS AS (uuid_extract_timestamp(id)) STORED,

    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    session_id UUID NOT NULL,

    report_interval_seconds INT NOT NULL DEFAULT 30,

    startup_time_ms INT,
    rebuffer_ratio REAL,
    average_bitrate_bps BIGINT,
    switches_per_minute REAL,
    quality_drops INT,

    current_rung TEXT,
    current_buffer_seconds REAL,

    metadata JSONB NOT NULL DEFAULT '{}'
);

CREATE INDEX IF NOT EXISTS idx_qoe_reports_user ON qoe_reports (user_id);
CREATE INDEX IF NOT EXISTS idx_qoe_reports_session ON qoe_reports (session_id);
CREATE INDEX IF NOT EXISTS idx_qoe_reports_created ON qoe_reports (created_at DESC);

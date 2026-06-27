CREATE TABLE IF NOT EXISTS play_sessions (
    id UUID DEFAULT uuidv7(),
    created_at TIMESTAMPTZ GENERATED ALWAYS AS (uuid_extract_timestamp(id)) STORED,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),

    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    media_item_id UUID NOT NULL REFERENCES media_items(id) ON DELETE CASCADE,
    library_id UUID NOT NULL REFERENCES libraries(id) ON DELETE CASCADE,

    started_at TIMESTAMPTZ NOT NULL,
    stopped_at TIMESTAMPTZ,
    paused_seconds INT NOT NULL DEFAULT 0,
    duration_seconds INT NOT NULL DEFAULT 0,

    ip_address INET,
    location_type TEXT CHECK (location_type IN ('lan', 'wan', 'relay')),
    geo_city TEXT,
    geo_region TEXT,
    geo_country TEXT,
    geo_lat REAL,
    geo_lon REAL,

    client_name TEXT NOT NULL,
    client_product TEXT,
    client_platform TEXT,
    client_version TEXT,
    client_device TEXT,

    is_secure BOOLEAN NOT NULL DEFAULT false,
    bandwidth_bps BIGINT,
    quality_profile TEXT,

    stream_decision TEXT NOT NULL CHECK (stream_decision IN ('direct_play', 'direct_stream', 'transcode')),
    percent_complete REAL,
    plays_in_session INT NOT NULL DEFAULT 1,

    metadata JSONB NOT NULL DEFAULT '{}'
) PARTITION BY RANGE (started_at);

CREATE TABLE IF NOT EXISTS play_sessions_2026_06 PARTITION OF play_sessions
    FOR VALUES FROM ('2026-06-01') TO ('2026-07-01');

CREATE TABLE IF NOT EXISTS play_sessions_2026_07 PARTITION OF play_sessions
    FOR VALUES FROM ('2026-07-01') TO ('2026-08-01');

CREATE INDEX IF NOT EXISTS idx_play_sessions_user_id ON play_sessions (user_id);
CREATE INDEX IF NOT EXISTS idx_play_sessions_id ON play_sessions (id);
CREATE INDEX IF NOT EXISTS idx_play_sessions_media_item_id ON play_sessions (media_item_id);
CREATE INDEX IF NOT EXISTS idx_play_sessions_library_id ON play_sessions (library_id);
CREATE INDEX IF NOT EXISTS idx_play_sessions_started_at ON play_sessions (started_at DESC);
CREATE INDEX IF NOT EXISTS idx_play_sessions_stream_decision ON play_sessions (stream_decision);
CREATE INDEX IF NOT EXISTS idx_play_sessions_ip_address ON play_sessions (ip_address);
CREATE INDEX IF NOT EXISTS idx_play_sessions_location_type ON play_sessions (location_type);
CREATE INDEX IF NOT EXISTS idx_play_sessions_metadata ON play_sessions USING GIN (metadata jsonb_path_ops);

CREATE TABLE IF NOT EXISTS play_session_streams (
    id UUID DEFAULT uuidv7() PRIMARY KEY,
    created_at TIMESTAMPTZ GENERATED ALWAYS AS (uuid_extract_timestamp(id)) STORED,

    play_session_id UUID NOT NULL UNIQUE,

    source_video_codec TEXT,
    source_video_resolution TEXT,
    source_video_bitrate INT,
    source_video_dynamic_range TEXT,
    source_video_frame_rate NUMERIC(6,3),
    source_video_scan_type TEXT,
    source_video_bit_depth INT,

    source_audio_codec TEXT,
    source_audio_channels INT,
    source_audio_bitrate INT,
    source_audio_language TEXT,

    source_container TEXT,
    source_total_bitrate INT,

    transcode_protocol TEXT,
    transcode_container TEXT,
    transcode_video_codec TEXT,
    transcode_audio_codec TEXT,
    transcode_audio_channels INT,
    transcode_video_width INT,
    transcode_video_height INT,
    transcode_hw_decode TEXT,
    transcode_hw_encode TEXT,
    transcode_hw_accelerated BOOLEAN NOT NULL DEFAULT false,

    stream_video_codec TEXT,
    stream_video_resolution TEXT,
    stream_video_bitrate INT,
    stream_video_dynamic_range TEXT,
    stream_video_frame_rate NUMERIC(6,3),

    stream_audio_codec TEXT,
    stream_audio_channels INT,
    stream_audio_bitrate INT,
    stream_audio_language TEXT,

    stream_container TEXT,
    stream_total_bitrate INT,

    subtitle_codec TEXT,
    subtitle_language TEXT,
    subtitle_forced BOOLEAN NOT NULL DEFAULT false,

    additional_streams JSONB DEFAULT '{}'
);

CREATE INDEX IF NOT EXISTS idx_play_session_streams_play_session_id ON play_session_streams (play_session_id);

CREATE TABLE IF NOT EXISTS play_events (
    id UUID DEFAULT uuidv7(),
    created_at TIMESTAMPTZ GENERATED ALWAYS AS (uuid_extract_timestamp(id)) STORED,

    play_session_id UUID NOT NULL,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,

    event_type TEXT NOT NULL CHECK (event_type IN (
        'play', 'pause', 'stop', 'resume', 'buffer_start', 'buffer_end',
        'seek', 'error', 'transcode_change', 'heartbeat'
    )),
    event_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    position_seconds INT,
    details JSONB DEFAULT '{}'
) PARTITION BY RANGE (event_at);

CREATE TABLE IF NOT EXISTS play_events_2026_06 PARTITION OF play_events
    FOR VALUES FROM ('2026-06-01') TO ('2026-07-01');

CREATE TABLE IF NOT EXISTS play_events_2026_07 PARTITION OF play_events
    FOR VALUES FROM ('2026-07-01') TO ('2026-08-01');

CREATE INDEX IF NOT EXISTS idx_play_events_play_session_id ON play_events (play_session_id);
CREATE INDEX IF NOT EXISTS idx_play_events_id ON play_events (id);
CREATE INDEX IF NOT EXISTS idx_play_events_user_id ON play_events (user_id);
CREATE INDEX IF NOT EXISTS idx_play_events_event_type ON play_events (event_type);
CREATE INDEX IF NOT EXISTS idx_play_events_event_at ON play_events (event_at DESC);

CREATE TABLE IF NOT EXISTS user_trust_events (
    id UUID DEFAULT uuidv7() PRIMARY KEY,
    created_at TIMESTAMPTZ GENERATED ALWAYS AS (uuid_extract_timestamp(id)) STORED,

    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    play_session_id UUID,

    rule_type TEXT NOT NULL CHECK (rule_type IN (
        'impossible_travel', 'simultaneous_locations', 'device_velocity',
        'concurrent_streams', 'geo_restriction', 'account_inactivity'
    )),
    severity TEXT NOT NULL CHECK (severity IN ('low', 'medium', 'high')),
    score_impact INT NOT NULL DEFAULT 0,
    details JSONB NOT NULL DEFAULT '{}',
    acknowledged BOOLEAN NOT NULL DEFAULT false,
    acknowledged_at TIMESTAMPTZ
);

CREATE INDEX IF NOT EXISTS idx_user_trust_events_user_id ON user_trust_events (user_id);
CREATE INDEX IF NOT EXISTS idx_user_trust_events_rule_type ON user_trust_events (rule_type);
CREATE INDEX IF NOT EXISTS idx_user_trust_events_created_at ON user_trust_events (created_at DESC);
CREATE INDEX IF NOT EXISTS idx_user_trust_events_unack ON user_trust_events (user_id) WHERE acknowledged = false;

CREATE TABLE IF NOT EXISTS user_trust_scores (
    id UUID DEFAULT uuidv7() PRIMARY KEY,
    created_at TIMESTAMPTZ GENERATED ALWAYS AS (uuid_extract_timestamp(id)) STORED,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),

    user_id UUID NOT NULL UNIQUE REFERENCES users(id) ON DELETE CASCADE,

    score INT NOT NULL DEFAULT 100 CHECK (score BETWEEN 0 AND 100),
    total_violations INT NOT NULL DEFAULT 0,
    last_violation_at TIMESTAMPTZ,
    last_good_session_at TIMESTAMPTZ,

    metadata JSONB NOT NULL DEFAULT '{}'
);

CREATE INDEX IF NOT EXISTS idx_user_trust_scores_score ON user_trust_scores (score);

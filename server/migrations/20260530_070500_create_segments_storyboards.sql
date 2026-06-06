CREATE TABLE IF NOT EXISTS media_segments (
    id UUID DEFAULT uuidv7() PRIMARY KEY,
    created_at TIMESTAMPTZ GENERATED ALWAYS AS (uuid_extract_timestamp(id)) STORED,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),

    media_item_id UUID NOT NULL REFERENCES media_items(id) ON DELETE CASCADE,

    segment_type TEXT NOT NULL CHECK (segment_type IN (
        'intro', 'credits', 'recap', 'preview', 'outro'
    )),

    start_ms INT NOT NULL CHECK (start_ms >= 0),
    end_ms INT NOT NULL CHECK (end_ms > start_ms),

    skip_to_ms INT NOT NULL CHECK (skip_to_ms >= start_ms AND skip_to_ms <= end_ms),

    confidence REAL NOT NULL DEFAULT 1.0 CHECK (confidence BETWEEN 0 AND 1),
    source TEXT NOT NULL CHECK (source IN ('chapter', 'chromaprint', 'blackframe', 'silence', 'manual', 'combined')),

    is_manual BOOLEAN NOT NULL DEFAULT false,

    metadata JSONB NOT NULL DEFAULT '{}'
);

CREATE INDEX IF NOT EXISTS idx_media_segments_media_item_id ON media_segments (media_item_id);
CREATE INDEX IF NOT EXISTS idx_media_segments_type ON media_segments (segment_type);
CREATE UNIQUE INDEX IF NOT EXISTS media_segments_item_type_unique ON media_segments (media_item_id, segment_type) WHERE is_manual = true;

CREATE TABLE IF NOT EXISTS media_fingerprints (
    id UUID DEFAULT uuidv7() PRIMARY KEY,
    created_at TIMESTAMPTZ GENERATED ALWAYS AS (uuid_extract_timestamp(id)) STORED,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),

    media_file_id UUID NOT NULL UNIQUE REFERENCES media_files(id) ON DELETE CASCADE,

    file_hash TEXT NOT NULL,

    fingerprint BYTEA NOT NULL,
    fingerprint_algorithm TEXT NOT NULL DEFAULT 'test2',
    fingerprint_duration_ms INT NOT NULL,

    chapters_json JSONB,

    metadata JSONB NOT NULL DEFAULT '{}'
);

CREATE INDEX IF NOT EXISTS idx_media_fingerprints_media_file_id ON media_fingerprints (media_file_id);
CREATE INDEX IF NOT EXISTS idx_media_fingerprints_file_hash ON media_fingerprints (file_hash);

CREATE TABLE IF NOT EXISTS storyboards (
    id UUID DEFAULT uuidv7() PRIMARY KEY,
    created_at TIMESTAMPTZ GENERATED ALWAYS AS (uuid_extract_timestamp(id)) STORED,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),

    media_file_id UUID NOT NULL UNIQUE REFERENCES media_files(id) ON DELETE CASCADE,

    file_hash TEXT NOT NULL,

    interval_seconds INT NOT NULL,
    width INT NOT NULL,
    height INT NOT NULL,
    sprite_count INT NOT NULL,
    total_thumbnails INT NOT NULL,
    total_size_bytes BIGINT NOT NULL,

    keyframe_only BOOLEAN NOT NULL DEFAULT true,
    quality INT NOT NULL DEFAULT 75,

    generated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    generation_duration_ms INT,

    metadata JSONB NOT NULL DEFAULT '{}'
);

CREATE INDEX IF NOT EXISTS idx_storyboards_media_file_id ON storyboards (media_file_id);
CREATE INDEX IF NOT EXISTS idx_storyboards_file_hash ON storyboards (file_hash);

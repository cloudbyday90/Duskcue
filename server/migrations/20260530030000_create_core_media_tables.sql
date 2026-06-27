CREATE TABLE IF NOT EXISTS libraries (
    id UUID DEFAULT uuidv7() PRIMARY KEY,
    created_at TIMESTAMPTZ GENERATED ALWAYS AS (uuid_extract_timestamp(id)) STORED,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),

    name TEXT NOT NULL,
    slug TEXT NOT NULL,
    media_type TEXT NOT NULL CHECK (media_type IN ('movies', 'tvshows')),
    root_path TEXT NOT NULL,
    scan_enabled BOOLEAN NOT NULL DEFAULT true,
    scan_interval_seconds INT NOT NULL DEFAULT 86400,
    metadata_language TEXT NOT NULL DEFAULT 'en',
    metadata JSONB NOT NULL DEFAULT '{}',
    last_scan_at TIMESTAMPTZ,
    deleted_at TIMESTAMPTZ
);

CREATE UNIQUE INDEX IF NOT EXISTS libraries_slug_active ON libraries (slug) WHERE deleted_at IS NULL;

CREATE TABLE IF NOT EXISTS library_paths (
    id UUID DEFAULT uuidv7() PRIMARY KEY,
    created_at TIMESTAMPTZ GENERATED ALWAYS AS (uuid_extract_timestamp(id)) STORED,
    library_id UUID NOT NULL REFERENCES libraries(id) ON DELETE CASCADE,
    path TEXT NOT NULL,
    is_default BOOLEAN NOT NULL DEFAULT false,
    scan_enabled BOOLEAN NOT NULL DEFAULT true,
    last_scan_at TIMESTAMPTZ,
    UNIQUE(library_id, path)
);

CREATE INDEX IF NOT EXISTS idx_library_paths_library ON library_paths (library_id);

CREATE TABLE IF NOT EXISTS media_items (
    id UUID DEFAULT uuidv7() PRIMARY KEY,
    created_at TIMESTAMPTZ GENERATED ALWAYS AS (uuid_extract_timestamp(id)) STORED,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),

    library_id UUID NOT NULL REFERENCES libraries(id) ON DELETE CASCADE,
    type TEXT NOT NULL CHECK (type IN ('movie', 'series', 'season', 'episode')),

    title TEXT NOT NULL,
    sort_title TEXT NOT NULL,
    original_title TEXT,
    overview TEXT,

    premiere_date DATE,
    end_date DATE,
    content_rating TEXT,
    runtime_seconds INT,

    tmdb_id BIGINT,
    imdb_id TEXT,
    tvdb_id BIGINT,
    trakt_id BIGINT,

    rating_average REAL,
    rating_vote_count INT,

    search_vector TSVECTOR,
    metadata JSONB NOT NULL DEFAULT '{}',

    match_state TEXT NOT NULL DEFAULT 'confirmed'
        CHECK (match_state IN ('unmatched', 'auto_matched', 'confirmed', 'manual')),
    identification_source TEXT
        CHECK (identification_source IS NULL OR identification_source IN (
            'media_match', 'nfo', 'provider_id_tag', 'filename_parse', 'manual'
        ))
);

CREATE INDEX IF NOT EXISTS idx_media_items_library_id ON media_items (library_id);
CREATE INDEX IF NOT EXISTS idx_media_items_type ON media_items (type);
CREATE INDEX IF NOT EXISTS idx_media_items_sort_title ON media_items (sort_title);
CREATE INDEX IF NOT EXISTS idx_media_items_premiere_date ON media_items (premiere_date DESC NULLS LAST);
CREATE INDEX IF NOT EXISTS idx_media_items_tmdb_id ON media_items (tmdb_id) WHERE tmdb_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_media_items_imdb_id ON media_items (imdb_id) WHERE imdb_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_media_items_tvdb_id ON media_items (tvdb_id) WHERE tvdb_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_media_items_trakt_id ON media_items (trakt_id) WHERE trakt_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_media_items_match_state ON media_items (match_state) WHERE match_state != 'confirmed';
CREATE INDEX IF NOT EXISTS idx_media_items_metadata ON media_items USING GIN (metadata jsonb_path_ops);
CREATE INDEX IF NOT EXISTS idx_media_items_search ON media_items USING GIN (search_vector) WHERE search_vector IS NOT NULL;

CREATE TABLE IF NOT EXISTS movies (
    id UUID PRIMARY KEY REFERENCES media_items(id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ GENERATED ALWAYS AS (uuid_extract_timestamp(id)) STORED,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS series (
    id UUID PRIMARY KEY REFERENCES media_items(id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ GENERATED ALWAYS AS (uuid_extract_timestamp(id)) STORED,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),

    status TEXT NOT NULL DEFAULT 'continuing' CHECK (status IN ('continuing', 'ended', 'upcoming', 'canceled'))
);

CREATE TABLE IF NOT EXISTS seasons (
    id UUID PRIMARY KEY REFERENCES media_items(id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ GENERATED ALWAYS AS (uuid_extract_timestamp(id)) STORED,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),

    series_id UUID NOT NULL REFERENCES media_items(id) ON DELETE CASCADE,
    season_number INT NOT NULL,

    UNIQUE(series_id, season_number)
);

CREATE INDEX IF NOT EXISTS idx_seasons_series_id ON seasons (series_id);

CREATE TABLE IF NOT EXISTS episodes (
    id UUID PRIMARY KEY REFERENCES media_items(id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ GENERATED ALWAYS AS (uuid_extract_timestamp(id)) STORED,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),

    series_id UUID NOT NULL REFERENCES media_items(id) ON DELETE CASCADE,
    season_id UUID NOT NULL REFERENCES seasons(id) ON DELETE CASCADE,
    episode_number INT,
    absolute_episode_number INT,

    UNIQUE(season_id, episode_number)
);

CREATE INDEX IF NOT EXISTS idx_episodes_series_id ON episodes (series_id);
CREATE INDEX IF NOT EXISTS idx_episodes_season_id ON episodes (season_id);

CREATE TABLE IF NOT EXISTS media_files (
    id UUID DEFAULT uuidv7() PRIMARY KEY,
    created_at TIMESTAMPTZ GENERATED ALWAYS AS (uuid_extract_timestamp(id)) STORED,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),

    media_item_id UUID NOT NULL REFERENCES media_items(id) ON DELETE CASCADE,

    file_path TEXT NOT NULL,
    file_size BIGINT NOT NULL,
    file_hash TEXT,
    file_modified_at TIMESTAMPTZ,

    container_format TEXT NOT NULL,

    video_codec TEXT,
    video_resolution TEXT,
    video_bitrate INT,
    video_dynamic_range TEXT,
    video_frame_rate NUMERIC(6,3),

    audio_codec TEXT,
    audio_channels INT,
    audio_language TEXT,
    audio_bitrate INT,

    runtime_seconds INT NOT NULL,

    last_scanned_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    is_healthy BOOLEAN NOT NULL DEFAULT true,

    additional_streams JSONB DEFAULT '{}',

    UNIQUE(media_item_id, file_path)
);

CREATE INDEX IF NOT EXISTS idx_media_files_media_item_id ON media_files (media_item_id);
CREATE INDEX IF NOT EXISTS idx_media_files_video_resolution ON media_files (video_resolution);
CREATE INDEX IF NOT EXISTS idx_media_files_file_path ON media_files (file_path);

CREATE TABLE IF NOT EXISTS subtitle_files (
    id UUID DEFAULT uuidv7() PRIMARY KEY,
    created_at TIMESTAMPTZ GENERATED ALWAYS AS (uuid_extract_timestamp(id)) STORED,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),

    media_item_id UUID NOT NULL REFERENCES media_items(id) ON DELETE CASCADE,

    file_path TEXT NOT NULL,
    language TEXT NOT NULL,
    subtitle_type TEXT NOT NULL CHECK (subtitle_type IN ('embedded', 'external', 'fetched')),
    is_forced BOOLEAN NOT NULL DEFAULT false,
    is_hearing_impaired BOOLEAN NOT NULL DEFAULT false,
    source_provider TEXT,

    UNIQUE(media_item_id, file_path)
);

CREATE INDEX IF NOT EXISTS idx_subtitle_files_media_item_id ON subtitle_files (media_item_id);

CREATE TABLE IF NOT EXISTS subtitle_ocr_cache (
    id UUID DEFAULT uuidv7() PRIMARY KEY,
    created_at TIMESTAMPTZ GENERATED ALWAYS AS (uuid_extract_timestamp(id)) STORED,

    media_item_id UUID NOT NULL REFERENCES media_items(id) ON DELETE CASCADE,
    subtitle_stream_index INT NOT NULL,
    source_hash TEXT NOT NULL,

    ocr_engine TEXT NOT NULL CHECK (ocr_engine IN ('paddleocr', 'tesseract')),
    confidence_score NUMERIC(3,2),

    srt_content TEXT NOT NULL,

    UNIQUE(media_item_id, subtitle_stream_index)
);

CREATE INDEX IF NOT EXISTS idx_subtitle_ocr_cache_media_item_id ON subtitle_ocr_cache (media_item_id);

CREATE TABLE IF NOT EXISTS subtitle_sync_data (
    id UUID DEFAULT uuidv7() PRIMARY KEY,
    created_at TIMESTAMPTZ GENERATED ALWAYS AS (uuid_extract_timestamp(id)) STORED,

    media_item_id UUID NOT NULL REFERENCES media_items(id) ON DELETE CASCADE,
    subtitle_file_id UUID NOT NULL REFERENCES subtitle_files(id) ON DELETE CASCADE,

    sync_method TEXT NOT NULL CHECK (sync_method IN ('voice_activity', 'fps_adjust', 'manual')),
    offset_ms INT NOT NULL DEFAULT 0,
    confidence NUMERIC(3,2),

    fps_source NUMERIC(8,4),
    fps_target NUMERIC(8,4),

    UNIQUE(media_item_id, subtitle_file_id, sync_method)
);

CREATE INDEX IF NOT EXISTS idx_subtitle_sync_data_media_item_id ON subtitle_sync_data (media_item_id);

CREATE TABLE IF NOT EXISTS genres (
    id UUID DEFAULT uuidv7() PRIMARY KEY,
    created_at TIMESTAMPTZ GENERATED ALWAYS AS (uuid_extract_timestamp(id)) STORED,

    name TEXT NOT NULL UNIQUE,
    slug TEXT NOT NULL UNIQUE
);

CREATE TABLE IF NOT EXISTS media_genres (
    media_item_id UUID NOT NULL REFERENCES media_items(id) ON DELETE CASCADE,
    genre_id UUID NOT NULL REFERENCES genres(id) ON DELETE CASCADE,

    PRIMARY KEY (media_item_id, genre_id)
);

CREATE TABLE IF NOT EXISTS tags (
    id UUID DEFAULT uuidv7() PRIMARY KEY,
    created_at TIMESTAMPTZ GENERATED ALWAYS AS (uuid_extract_timestamp(id)) STORED,

    name TEXT NOT NULL UNIQUE
);

CREATE TABLE IF NOT EXISTS media_tags (
    media_item_id UUID NOT NULL REFERENCES media_items(id) ON DELETE CASCADE,
    tag_id UUID NOT NULL REFERENCES tags(id) ON DELETE CASCADE,

    PRIMARY KEY (media_item_id, tag_id)
);

CREATE TABLE IF NOT EXISTS people (
    id UUID DEFAULT uuidv7() PRIMARY KEY,
    created_at TIMESTAMPTZ GENERATED ALWAYS AS (uuid_extract_timestamp(id)) STORED,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),

    name TEXT NOT NULL,
    sort_name TEXT NOT NULL,
    tmdb_person_id BIGINT,
    imdb_person_id TEXT,
    trakt_person_id BIGINT,
    image_url TEXT,
    metadata JSONB DEFAULT '{}'
);

CREATE TABLE IF NOT EXISTS media_credits (
    id UUID DEFAULT uuidv7() PRIMARY KEY,
    created_at TIMESTAMPTZ GENERATED ALWAYS AS (uuid_extract_timestamp(id)) STORED,

    media_item_id UUID NOT NULL REFERENCES media_items(id) ON DELETE CASCADE,
    person_id UUID NOT NULL REFERENCES people(id) ON DELETE CASCADE,
    credit_type TEXT NOT NULL CHECK (credit_type IN ('cast', 'crew')),
    role TEXT,
    department TEXT,
    "order" INT NOT NULL DEFAULT 0,

    UNIQUE(media_item_id, person_id, credit_type, role)
);

CREATE INDEX IF NOT EXISTS idx_media_credits_media_item_id ON media_credits (media_item_id);
CREATE INDEX IF NOT EXISTS idx_media_credits_person_id ON media_credits (person_id);

CREATE TABLE IF NOT EXISTS artwork (
    id UUID DEFAULT uuidv7() PRIMARY KEY,
    created_at TIMESTAMPTZ GENERATED ALWAYS AS (uuid_extract_timestamp(id)) STORED,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),

    media_item_id UUID REFERENCES media_items(id) ON DELETE CASCADE,
    artwork_type TEXT NOT NULL CHECK (artwork_type IN ('poster', 'backdrop', 'thumbnail', 'logo', 'banner', 'season_poster')),
    source_url TEXT,
    local_path TEXT,
    width INT,
    height INT,
    language TEXT,
    provider TEXT,
    "order" INT NOT NULL DEFAULT 0,

    UNIQUE(media_item_id, artwork_type, "order")
);

CREATE INDEX IF NOT EXISTS idx_artwork_media_item_id ON artwork (media_item_id);

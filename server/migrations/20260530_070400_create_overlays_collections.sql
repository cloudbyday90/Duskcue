CREATE TABLE IF NOT EXISTS overlay_definitions (
    id UUID DEFAULT uuidv7() PRIMARY KEY,
    created_at TIMESTAMPTZ GENERATED ALWAYS AS (uuid_extract_timestamp(id)) STORED,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),

    name TEXT NOT NULL,
    slug TEXT NOT NULL,

    library_id UUID REFERENCES libraries(id) ON DELETE CASCADE,

    overlay_type TEXT NOT NULL CHECK (overlay_type IN ('image', 'text', 'backdrop')),

    image_path TEXT,
    text_template TEXT,
    font_family TEXT NOT NULL DEFAULT 'Inter',
    font_size INT NOT NULL DEFAULT 63,
    font_color TEXT NOT NULL DEFAULT '#FFFFFF',
    stroke_color TEXT,
    stroke_width INT DEFAULT 0,

    back_color TEXT,
    back_width INT,
    back_height INT,
    back_radius INT DEFAULT 0,
    back_padding INT DEFAULT 0,

    horizontal_offset INT NOT NULL DEFAULT 0,
    horizontal_align TEXT NOT NULL DEFAULT 'left' CHECK (horizontal_align IN ('left', 'center', 'right')),
    vertical_offset INT NOT NULL DEFAULT 0,
    vertical_align TEXT NOT NULL DEFAULT 'top' CHECK (vertical_align IN ('top', 'center', 'bottom')),

    scale_width INT,
    scale_height INT,

    group_name TEXT,
    weight INT NOT NULL DEFAULT 0,

    queue_name TEXT,

    conditions JSONB NOT NULL DEFAULT '{}',
    suppresses TEXT[] NOT NULL DEFAULT '{}',

    applies_to TEXT NOT NULL DEFAULT 'poster' CHECK (applies_to IN ('poster', 'backdrop', 'season_poster', 'episode_thumb')),

    is_enabled BOOLEAN NOT NULL DEFAULT true,
    is_system BOOLEAN NOT NULL DEFAULT false,

    metadata JSONB NOT NULL DEFAULT '{}'
);

CREATE INDEX IF NOT EXISTS idx_overlay_definitions_library ON overlay_definitions (library_id) WHERE library_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_overlay_definitions_group ON overlay_definitions (group_name) WHERE group_name IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_overlay_definitions_queue ON overlay_definitions (queue_name) WHERE queue_name IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_overlay_definitions_enabled ON overlay_definitions (is_enabled) WHERE is_enabled = true;

CREATE TABLE IF NOT EXISTS artwork_overlay_state (
    id UUID DEFAULT uuidv7() PRIMARY KEY,
    created_at TIMESTAMPTZ GENERATED ALWAYS AS (uuid_extract_timestamp(id)) STORED,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),

    media_item_id UUID NOT NULL REFERENCES media_items(id) ON DELETE CASCADE,
    artwork_type TEXT NOT NULL CHECK (artwork_type IN ('poster', 'backdrop', 'season_poster', 'episode_thumb')),

    applied_overlay_ids UUID[] NOT NULL DEFAULT '{}',
    overlay_config_hash TEXT NOT NULL,

    clean_art_path TEXT NOT NULL,
    overlaid_art_path TEXT,

    applied_at TIMESTAMPTZ NOT NULL DEFAULT now(),

    UNIQUE(media_item_id, artwork_type)
);

CREATE INDEX IF NOT EXISTS idx_artwork_overlay_state_media_item ON artwork_overlay_state (media_item_id);
CREATE INDEX IF NOT EXISTS idx_artwork_overlay_state_hash ON artwork_overlay_state (overlay_config_hash);

DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'artwork' AND column_name = 'is_locked') THEN
        ALTER TABLE artwork ADD COLUMN is_locked BOOLEAN NOT NULL DEFAULT false;
    END IF;

    IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'artwork' AND column_name = 'source_type') THEN
        ALTER TABLE artwork ADD COLUMN source_type TEXT
            CHECK (source_type IS NULL OR source_type IN ('tmdb', 'user_upload', 'asset_directory', 'community'));
    END IF;
END $$;

CREATE TABLE IF NOT EXISTS collections (
    id UUID DEFAULT uuidv7() PRIMARY KEY,
    created_at TIMESTAMPTZ GENERATED ALWAYS AS (uuid_extract_timestamp(id)) STORED,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),

    library_id UUID REFERENCES libraries(id) ON DELETE CASCADE,

    name TEXT NOT NULL,
    slug TEXT NOT NULL,
    description TEXT,

    collection_type TEXT NOT NULL DEFAULT 'static' CHECK (collection_type IN ('static', 'dynamic', 'smart')),
    visibility TEXT NOT NULL DEFAULT 'visible' CHECK (visibility IN ('visible', 'hidden', 'featured')),

    is_dynamic BOOLEAN NOT NULL DEFAULT false,
    dynamic_config JSONB,

    is_smart BOOLEAN NOT NULL DEFAULT false,
    smart_filter JSONB,

    poster_artwork_id UUID REFERENCES artwork(id) ON DELETE SET NULL,
    backdrop_artwork_id UUID REFERENCES artwork(id) ON DELETE SET NULL,

    sort_order INT NOT NULL DEFAULT 0,
    sort_by TEXT NOT NULL DEFAULT 'title.asc',

    item_count INT NOT NULL DEFAULT 0,
    total_duration_seconds INT NOT NULL DEFAULT 0,

    sync_mode TEXT NOT NULL DEFAULT 'sync' CHECK (sync_mode IN ('sync', 'append')),
    schedule TEXT NOT NULL DEFAULT '0 6 * * *',
    last_synced_at TIMESTAMPTZ,
    last_sync_result JSONB,

    is_enabled BOOLEAN NOT NULL DEFAULT true,
    is_system BOOLEAN NOT NULL DEFAULT false,

    metadata JSONB NOT NULL DEFAULT '{}'
);

CREATE UNIQUE INDEX IF NOT EXISTS collections_slug_library ON collections (slug, library_id) WHERE library_id IS NOT NULL;
CREATE UNIQUE INDEX IF NOT EXISTS collections_slug_global ON collections (slug) WHERE library_id IS NULL;
CREATE INDEX IF NOT EXISTS idx_collections_library ON collections (library_id) WHERE library_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_collections_type ON collections (collection_type);
CREATE INDEX IF NOT EXISTS idx_collections_visibility ON collections (visibility);
CREATE INDEX IF NOT EXISTS idx_collections_enabled ON collections (is_enabled) WHERE is_enabled = true;
CREATE INDEX IF NOT EXISTS idx_collections_dynamic ON collections (is_dynamic) WHERE is_dynamic = true;
CREATE INDEX IF NOT EXISTS idx_collections_schedule ON collections (last_synced_at) WHERE is_dynamic = true AND is_enabled = true;

CREATE TABLE IF NOT EXISTS collection_items (
    id UUID DEFAULT uuidv7() PRIMARY KEY,
    created_at TIMESTAMPTZ GENERATED ALWAYS AS (uuid_extract_timestamp(id)) STORED,

    collection_id UUID NOT NULL REFERENCES collections(id) ON DELETE CASCADE,
    media_item_id UUID NOT NULL REFERENCES media_items(id) ON DELETE CASCADE,

    position INT NOT NULL DEFAULT 0,

    is_missing BOOLEAN NOT NULL DEFAULT false,
    missing_reason TEXT,

    UNIQUE(collection_id, media_item_id)
);

CREATE INDEX IF NOT EXISTS idx_collection_items_collection ON collection_items (collection_id);
CREATE INDEX IF NOT EXISTS idx_collection_items_media_item ON collection_items (media_item_id);
CREATE INDEX IF NOT EXISTS idx_collection_items_position ON collection_items (collection_id, position);
CREATE INDEX IF NOT EXISTS idx_collection_items_missing ON collection_items (is_missing) WHERE is_missing = true;

CREATE TABLE IF NOT EXISTS collection_templates (
    id UUID DEFAULT uuidv7() PRIMARY KEY,
    created_at TIMESTAMPTZ GENERATED ALWAYS AS (uuid_extract_timestamp(id)) STORED,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),

    name TEXT NOT NULL UNIQUE,
    description TEXT,

    template_type TEXT NOT NULL CHECK (template_type IN ('single', 'multi')),
    template_json JSONB NOT NULL,

    author TEXT,
    source_url TEXT,

    is_system BOOLEAN NOT NULL DEFAULT false,

    metadata JSONB NOT NULL DEFAULT '{}'
);

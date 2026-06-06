CREATE TABLE IF NOT EXISTS migration_sources (
    id UUID DEFAULT uuidv7() PRIMARY KEY,
    created_at TIMESTAMPTZ GENERATED ALWAYS AS (uuid_extract_timestamp(id)) STORED,

    platform TEXT NOT NULL CHECK (platform IN ('plex', 'jellyfin', 'emby')),
    name TEXT NOT NULL,
    connection_config JSONB NOT NULL,

    last_run_at TIMESTAMPTZ,
    status TEXT NOT NULL DEFAULT 'pending'
        CHECK (status IN ('pending', 'discovering', 'matching', 'importing', 'completed', 'failed'))
);

CREATE TABLE IF NOT EXISTS migration_user_mapping (
    id UUID DEFAULT uuidv7() PRIMARY KEY,
    created_at TIMESTAMPTZ GENERATED ALWAYS AS (uuid_extract_timestamp(id)) STORED,

    migration_source_id UUID NOT NULL REFERENCES migration_sources(id) ON DELETE CASCADE,
    source_user_id TEXT NOT NULL,
    source_user_name TEXT NOT NULL,

    platform_user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,

    status TEXT NOT NULL DEFAULT 'pending'
        CHECK (status IN ('pending', 'imported', 'failed')),

    items_matched INT NOT NULL DEFAULT 0,
    items_unmatched INT NOT NULL DEFAULT 0,
    items_imported INT NOT NULL DEFAULT 0,
    items_skipped INT NOT NULL DEFAULT 0,
    imported_at TIMESTAMPTZ,

    UNIQUE(migration_source_id, source_user_id)
);

CREATE TABLE IF NOT EXISTS migration_import_log (
    id UUID DEFAULT uuidv7() PRIMARY KEY,
    created_at TIMESTAMPTZ GENERATED ALWAYS AS (uuid_extract_timestamp(id)) STORED,

    migration_source_id UUID NOT NULL REFERENCES migration_sources(id) ON DELETE CASCADE,
    migration_user_mapping_id UUID NOT NULL REFERENCES migration_user_mapping(id) ON DELETE CASCADE,

    source_item_id TEXT NOT NULL,
    source_item_title TEXT NOT NULL,
    source_item_type TEXT NOT NULL CHECK (source_item_type IN ('movie', 'episode')),
    source_item_year INT,
    source_provider_ids JSONB NOT NULL DEFAULT '{}',

    matched_media_item_id UUID REFERENCES media_items(id) ON DELETE SET NULL,
    match_method TEXT CHECK (match_method IN ('tmdb_id', 'imdb_id', 'tvdb_id', 'title_year', 'unmatched')),

    imported_user_item_data_id UUID REFERENCES user_item_data(id) ON DELETE SET NULL,
    status TEXT NOT NULL CHECK (status IN ('matched', 'unmatched', 'imported', 'skipped', 'error')),
    error_detail TEXT,

    UNIQUE(migration_user_mapping_id, source_item_id)
);

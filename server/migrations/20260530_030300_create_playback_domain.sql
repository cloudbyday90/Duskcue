CREATE TABLE IF NOT EXISTS user_item_data (
    id UUID DEFAULT uuidv7() PRIMARY KEY,
    created_at TIMESTAMPTZ GENERATED ALWAYS AS (uuid_extract_timestamp(id)) STORED,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),

    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    media_item_id UUID NOT NULL REFERENCES media_items(id) ON DELETE CASCADE,

    is_watched BOOLEAN NOT NULL DEFAULT false,
    play_count INT NOT NULL DEFAULT 0,
    last_played_at TIMESTAMPTZ,
    resume_position_ms INT NOT NULL DEFAULT 0,
    last_played_media_file_id UUID REFERENCES media_files(id) ON DELETE SET NULL,

    is_favorite BOOLEAN NOT NULL DEFAULT false,
    user_rating INT CHECK (user_rating BETWEEN 1 AND 10),

    audio_stream_index INT,
    subtitle_stream_index INT,

    UNIQUE(user_id, media_item_id)
) WITH (fillfactor = 85);

CREATE INDEX IF NOT EXISTS idx_user_item_data_user_id ON user_item_data (user_id);
CREATE INDEX IF NOT EXISTS idx_user_item_data_media_item_id ON user_item_data (media_item_id);
CREATE INDEX IF NOT EXISTS idx_user_item_data_continue_watching ON user_item_data (user_id, last_played_at DESC)
    WHERE is_watched = false AND resume_position_ms > 0;
CREATE INDEX IF NOT EXISTS idx_user_item_data_favorites ON user_item_data (user_id, updated_at DESC)
    WHERE is_favorite = true;
CREATE INDEX IF NOT EXISTS idx_user_item_data_watched ON user_item_data (user_id)
    WHERE is_watched = true;
CREATE INDEX IF NOT EXISTS idx_user_item_data_user_rating ON user_item_data (user_id, user_rating DESC)
    WHERE user_rating IS NOT NULL;

CREATE TABLE IF NOT EXISTS bookmarks (
    id UUID DEFAULT uuidv7() PRIMARY KEY,
    created_at TIMESTAMPTZ GENERATED ALWAYS AS (uuid_extract_timestamp(id)) STORED,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),

    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    media_item_id UUID NOT NULL REFERENCES media_items(id) ON DELETE CASCADE,

    position_ms INT NOT NULL CHECK (position_ms >= 0),
    label TEXT NOT NULL,
    description TEXT,

    UNIQUE(user_id, media_item_id, position_ms)
);

CREATE INDEX IF NOT EXISTS idx_bookmarks_user_id ON bookmarks (user_id);
CREATE INDEX IF NOT EXISTS idx_bookmarks_media_item_id ON bookmarks (media_item_id);
CREATE INDEX IF NOT EXISTS idx_bookmarks_user_item ON bookmarks (user_id, media_item_id, position_ms);

CREATE TABLE IF NOT EXISTS playlists (
    id UUID DEFAULT uuidv7() PRIMARY KEY,
    created_at TIMESTAMPTZ GENERATED ALWAYS AS (uuid_extract_timestamp(id)) STORED,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),

    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,

    name TEXT NOT NULL,
    description TEXT,
    visibility TEXT NOT NULL DEFAULT 'private' CHECK (visibility IN ('private', 'shared', 'public')),

    is_smart BOOLEAN NOT NULL DEFAULT false,
    smart_filter JSONB,

    item_count INT NOT NULL DEFAULT 0,
    total_duration_seconds INT NOT NULL DEFAULT 0,

    metadata JSONB NOT NULL DEFAULT '{}',
    deleted_at TIMESTAMPTZ
);

CREATE INDEX IF NOT EXISTS idx_playlists_user_id ON playlists (user_id);
CREATE INDEX IF NOT EXISTS idx_playlists_visibility ON playlists (visibility) WHERE visibility IN ('shared', 'public');
CREATE INDEX IF NOT EXISTS idx_playlists_smart_filter ON playlists USING GIN (smart_filter jsonb_path_ops) WHERE is_smart = true;

CREATE TABLE IF NOT EXISTS playlist_items (
    id UUID DEFAULT uuidv7() PRIMARY KEY,
    created_at TIMESTAMPTZ GENERATED ALWAYS AS (uuid_extract_timestamp(id)) STORED,

    playlist_id UUID NOT NULL REFERENCES playlists(id) ON DELETE CASCADE,
    media_item_id UUID NOT NULL REFERENCES media_items(id) ON DELETE CASCADE,

    position INT NOT NULL,

    UNIQUE(playlist_id, position),
    UNIQUE(playlist_id, media_item_id)
);

CREATE INDEX IF NOT EXISTS idx_playlist_items_playlist_id ON playlist_items (playlist_id);
CREATE INDEX IF NOT EXISTS idx_playlist_items_media_item_id ON playlist_items (media_item_id);

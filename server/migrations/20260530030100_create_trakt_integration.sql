CREATE TABLE IF NOT EXISTS users (
    id UUID DEFAULT uuidv7() PRIMARY KEY,
    created_at TIMESTAMPTZ GENERATED ALWAYS AS (uuid_extract_timestamp(id)) STORED,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),

    username TEXT NOT NULL,
    display_name TEXT NOT NULL,
    email TEXT UNIQUE,
    is_active BOOLEAN NOT NULL DEFAULT true,
    metadata JSONB NOT NULL DEFAULT '{}'
);

CREATE TABLE IF NOT EXISTS trakt_accounts (
    id UUID DEFAULT uuidv7() PRIMARY KEY,
    created_at TIMESTAMPTZ GENERATED ALWAYS AS (uuid_extract_timestamp(id)) STORED,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),

    user_id UUID NOT NULL UNIQUE REFERENCES users(id) ON DELETE CASCADE,

    trakt_username TEXT NOT NULL,
    trakt_user_id BIGINT NOT NULL,

    access_token TEXT NOT NULL,
    refresh_token TEXT NOT NULL,
    token_expires_at TIMESTAMPTZ NOT NULL,
    token_scope TEXT,

    last_full_sync_at TIMESTAMPTZ,
    sync_enabled BOOLEAN NOT NULL DEFAULT true,

    sync_watched BOOLEAN NOT NULL DEFAULT true,
    sync_watchlist BOOLEAN NOT NULL DEFAULT true,
    sync_collection BOOLEAN NOT NULL DEFAULT true,
    sync_ratings BOOLEAN NOT NULL DEFAULT true,

    metadata JSONB NOT NULL DEFAULT '{}'
);

CREATE INDEX IF NOT EXISTS idx_trakt_accounts_user_id ON trakt_accounts (user_id);
CREATE INDEX IF NOT EXISTS idx_trakt_accounts_trakt_user_id ON trakt_accounts (trakt_user_id);

CREATE TABLE IF NOT EXISTS trakt_sync_state (
    id UUID DEFAULT uuidv7() PRIMARY KEY,
    created_at TIMESTAMPTZ GENERATED ALWAYS AS (uuid_extract_timestamp(id)) STORED,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),

    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    media_item_id UUID NOT NULL REFERENCES media_items(id) ON DELETE CASCADE,

    trakt_id BIGINT NOT NULL,
    trakt_history_id BIGINT,

    is_watched BOOLEAN NOT NULL DEFAULT false,
    watched_at TIMESTAMPTZ,
    plays INT NOT NULL DEFAULT 0,

    is_in_watchlist BOOLEAN NOT NULL DEFAULT false,
    watchlist_added_at TIMESTAMPTZ,

    is_in_collection BOOLEAN NOT NULL DEFAULT false,
    collected_at TIMESTAMPTZ,

    rating INT CHECK (rating BETWEEN 1 AND 10),
    rated_at TIMESTAMPTZ,

    sync_error TEXT,
    synced_at TIMESTAMPTZ NOT NULL DEFAULT now(),

    UNIQUE(user_id, media_item_id)
);

CREATE INDEX IF NOT EXISTS idx_trakt_sync_state_user_id ON trakt_sync_state (user_id);
CREATE INDEX IF NOT EXISTS idx_trakt_sync_state_media_item_id ON trakt_sync_state (media_item_id);
CREATE INDEX IF NOT EXISTS idx_trakt_sync_state_trakt_id ON trakt_sync_state (trakt_id);
CREATE INDEX IF NOT EXISTS idx_trakt_sync_state_synced_at ON trakt_sync_state (synced_at DESC);
CREATE INDEX IF NOT EXISTS idx_trakt_sync_state_watched ON trakt_sync_state (user_id) WHERE is_watched = true;
CREATE INDEX IF NOT EXISTS idx_trakt_sync_state_watchlist ON trakt_sync_state (user_id) WHERE is_in_watchlist = true;

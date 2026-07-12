CREATE TABLE IF NOT EXISTS user_profiles (
    id UUID DEFAULT uuidv7() PRIMARY KEY,
    created_at TIMESTAMPTZ GENERATED ALWAYS AS (uuid_extract_timestamp(id)) STORED,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),

    owner_user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    name TEXT NOT NULL CHECK (char_length(trim(name)) BETWEEN 1 AND 80),
    avatar TEXT,
    profile_type TEXT NOT NULL DEFAULT 'standard' CHECK (profile_type IN ('standard', 'kids')),
    is_default BOOLEAN NOT NULL DEFAULT false,

    max_content_rating TEXT NOT NULL DEFAULT 'NC-17' CHECK (max_content_rating IN ('TV-Y', 'TV-Y7', 'G', 'TV-G', 'PG', 'TV-PG', 'PG-13', 'TV-14', 'R', 'TV-MA', 'NC-17')),
    allow_search BOOLEAN NOT NULL DEFAULT true,
    allow_downloads BOOLEAN NOT NULL DEFAULT true,
    allow_external_links BOOLEAN NOT NULL DEFAULT true,
    allow_ambient_channels BOOLEAN NOT NULL DEFAULT true,

    metadata JSONB NOT NULL DEFAULT '{}'
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_user_profiles_default_owner
    ON user_profiles (owner_user_id)
    WHERE is_default = true;
CREATE INDEX IF NOT EXISTS idx_user_profiles_owner ON user_profiles (owner_user_id);
CREATE INDEX IF NOT EXISTS idx_user_profiles_type ON user_profiles (owner_user_id, profile_type);

INSERT INTO user_profiles (id, owner_user_id, name, profile_type, is_default)
SELECT uuidv7(), u.id, u.display_name, 'standard', true
FROM users u
WHERE NOT EXISTS (
    SELECT 1 FROM user_profiles p WHERE p.owner_user_id = u.id AND p.is_default = true
);

CREATE TABLE IF NOT EXISTS profile_library_access (
    id UUID DEFAULT uuidv7() PRIMARY KEY,
    created_at TIMESTAMPTZ GENERATED ALWAYS AS (uuid_extract_timestamp(id)) STORED,
    profile_id UUID NOT NULL REFERENCES user_profiles(id) ON DELETE CASCADE,
    library_id UUID NOT NULL REFERENCES libraries(id) ON DELETE CASCADE,
    UNIQUE(profile_id, library_id)
);

CREATE INDEX IF NOT EXISTS idx_profile_library_access_profile ON profile_library_access (profile_id);
CREATE INDEX IF NOT EXISTS idx_profile_library_access_library ON profile_library_access (library_id);

ALTER TABLE user_sessions
    ADD COLUMN IF NOT EXISTS active_profile_id UUID REFERENCES user_profiles(id) ON DELETE SET NULL;

UPDATE user_sessions s
SET active_profile_id = p.id
FROM user_profiles p
WHERE p.owner_user_id = s.user_id
  AND p.is_default = true
  AND s.active_profile_id IS NULL;

CREATE INDEX IF NOT EXISTS idx_user_sessions_active_profile ON user_sessions (active_profile_id);

ALTER TABLE user_item_data
    ADD COLUMN IF NOT EXISTS profile_id UUID REFERENCES user_profiles(id) ON DELETE CASCADE;

UPDATE user_item_data uid
SET profile_id = p.id
FROM user_profiles p
WHERE p.owner_user_id = uid.user_id
  AND p.is_default = true
  AND uid.profile_id IS NULL;

ALTER TABLE user_item_data
    ALTER COLUMN profile_id SET NOT NULL;
ALTER TABLE user_item_data
    DROP CONSTRAINT IF EXISTS user_item_data_user_id_media_item_id_key;
ALTER TABLE user_item_data
    ADD CONSTRAINT user_item_data_profile_id_media_item_id_key UNIQUE(profile_id, media_item_id);

CREATE INDEX IF NOT EXISTS idx_user_item_data_profile_id ON user_item_data (profile_id);
CREATE INDEX IF NOT EXISTS idx_user_item_data_profile_continue_watching ON user_item_data (profile_id, last_played_at DESC)
    WHERE is_watched = false AND resume_position_ms > 0;
CREATE INDEX IF NOT EXISTS idx_user_item_data_profile_favorites ON user_item_data (profile_id, updated_at DESC)
    WHERE is_favorite = true;

ALTER TABLE play_sessions
    ADD COLUMN IF NOT EXISTS profile_id UUID REFERENCES user_profiles(id) ON DELETE SET NULL;
ALTER TABLE play_sessions
    ADD COLUMN IF NOT EXISTS playback_mode TEXT NOT NULL DEFAULT 'interactive' CHECK (playback_mode IN ('interactive', 'ambient'));

UPDATE play_sessions ps
SET profile_id = p.id
FROM user_profiles p
WHERE p.owner_user_id = ps.user_id
  AND p.is_default = true
  AND ps.profile_id IS NULL;

CREATE INDEX IF NOT EXISTS idx_play_sessions_profile_id ON play_sessions (profile_id);
CREATE INDEX IF NOT EXISTS idx_play_sessions_playback_mode ON play_sessions (playback_mode);

CREATE TABLE IF NOT EXISTS ambient_channels (
    id UUID DEFAULT uuidv7() PRIMARY KEY,
    created_at TIMESTAMPTZ GENERATED ALWAYS AS (uuid_extract_timestamp(id)) STORED,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),

    owner_user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    name TEXT NOT NULL CHECK (char_length(trim(name)) BETWEEN 1 AND 120),
    description TEXT,
    audience TEXT NOT NULL CHECK (audience IN ('standard', 'kids')),
    is_enabled BOOLEAN NOT NULL DEFAULT true,

    metadata JSONB NOT NULL DEFAULT '{}'
);

CREATE INDEX IF NOT EXISTS idx_ambient_channels_owner ON ambient_channels (owner_user_id, audience);
CREATE INDEX IF NOT EXISTS idx_ambient_channels_enabled ON ambient_channels (owner_user_id, audience)
    WHERE is_enabled = true;

CREATE TABLE IF NOT EXISTS ambient_channel_items (
    id UUID DEFAULT uuidv7() PRIMARY KEY,
    created_at TIMESTAMPTZ GENERATED ALWAYS AS (uuid_extract_timestamp(id)) STORED,

    channel_id UUID NOT NULL REFERENCES ambient_channels(id) ON DELETE CASCADE,
    media_item_id UUID NOT NULL REFERENCES media_items(id) ON DELETE CASCADE,
    position INT NOT NULL CHECK (position >= 0),

    UNIQUE(channel_id, position),
    UNIQUE(channel_id, media_item_id)
);

CREATE INDEX IF NOT EXISTS idx_ambient_channel_items_channel ON ambient_channel_items (channel_id, position);
CREATE INDEX IF NOT EXISTS idx_ambient_channel_items_media_item ON ambient_channel_items (media_item_id);

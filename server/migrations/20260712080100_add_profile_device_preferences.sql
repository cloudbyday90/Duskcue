ALTER TABLE user_profiles
    ADD CONSTRAINT user_profiles_owner_id_key UNIQUE (owner_user_id, id);

CREATE TABLE IF NOT EXISTS profile_device_preferences (
    id UUID DEFAULT uuidv7() PRIMARY KEY,
    created_at TIMESTAMPTZ GENERATED ALWAYS AS (uuid_extract_timestamp(id)) STORED,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),

    owner_user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    device_id TEXT NOT NULL CHECK (char_length(btrim(device_id)) BETWEEN 1 AND 200),
    profile_id UUID NOT NULL,

    CONSTRAINT profile_device_preferences_owner_profile_fkey
        FOREIGN KEY (owner_user_id, profile_id)
        REFERENCES user_profiles (owner_user_id, id)
        ON DELETE CASCADE,
    CONSTRAINT profile_device_preferences_owner_device_key
        UNIQUE (owner_user_id, device_id)
);

CREATE INDEX IF NOT EXISTS idx_profile_device_preferences_profile
    ON profile_device_preferences (profile_id);

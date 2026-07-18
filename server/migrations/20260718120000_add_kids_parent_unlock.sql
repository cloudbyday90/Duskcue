ALTER TABLE user_profiles
    ADD COLUMN IF NOT EXISTS parent_pin_hash TEXT,
    ADD COLUMN IF NOT EXISTS parent_pin_failed_attempts SMALLINT NOT NULL DEFAULT 0
        CHECK (parent_pin_failed_attempts >= 0),
    ADD COLUMN IF NOT EXISTS parent_pin_locked_until TIMESTAMPTZ;

ALTER TABLE user_sessions
    ADD COLUMN IF NOT EXISTS parent_unlock_profile_id UUID REFERENCES user_profiles(id) ON DELETE SET NULL,
    ADD COLUMN IF NOT EXISTS parent_unlock_expires_at TIMESTAMPTZ;

CREATE INDEX IF NOT EXISTS idx_user_sessions_parent_unlock_profile
    ON user_sessions (parent_unlock_profile_id)
    WHERE parent_unlock_profile_id IS NOT NULL;

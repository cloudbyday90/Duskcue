CREATE TABLE IF NOT EXISTS user_push_devices (
    id UUID DEFAULT uuidv7() PRIMARY KEY,
    created_at TIMESTAMPTZ GENERATED ALWAYS AS (uuid_extract_timestamp(id)) STORED,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),

    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,

    provider TEXT NOT NULL CHECK (provider IN ('fcm', 'apns', 'unifiedpush')),

    token TEXT NOT NULL,

    device_name TEXT,
    platform TEXT,
    app_version TEXT,

    last_seen_at TIMESTAMPTZ,
    is_active BOOLEAN NOT NULL DEFAULT true,

    invalidated_at TIMESTAMPTZ,

    UNIQUE(user_id, provider, token)
);

CREATE INDEX IF NOT EXISTS idx_user_push_devices_user
    ON user_push_devices (user_id) WHERE is_active = true;

CREATE TABLE IF NOT EXISTS user_location_history (
    id UUID DEFAULT uuidv7() PRIMARY KEY,
    created_at TIMESTAMPTZ GENERATED ALWAYS AS (uuid_extract_timestamp(id)) STORED,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),

    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    country_code TEXT NOT NULL,

    first_seen_at TIMESTAMPTZ NOT NULL,
    last_seen_at TIMESTAMPTZ NOT NULL,
    session_count INT NOT NULL DEFAULT 1,

    CONSTRAINT uq_user_location_country UNIQUE (user_id, country_code)
);

CREATE INDEX IF NOT EXISTS idx_user_location_history_user_id ON user_location_history (user_id);
CREATE INDEX IF NOT EXISTS idx_user_location_history_last_seen ON user_location_history (last_seen_at DESC);

ALTER TABLE user_item_data SET (
    autovacuum_vacuum_scale_factor = 0.02,
    autovacuum_analyze_scale_factor = 0.01,
    autovacuum_vacuum_cost_delay = 1,
    autovacuum_vacuum_cost_limit = 2000
);

ALTER TABLE user_sessions SET (
    autovacuum_vacuum_scale_factor = 0.05,
    autovacuum_analyze_scale_factor = 0.02
);

ALTER TABLE server_config SET (
    autovacuum_vacuum_scale_factor = 0.0,
    autovacuum_vacuum_threshold = 1,
    autovacuum_analyze_scale_factor = 0.0,
    autovacuum_analyze_threshold = 1
);

ALTER TABLE scheduled_tasks SET (
    autovacuum_vacuum_scale_factor = 0.05,
    autovacuum_analyze_scale_factor = 0.02
);

ALTER TABLE users SET (
    autovacuum_vacuum_scale_factor = 0.05,
    autovacuum_analyze_scale_factor = 0.02
);

ALTER TABLE media_items SET (
    autovacuum_vacuum_scale_factor = 0.05,
    autovacuum_analyze_scale_factor = 0.02
);

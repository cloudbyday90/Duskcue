ALTER TABLE play_sessions
    ADD COLUMN IF NOT EXISTS ambient_channel_id UUID REFERENCES ambient_channels(id) ON DELETE SET NULL;

UPDATE play_sessions ps
SET ambient_channel_id = c.id
FROM ambient_channels c
WHERE ps.playback_mode = 'ambient'
  AND ps.ambient_channel_id IS NULL
  AND jsonb_typeof(ps.metadata -> 'ambient_channel_id') = 'string'
  AND ps.metadata ->> 'ambient_channel_id' ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[1-5][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$'
  AND c.id = (ps.metadata ->> 'ambient_channel_id')::uuid;

CREATE INDEX IF NOT EXISTS idx_play_sessions_ambient_channel_id
    ON play_sessions (ambient_channel_id, started_at DESC)
    WHERE playback_mode = 'ambient' AND ambient_channel_id IS NOT NULL;

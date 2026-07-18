-- Duskcue — Self-hosted media streaming server
-- Copyright (C) 2026-2026 Duskcue Contributors
--
-- This program is free software: you can redistribute it and/or modify
-- it under the terms of the GNU Affero General Public License as published by
-- the Free Software Foundation, either version 3 of the License, or
-- (at your option) any later version.
--
-- This program is distributed in the hope that it will be useful,
-- but WITHOUT ANY WARRANTY; without even the implied warranty of
-- MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
-- GNU Affero General Public License for more details.
--
-- You should have received a copy of the GNU Affero General Public License
-- along with this program. If not, see <https://www.gnu.org/licenses/>.

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

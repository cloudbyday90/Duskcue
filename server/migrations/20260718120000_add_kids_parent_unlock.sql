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

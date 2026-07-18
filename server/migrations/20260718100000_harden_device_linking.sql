-- Duskcue — Self-hosted media streaming server
-- Copyright (C) 2026-2026 Duskcue Contributors
--
-- This program is free software: licensed under AGPL-3.0
-- See LICENSE file for details.

ALTER TABLE device_linking_codes
    ADD COLUMN IF NOT EXISTS is_denied BOOLEAN NOT NULL DEFAULT false,
    ADD COLUMN IF NOT EXISTS denied_by_user_id UUID REFERENCES users(id) ON DELETE SET NULL,
    ADD COLUMN IF NOT EXISTS denied_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS last_polled_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS poll_interval_seconds INTEGER NOT NULL DEFAULT 5
        CHECK (poll_interval_seconds > 0);

CREATE INDEX IF NOT EXISTS idx_device_linking_pending_expires
    ON device_linking_codes (expires_at)
    WHERE is_approved = false AND is_denied = false;

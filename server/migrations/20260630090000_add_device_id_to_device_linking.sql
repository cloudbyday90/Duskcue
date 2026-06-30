-- Duskcue — Self-hosted media streaming server
-- Copyright (C) 2026 Duskcue Contributors
--
-- This program is free software: licensed under AGPL-3.0
-- See LICENSE file for details.

ALTER TABLE device_linking_codes
    ADD COLUMN IF NOT EXISTS device_id TEXT;

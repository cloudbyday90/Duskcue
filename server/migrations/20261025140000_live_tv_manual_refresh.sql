-- Duskcue — Self-hosted media streaming server
-- Copyright (C) 2026-2026 Duskcue Contributors
-- Licensed under AGPL-3.0. See LICENSE for details.

CREATE TABLE live_tv_refresh_control (
    id UUID PRIMARY KEY DEFAULT uuidv7() CHECK (uuid_extract_version(id)=7),
    singleton BOOLEAN NOT NULL DEFAULT true UNIQUE CHECK (singleton),
    revision BIGINT NOT NULL DEFAULT 1 CHECK (revision>0),
    enabled BOOLEAN NOT NULL DEFAULT false
);
INSERT INTO live_tv_refresh_control DEFAULT VALUES;

-- Duskcue — Self-hosted media streaming server
-- Copyright (C) 2026-2026 Duskcue Contributors
-- Licensed under AGPL-3.0. See LICENSE for details.

CREATE TABLE live_tv_assignments (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    household_user_id UUID NOT NULL UNIQUE REFERENCES users(id) ON DELETE CASCADE,
    revision BIGINT NOT NULL DEFAULT 1 CHECK (revision > 0),
    playlist_secret TEXT NOT NULL CHECK (octet_length(playlist_secret) BETWEEN 58 AND 5518 AND playlist_secret LIKE 'duskcue-secret:v1:%'),
    guide_secret TEXT NOT NULL CHECK (octet_length(guide_secret) BETWEEN 58 AND 5518 AND guide_secret LIKE 'duskcue-secret:v1:%'),
    playlist_transport TEXT NOT NULL CHECK (playlist_transport IN ('http','https')),
    guide_transport TEXT NOT NULL CHECK (guide_transport IN ('http','https')),
    enabled BOOLEAN NOT NULL DEFAULT false,
    max_live_streams INTEGER CHECK (max_live_streams > 0),
    allow_owner_profile_access BOOLEAN NOT NULL DEFAULT false,
    allow_owner_live_limit BOOLEAN NOT NULL DEFAULT false,
    owner_max_live_streams INTEGER CHECK (owner_max_live_streams > 0),
    created_at TIMESTAMPTZ GENERATED ALWAYS AS (uuid_extract_timestamp(id)) STORED,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE(household_user_id,id)
);

CREATE TABLE live_tv_profile_access (
    household_user_id UUID NOT NULL,
    assignment_id UUID NOT NULL,
    profile_id UUID NOT NULL,
    enabled BOOLEAN NOT NULL DEFAULT false,
    PRIMARY KEY(assignment_id,profile_id),
    FOREIGN KEY(household_user_id,assignment_id) REFERENCES live_tv_assignments(household_user_id,id) ON DELETE CASCADE,
    FOREIGN KEY(household_user_id,profile_id) REFERENCES user_profiles(owner_user_id,id) ON DELETE CASCADE
);
CREATE INDEX idx_live_tv_profile_access_profile ON live_tv_profile_access(profile_id);

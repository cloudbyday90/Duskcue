-- Duskcue — Self-hosted media streaming server
-- Copyright (C) 2026-2026 Duskcue Contributors
-- Licensed under AGPL-3.0. See LICENSE for details.

CREATE TABLE live_tv_catalog_snapshots (
    id UUID PRIMARY KEY CHECK (uuid_extract_version(id) = 7),
    household_user_id UUID NOT NULL,
    assignment_id UUID NOT NULL,
    source_generation BIGINT NOT NULL CHECK (source_generation > 0),
    attempt_id UUID NOT NULL CHECK (uuid_extract_version(attempt_id) = 7),
    attempt_sequence BIGINT NOT NULL CHECK (attempt_sequence > 0),
    channel_count INTEGER NOT NULL CHECK (channel_count BETWEEN 0 AND 10000),
    guide_count INTEGER NOT NULL CHECK (guide_count BETWEEN 0 AND 10000),
    programme_count INTEGER NOT NULL CHECK (programme_count BETWEEN 0 AND 250000),
    imported_at TIMESTAMPTZ NOT NULL,
    manifest TEXT NOT NULL CHECK (octet_length(manifest) BETWEEN 58 AND 5518 AND manifest LIKE 'duskcue-secret:v1:%'),
    UNIQUE (assignment_id) DEFERRABLE INITIALLY DEFERRED,
    UNIQUE (household_user_id,assignment_id,id),
    FOREIGN KEY (household_user_id,assignment_id) REFERENCES live_tv_assignments(household_user_id,id) ON DELETE CASCADE
);

CREATE TABLE live_tv_import_state (
    household_user_id UUID NOT NULL,
    assignment_id UUID PRIMARY KEY CHECK (uuid_extract_version(assignment_id) = 7),
    attempt_id UUID NOT NULL CHECK (uuid_extract_version(attempt_id) = 7),
    attempt_sequence BIGINT NOT NULL CHECK (attempt_sequence > 0),
    state TEXT NOT NULL CHECK (state IN ('running','succeeded','failed','indeterminate')),
    category TEXT CHECK (category IN ('authority','capacity','cancelled','deadline','network','format','limit','storage','indeterminate','cleanup')),
    attempted_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
    finished_at TIMESTAMPTZ,
    current_snapshot_id UUID,
    FOREIGN KEY (household_user_id,assignment_id) REFERENCES live_tv_assignments(household_user_id,id) ON DELETE CASCADE,
    FOREIGN KEY (household_user_id,assignment_id,current_snapshot_id) REFERENCES live_tv_catalog_snapshots(household_user_id,assignment_id,id) DEFERRABLE INITIALLY DEFERRED
);

CREATE TABLE live_tv_catalog_guides (
    id UUID PRIMARY KEY CHECK (uuid_extract_version(id) = 7),
    household_user_id UUID NOT NULL,
    assignment_id UUID NOT NULL,
    snapshot_id UUID NOT NULL,
    metadata TEXT NOT NULL CHECK (octet_length(metadata) BETWEEN 58 AND 5518 AND metadata LIKE 'duskcue-secret:v1:%'),
    UNIQUE (household_user_id,assignment_id,snapshot_id,id),
    FOREIGN KEY (household_user_id,assignment_id,snapshot_id) REFERENCES live_tv_catalog_snapshots(household_user_id,assignment_id,id) ON DELETE CASCADE
);

CREATE TABLE live_tv_catalog_channels (
    id UUID PRIMARY KEY CHECK (uuid_extract_version(id) = 7),
    household_user_id UUID NOT NULL,
    assignment_id UUID NOT NULL,
    snapshot_id UUID NOT NULL,
    channel_key BYTEA NOT NULL CHECK (octet_length(channel_key)=32),
    guide_id UUID,
    metadata TEXT NOT NULL CHECK (octet_length(metadata) BETWEEN 58 AND 5518 AND metadata LIKE 'duskcue-secret:v1:%'),
    stream TEXT NOT NULL CHECK (octet_length(stream) BETWEEN 58 AND 5518 AND stream LIKE 'duskcue-secret:v1:%'),
    logo TEXT CHECK (logo IS NULL OR (octet_length(logo) BETWEEN 58 AND 5518 AND logo LIKE 'duskcue-secret:v1:%')),
    UNIQUE (snapshot_id,channel_key),
    FOREIGN KEY (household_user_id,assignment_id,snapshot_id) REFERENCES live_tv_catalog_snapshots(household_user_id,assignment_id,id) ON DELETE CASCADE,
    FOREIGN KEY (household_user_id,assignment_id,snapshot_id,guide_id) REFERENCES live_tv_catalog_guides(household_user_id,assignment_id,snapshot_id,id)
);
CREATE INDEX live_tv_catalog_channels_guide ON live_tv_catalog_channels(guide_id);

CREATE TABLE live_tv_catalog_programmes (
    id UUID PRIMARY KEY CHECK (uuid_extract_version(id) = 7),
    household_user_id UUID NOT NULL,
    assignment_id UUID NOT NULL,
    snapshot_id UUID NOT NULL,
    guide_id UUID NOT NULL,
    starts_at TIMESTAMPTZ,
    stops_at TIMESTAMPTZ,
    quality_flags INTEGER NOT NULL CHECK (quality_flags BETWEEN 0 AND 15),
    metadata TEXT NOT NULL CHECK (octet_length(metadata) BETWEEN 58 AND 5518 AND metadata LIKE 'duskcue-secret:v1:%'),
    CHECK (starts_at IS NULL OR stops_at IS NULL OR starts_at < stops_at),
    FOREIGN KEY (household_user_id,assignment_id,snapshot_id) REFERENCES live_tv_catalog_snapshots(household_user_id,assignment_id,id) ON DELETE CASCADE,
    FOREIGN KEY (household_user_id,assignment_id,snapshot_id,guide_id) REFERENCES live_tv_catalog_guides(household_user_id,assignment_id,snapshot_id,id)
);
CREATE INDEX live_tv_catalog_programmes_guide_time ON live_tv_catalog_programmes(guide_id,starts_at);

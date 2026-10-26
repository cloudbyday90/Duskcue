-- Duskcue — Self-hosted media streaming server
-- Copyright (C) 2026-2026 Duskcue Contributors
-- Licensed under AGPL-3.0. See LICENSE for details.

CREATE TABLE playback_instances (
    id UUID PRIMARY KEY DEFAULT uuidv7() CHECK (uuid_extract_version(id)=7),
    revision BIGINT NOT NULL DEFAULT 1 CHECK (revision > 0),
    fenced BOOLEAN NOT NULL DEFAULT false,
    created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
    fenced_at TIMESTAMPTZ,
    reconciled_at TIMESTAMPTZ
);

CREATE TABLE playback_reservations (
    id UUID PRIMARY KEY CHECK (uuid_extract_version(id)=7),
    instance_id UUID NOT NULL REFERENCES playback_instances(id),
    household_id UUID NOT NULL,
    fingerprint TEXT NOT NULL CHECK (octet_length(fingerprint) BETWEEN 1 AND 2048),
    kind TEXT NOT NULL CHECK (kind IN ('vod','live')),
    assignment_id UUID,
    source_generation BIGINT,
    channel_key BYTEA,
    state TEXT NOT NULL DEFAULT 'held' CHECK (state IN ('held','retiring','released')),
    deadline TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp()+interval '60 seconds',
    created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
    released_at TIMESTAMPTZ,
    CHECK ((kind='vod' AND assignment_id IS NULL AND source_generation IS NULL AND channel_key IS NULL)
        OR (kind='live' AND assignment_id IS NOT NULL AND source_generation IS NOT NULL
            AND source_generation>0 AND channel_key IS NOT NULL AND octet_length(channel_key)=32)),
    CHECK ((state='released')=(released_at IS NOT NULL))
);

CREATE INDEX playback_reservations_count ON playback_reservations(household_id,kind) WHERE state<>'released';
CREATE INDEX playback_reservations_owner ON playback_reservations(instance_id,id) WHERE state<>'released';

CREATE TABLE playback_operator_events (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    instance_id UUID NOT NULL REFERENCES playback_instances(id),
    revision BIGINT NOT NULL,
    operation TEXT NOT NULL CHECK (operation IN ('fence','reconcile')),
    occurred_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp()
);

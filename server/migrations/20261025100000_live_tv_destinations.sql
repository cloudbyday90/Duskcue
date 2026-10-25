-- Duskcue — Self-hosted media streaming server
-- Copyright (C) 2026-2026 Duskcue Contributors
-- Licensed under AGPL-3.0. See LICENSE for details.

ALTER TABLE live_tv_assignments
    ADD COLUMN source_generation BIGINT NOT NULL DEFAULT 1 CHECK (source_generation > 0),
    ADD COLUMN playlist_destination TEXT,
    ADD COLUMN guide_destination TEXT,
    ADD COLUMN playlist_address_count SMALLINT NOT NULL DEFAULT 0,
    ADD COLUMN guide_address_count SMALLINT NOT NULL DEFAULT 0,
    ADD CONSTRAINT playlist_destination_valid CHECK (
        (playlist_destination IS NULL AND playlist_address_count = 0) OR
        (playlist_destination IS NOT NULL AND playlist_address_count BETWEEN 1 AND 8
         AND octet_length(playlist_destination) BETWEEN 58 AND 5518 AND playlist_destination LIKE 'duskcue-secret:v1:%')),
    ADD CONSTRAINT guide_destination_valid CHECK (
        (guide_destination IS NULL AND guide_address_count = 0) OR
        (guide_destination IS NOT NULL AND guide_address_count BETWEEN 1 AND 8
         AND octet_length(guide_destination) BETWEEN 58 AND 5518 AND guide_destination LIKE 'duskcue-secret:v1:%'));

CREATE FUNCTION live_tv_account_source_changed() RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    IF OLD.status IS DISTINCT FROM NEW.status OR OLD.deleted_at IS DISTINCT FROM NEW.deleted_at THEN
        UPDATE live_tv_assignments
        SET source_generation = source_generation + 1, revision = revision + 1, updated_at = clock_timestamp()
        WHERE household_user_id = NEW.id;
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER live_tv_account_source_changed
AFTER UPDATE OF status, deleted_at ON users
FOR EACH ROW EXECUTE FUNCTION live_tv_account_source_changed();

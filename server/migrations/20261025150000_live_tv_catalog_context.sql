-- Duskcue — Self-hosted media streaming server
-- Copyright (C) 2026-2026 Duskcue Contributors
-- Licensed under AGPL-3.0. See LICENSE for details.

ALTER TABLE user_sessions ADD COLUMN profile_context_revision BIGINT NOT NULL DEFAULT 1 CHECK (profile_context_revision > 0);

CREATE FUNCTION advance_session_profile_context() RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    IF OLD.active_profile_id IS DISTINCT FROM NEW.active_profile_id
       OR OLD.profile_selection_required IS DISTINCT FROM NEW.profile_selection_required THEN
        NEW.profile_context_revision := OLD.profile_context_revision + 1;
    ELSE
        NEW.profile_context_revision := OLD.profile_context_revision;
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER session_profile_context BEFORE UPDATE ON user_sessions
    FOR EACH ROW EXECUTE FUNCTION advance_session_profile_context();

-- Duskcue — Self-hosted media streaming server
-- Copyright (C) 2026-2026 Duskcue Contributors
-- Licensed under AGPL-3.0. See LICENSE for details.

CREATE TABLE household_owner_state (
    user_id UUID PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    enabled BOOLEAN NOT NULL DEFAULT false,
    revision BIGINT NOT NULL DEFAULT 1 CHECK (revision > 0),
    password_fingerprint TEXT,
    failed_attempts SMALLINT NOT NULL DEFAULT 0 CHECK (failed_attempts BETWEEN 0 AND 5),
    locked_until TIMESTAMPTZ
);

ALTER TABLE user_passkeys ADD COLUMN owner_trusted BOOLEAN NOT NULL DEFAULT false;
ALTER TABLE user_sessions ADD COLUMN credential_version BIGINT NOT NULL DEFAULT 1 CHECK (credential_version > 0);

CREATE TABLE household_owner_handoffs (
    user_id UUID PRIMARY KEY REFERENCES household_owner_state(user_id) ON DELETE CASCADE,
    id UUID NOT NULL DEFAULT uuidv7() UNIQUE,
    token_hash TEXT NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL
);

CREATE TABLE household_owner_grants (
    session_id UUID PRIMARY KEY REFERENCES user_sessions(id) ON DELETE CASCADE,
    credential_version BIGINT NOT NULL,
    user_id UUID NOT NULL REFERENCES household_owner_state(user_id) ON DELETE CASCADE,
    profile_id UUID NOT NULL REFERENCES user_profiles(id) ON DELETE CASCADE,
    revision BIGINT NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL
);
CREATE INDEX idx_household_owner_grants_user ON household_owner_grants(user_id);

CREATE TABLE household_owner_challenges (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    user_id UUID NOT NULL REFERENCES household_owner_state(user_id) ON DELETE CASCADE,
    session_id UUID NOT NULL REFERENCES user_sessions(id) ON DELETE CASCADE,
    credential_version BIGINT NOT NULL,
    profile_id UUID NOT NULL REFERENCES user_profiles(id) ON DELETE CASCADE,
    revision BIGINT NOT NULL,
    handoff_id UUID REFERENCES household_owner_handoffs(id) ON DELETE CASCADE,
    purpose TEXT NOT NULL CHECK (purpose IN ('unlock','enroll','register')),
    factor_id UUID REFERENCES user_passkeys(id) ON DELETE CASCADE,
    state JSONB NOT NULL CHECK (octet_length(state::text) <= 65536),
    name TEXT CHECK (char_length(name) BETWEEN 1 AND 200),
    expires_at TIMESTAMPTZ NOT NULL
);
CREATE INDEX idx_household_owner_challenges_user ON household_owner_challenges(user_id);
CREATE INDEX idx_household_owner_challenges_expiry ON household_owner_challenges(expires_at);

CREATE FUNCTION invalidate_household_profile_grants() RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    IF OLD.active_profile_id IS DISTINCT FROM NEW.active_profile_id THEN
        DELETE FROM household_owner_grants WHERE session_id = NEW.id;
        DELETE FROM household_owner_challenges WHERE session_id = NEW.id;
    END IF;
    RETURN NEW;
END;
$$;
CREATE TRIGGER household_profile_grants AFTER UPDATE OF active_profile_id ON user_sessions
    FOR EACH ROW EXECUTE FUNCTION invalidate_household_profile_grants();

CREATE FUNCTION invalidate_household_password_grants() RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    IF OLD.password_hash IS DISTINCT FROM NEW.password_hash OR OLD.status IS DISTINCT FROM NEW.status OR OLD.deleted_at IS DISTINCT FROM NEW.deleted_at THEN
        UPDATE household_owner_state SET revision = revision + 1, password_fingerprint = CASE WHEN OLD.password_hash IS DISTINCT FROM NEW.password_hash THEN NULL ELSE password_fingerprint END WHERE user_id = NEW.id;
        DELETE FROM household_owner_grants WHERE user_id = NEW.id;
        DELETE FROM household_owner_challenges WHERE user_id = NEW.id;
    END IF;
    RETURN NEW;
END;
$$;
CREATE TRIGGER household_password_grants AFTER UPDATE OF password_hash,status,deleted_at ON users
    FOR EACH ROW EXECUTE FUNCTION invalidate_household_password_grants();

-- Duskcue — Self-hosted media streaming server
-- Copyright (C) 2026-2026 Duskcue Contributors
-- Licensed under AGPL-3.0. See LICENSE for details.

CREATE TABLE playback_vod_commands (
    reservation_id UUID PRIMARY KEY REFERENCES playback_reservations(id) ON DELETE CASCADE,
    intent_hash BYTEA NOT NULL CHECK (octet_length(intent_hash)=32),
    policy_hash BYTEA NOT NULL CHECK (octet_length(policy_hash)=32),
    source_hash BYTEA NOT NULL CHECK (octet_length(source_hash)=32),
    delivery_mode TEXT NOT NULL CHECK (delivery_mode IN ('direct_play','direct_stream','transcode')),
    encodes BOOLEAN NOT NULL,
    source_4k BOOLEAN NOT NULL,
    ambient_channel_id UUID,
    ambient_revision TIMESTAMPTZ,
    state TEXT NOT NULL DEFAULT 'preparing' CHECK (state IN ('preparing','ready','stopping','failed')),
    history_id UUID,
    history_started_at TIMESTAMPTZ,
    generation_id UUID,
    result JSONB CHECK (octet_length(result::text)<=32768),
    resources_completed BOOLEAN NOT NULL DEFAULT false,
    cleanup_uncertain BOOLEAN NOT NULL DEFAULT false,
    CHECK ((history_id IS NULL)=(history_started_at IS NULL)),
    CHECK ((ambient_channel_id IS NULL)=(ambient_revision IS NULL)),
    CHECK (state<>'ready' OR (history_id IS NOT NULL AND result IS NOT NULL)),
    CHECK (delivery_mode<>'direct_play' OR (NOT encodes AND generation_id IS NULL))
);

CREATE UNIQUE INDEX playback_vod_history ON playback_vod_commands(history_id,history_started_at) WHERE history_id IS NOT NULL;
CREATE UNIQUE INDEX playback_vod_generation ON playback_vod_commands(generation_id) WHERE generation_id IS NOT NULL;
CREATE INDEX playback_vod_ambient ON playback_vod_commands(ambient_channel_id,reservation_id) WHERE ambient_channel_id IS NOT NULL;

CREATE FUNCTION playback_vod_command_guard() RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    IF TG_OP='DELETE' THEN
        IF EXISTS(SELECT 1 FROM playback_reservations WHERE id=OLD.reservation_id) THEN
            RAISE EXCEPTION 'VOD command retention is owned by its reservation' USING ERRCODE='23514';
        END IF;
        RETURN OLD;
    END IF;
    IF TG_OP='UPDATE' THEN
        IF (NEW.reservation_id,NEW.intent_hash,NEW.policy_hash,NEW.source_hash,NEW.delivery_mode,NEW.encodes,NEW.source_4k,NEW.ambient_channel_id,NEW.ambient_revision)
            IS DISTINCT FROM (OLD.reservation_id,OLD.intent_hash,OLD.policy_hash,OLD.source_hash,OLD.delivery_mode,OLD.encodes,OLD.source_4k,OLD.ambient_channel_id,OLD.ambient_revision)
            OR (OLD.resources_completed AND NOT NEW.resources_completed)
            OR (OLD.cleanup_uncertain AND NOT NEW.cleanup_uncertain)
            OR (OLD.history_id IS NOT NULL AND (NEW.history_id,NEW.history_started_at,NEW.generation_id,NEW.result)
                IS DISTINCT FROM (OLD.history_id,OLD.history_started_at,OLD.generation_id,OLD.result))
            OR (OLD.state<>'preparing' AND NEW.state='preparing')
            OR (OLD.state IN ('stopping','failed') AND NEW.state NOT IN (OLD.state,'failed')) THEN
            RAISE EXCEPTION 'VOD command attribution is immutable' USING ERRCODE='23514';
        END IF;
    ELSE
        IF NOT EXISTS(SELECT 1 FROM playback_reservations WHERE id=NEW.reservation_id AND kind='vod' AND state='held') THEN
            RAISE EXCEPTION 'VOD command requires an admitted reservation' USING ERRCODE='23514';
        END IF;
    END IF;
    RETURN NEW;
END;
$$;
CREATE TRIGGER playback_vod_command_guard BEFORE INSERT OR UPDATE OR DELETE ON playback_vod_commands
FOR EACH ROW EXECUTE FUNCTION playback_vod_command_guard();

CREATE FUNCTION playback_vod_no_truncate() RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    RAISE EXCEPTION 'VOD command retention is owned by its reservations' USING ERRCODE='23514';
END;
$$;
CREATE TRIGGER playback_vod_no_truncate BEFORE TRUNCATE ON playback_vod_commands
FOR EACH STATEMENT EXECUTE FUNCTION playback_vod_no_truncate();

CREATE FUNCTION playback_vod_policy(household UUID) RETURNS JSONB LANGUAGE SQL STABLE AS $$
    SELECT jsonb_build_object(
        'total',COALESCE(u.max_streams,p.max_streams),
        'transcode',COALESCE(u.max_transcode_streams,p.max_transcode_streams),
        'bandwidth',COALESCE(u.bandwidth_limit_bps,p.bandwidth_limit_bps),
        'allow_direct_play',p.allow_direct_play,'allow_direct_stream',p.allow_direct_stream,
        'allow_transcode',p.allow_transcode,'resolution',p.max_transcode_resolution,
        'allow_transcode_4k',p.allow_transcode_4k,'require_direct_play_4k',p.require_direct_play_4k,
        'supported',p.allowed_ip_ranges='[]'::jsonb AND p.blocked_ip_ranges='[]'::jsonb
            AND p.auto_terminate_paused_minutes IS NULL,
        'valid',COALESCE(u.max_streams,0)>=0 AND COALESCE(u.max_transcode_streams,0)>=0
            AND COALESCE(p.max_streams,0)>=0 AND COALESCE(p.max_transcode_streams,0)>=0
            AND COALESCE(u.bandwidth_limit_bps,0)>=0 AND COALESCE(p.bandwidth_limit_bps,0)>=0)
    FROM users u JOIN streaming_policies p ON
        (u.streaming_policy_id IS NOT NULL AND p.id=u.streaming_policy_id)
        OR (u.streaming_policy_id IS NULL AND p.is_default)
    WHERE u.id=household AND (u.streaming_policy_id IS NOT NULL OR
        (SELECT count(*) FROM streaming_policies WHERE is_default)=1)
$$;

CREATE FUNCTION playback_vod_policy_retirement() RETURNS TRIGGER LANGUAGE plpgsql AS $$
DECLARE household UUID; policy JSONB;
BEGIN
    FOR household IN SELECT DISTINCT r.household_id FROM playback_reservations r
        JOIN playback_vod_commands c ON c.reservation_id=r.id
        WHERE r.state='held' AND (TG_TABLE_NAME='streaming_policies' OR r.household_id=NEW.id)
        ORDER BY r.household_id LOOP
        PERFORM id FROM users WHERE id=household FOR NO KEY UPDATE;
        policy:=playback_vod_policy(household);
        UPDATE playback_reservations r SET state='retiring' FROM playback_vod_commands c
        WHERE r.id=c.reservation_id AND r.household_id=household AND r.state='held' AND (
            policy IS NULL OR NOT (policy->>'valid')::boolean OR NOT (policy->>'supported')::boolean
            OR NOT (policy->>('allow_'||c.delivery_mode))::boolean
            OR (c.encodes AND NOT (policy->>'allow_transcode')::boolean)
            OR (c.source_4k AND ((c.encodes AND NOT (policy->>'allow_transcode_4k')::boolean)
                OR (c.delivery_mode<>'direct_play' AND (policy->>'require_direct_play_4k')::boolean))));
    END LOOP;
    RETURN NULL;
END;
$$;
CREATE CONSTRAINT TRIGGER playback_vod_policy_changed AFTER INSERT OR UPDATE OR DELETE ON streaming_policies
DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION playback_vod_policy_retirement();
CREATE CONSTRAINT TRIGGER playback_vod_user_policy_changed AFTER UPDATE ON users
DEFERRABLE INITIALLY DEFERRED FOR EACH ROW
WHEN (OLD.streaming_policy_id IS DISTINCT FROM NEW.streaming_policy_id
    OR OLD.max_streams IS DISTINCT FROM NEW.max_streams
    OR OLD.max_transcode_streams IS DISTINCT FROM NEW.max_transcode_streams
    OR OLD.bandwidth_limit_bps IS DISTINCT FROM NEW.bandwidth_limit_bps)
EXECUTE FUNCTION playback_vod_policy_retirement();

CREATE FUNCTION playback_vod_policy_lock() RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    LOCK TABLE streaming_policies IN ROW EXCLUSIVE MODE;
    RETURN NULL;
END;
$$;
CREATE TRIGGER playback_vod_user_policy_lock BEFORE UPDATE OF streaming_policy_id,max_streams,max_transcode_streams,bandwidth_limit_bps
ON users FOR EACH STATEMENT EXECUTE FUNCTION playback_vod_policy_lock();

CREATE FUNCTION playback_vod_source_retirement() RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    UPDATE playback_reservations r SET state='retiring' FROM playback_vod_commands c
    WHERE r.id=c.reservation_id AND r.state='held' AND (
        (TG_TABLE_NAME='media_files' AND r.vod_file_id=OLD.id)
        OR (TG_TABLE_NAME='user_profiles' AND r.profile_id=OLD.id AND c.ambient_channel_id IS NOT NULL)
        OR (TG_TABLE_NAME='ambient_channels' AND c.ambient_channel_id=OLD.id));
    IF TG_OP='DELETE' THEN RETURN OLD; END IF;
    RETURN NULL;
END;
$$;
CREATE TRIGGER playback_vod_source_evidence AFTER UPDATE ON media_files FOR EACH ROW
WHEN (OLD IS DISTINCT FROM NEW) EXECUTE FUNCTION playback_vod_source_retirement();
CREATE TRIGGER playback_vod_ambient_profile AFTER UPDATE OF allow_ambient_channels ON user_profiles FOR EACH ROW
WHEN (OLD.allow_ambient_channels AND NOT NEW.allow_ambient_channels) EXECUTE FUNCTION playback_vod_source_retirement();
CREATE TRIGGER playback_vod_ambient_changed AFTER UPDATE ON ambient_channels FOR EACH ROW
WHEN (OLD IS DISTINCT FROM NEW) EXECUTE FUNCTION playback_vod_source_retirement();
CREATE TRIGGER playback_vod_ambient_deleted BEFORE DELETE ON ambient_channels FOR EACH ROW EXECUTE FUNCTION playback_vod_source_retirement();

CREATE FUNCTION playback_vod_membership_retirement() RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    UPDATE playback_reservations r SET state='retiring' FROM playback_vod_commands c
    WHERE r.id=c.reservation_id AND r.state='held' AND c.ambient_channel_id=OLD.channel_id AND r.vod_item_id=OLD.media_item_id;
    RETURN NULL;
END;
$$;
CREATE TRIGGER playback_vod_ambient_member_removed AFTER DELETE OR UPDATE ON ambient_channel_items
FOR EACH ROW EXECUTE FUNCTION playback_vod_membership_retirement();

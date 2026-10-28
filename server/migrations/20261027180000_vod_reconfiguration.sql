-- Duskcue — Self-hosted media streaming server
-- Copyright (C) 2026-2026 Duskcue Contributors
-- Licensed under AGPL-3.0. See LICENSE for details.

LOCK TABLE playback_reservations,playback_vod_commands IN ACCESS EXCLUSIVE MODE;
DO $$ BEGIN
    IF EXISTS(SELECT 1 FROM playback_vod_commands WHERE (history_id IS NULL)<>(result IS NULL)
        OR (state='ready' AND delivery_mode<>'direct_play' AND generation_id IS NULL)) THEN
        RAISE EXCEPTION 'VOD legacy attribution is inconsistent' USING ERRCODE='PBR04';
    END IF;
END $$;
ALTER TABLE playback_vod_commands ADD COLUMN protocol SMALLINT NOT NULL DEFAULT 1 CHECK(protocol IN (1,2));

CREATE TABLE playback_vod_controls (
    reservation_id UUID PRIMARY KEY REFERENCES playback_vod_commands(reservation_id) ON DELETE CASCADE,
    revision BIGINT NOT NULL DEFAULT 1 CHECK(revision>0),
    state TEXT NOT NULL DEFAULT 'preparing' CHECK(state IN ('preparing','ready','unavailable','changing','uncertain','retiring')),
    current_output UUID,
    pending_operation UUID,
    claim_encodes BOOLEAN NOT NULL,
    recovery_budget JSONB NOT NULL DEFAULT '{"attempts":0,"restarted_stall":false,"skipped_decode":false,"exhausted":false}'::jsonb
        CHECK(octet_length(recovery_budget::text)<=1024 AND recovery_budget ?& ARRAY['attempts','restarted_stall','skipped_decode','exhausted']
            AND jsonb_typeof(recovery_budget->'attempts')='number'
            AND (recovery_budget->>'attempts')::integer BETWEEN 0 AND 2
            AND jsonb_typeof(recovery_budget->'restarted_stall')='boolean' AND jsonb_typeof(recovery_budget->'skipped_decode')='boolean' AND jsonb_typeof(recovery_budget->'exhausted')='boolean'),
    checkpoint_output UUID,
    checkpoint_sequence BIGINT NOT NULL DEFAULT 0 CHECK(checkpoint_sequence BETWEEN 0 AND 9007199254740991),
    checkpoint_position INTEGER CHECK(checkpoint_position>=0),
    playback_state TEXT NOT NULL DEFAULT 'playing' CHECK(playback_state IN ('playing','paused','buffering')),
    retained_operations INTEGER NOT NULL DEFAULT 0 CHECK(retained_operations BETWEEN 0 AND 256),
    CHECK((checkpoint_sequence=0)=(checkpoint_position IS NULL)),
    CHECK((checkpoint_sequence=0)=(checkpoint_output IS NULL)),
    CHECK(state<>'changing' OR pending_operation IS NOT NULL),
    CHECK(state='retiring' OR current_output IS NOT NULL)
);
CREATE TABLE playback_vod_operations (
    id UUID PRIMARY KEY CHECK(uuid_extract_version(id)=7),
    reservation_id UUID NOT NULL REFERENCES playback_vod_commands(reservation_id),
    household_id UUID NOT NULL,
    intent_hash BYTEA NOT NULL CHECK(octet_length(intent_hash)=32),
    condition TEXT NOT NULL CHECK(octet_length(condition) BETWEEN 1 AND 128),
    action TEXT NOT NULL CHECK(action IN ('seek','reconfigure','recover')),
    expected_output UUID NOT NULL,
    candidate_output UUID NOT NULL,
    evidence JSONB NOT NULL DEFAULT '{}'::jsonb CHECK(octet_length(evidence::text)<=2048),
    state TEXT NOT NULL DEFAULT 'accepted' CHECK(state IN ('accepted','retiring','preparing','completed','failed','cancelled','uncertain')),
    result JSONB CHECK(octet_length(result::text)<=8192),
    created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
    terminal_at TIMESTAMPTZ,
    cleanup_at TIMESTAMPTZ,
    prune_after TIMESTAMPTZ,
    UNIQUE(reservation_id,id),
    CHECK((state IN ('completed','failed','cancelled'))=(terminal_at IS NOT NULL)),
    CHECK((terminal_at IS NOT NULL AND cleanup_at IS NOT NULL)=(prune_after IS NOT NULL))
);
CREATE UNIQUE INDEX playback_vod_operation_pending ON playback_vod_operations(reservation_id) WHERE state IN ('accepted','retiring','preparing','uncertain');
CREATE INDEX playback_vod_operation_household ON playback_vod_operations(household_id,id);
CREATE INDEX playback_vod_operation_prune ON playback_vod_operations(prune_after,id) INCLUDE(household_id,reservation_id) WHERE prune_after IS NOT NULL;
CREATE TABLE playback_vod_outputs (
    id UUID PRIMARY KEY CHECK(uuid_extract_version(id)=7),
    reservation_id UUID NOT NULL REFERENCES playback_vod_commands(reservation_id) ON DELETE CASCADE,
    operation_id UUID UNIQUE REFERENCES playback_vod_operations(id) DEFERRABLE INITIALLY DEFERRED,
    generation_id UUID UNIQUE,
    source_hash BYTEA NOT NULL CHECK(octet_length(source_hash)=32),
    policy_hash BYTEA NOT NULL CHECK(octet_length(policy_hash)=32),
    delivery_mode TEXT NOT NULL CHECK(delivery_mode IN ('direct_play','direct_stream','transcode')),
    encodes BOOLEAN NOT NULL,
    source_4k BOOLEAN NOT NULL,
    configuration JSONB NOT NULL DEFAULT '{}'::jsonb CHECK(octet_length(configuration::text)<=8192),
    state TEXT NOT NULL DEFAULT 'preparing' CHECK(state IN ('preparing','ready','retiring','complete')),
    result JSONB CHECK(octet_length(result::text)<=8192),
    failure JSONB CHECK(octet_length(failure::text)<=2048),
    published_at TIMESTAMPTZ,
    resources_completed BOOLEAN NOT NULL DEFAULT false,
    cleanup_uncertain BOOLEAN NOT NULL DEFAULT false,
    completed_at TIMESTAMPTZ,
    UNIQUE(reservation_id,id),
    CHECK((published_at IS NULL)=(result IS NULL)),
    CHECK(state<>'ready' OR published_at IS NOT NULL),
    CHECK(state<>'ready' OR delivery_mode='direct_play' OR generation_id IS NOT NULL),
    CHECK(resources_completed=(completed_at IS NOT NULL)),
    CHECK((state='complete')=resources_completed),
    CHECK(delivery_mode<>'direct_play' OR (NOT encodes AND generation_id IS NULL))
);
CREATE UNIQUE INDEX playback_vod_initial_output ON playback_vod_outputs(reservation_id) WHERE operation_id IS NULL;
ALTER TABLE playback_vod_controls
    ADD FOREIGN KEY(reservation_id,current_output) REFERENCES playback_vod_outputs(reservation_id,id) DEFERRABLE INITIALLY DEFERRED,
    ADD FOREIGN KEY(reservation_id,pending_operation) REFERENCES playback_vod_operations(reservation_id,id) DEFERRABLE INITIALLY DEFERRED;
ALTER TABLE playback_vod_operations
    ADD FOREIGN KEY(reservation_id,candidate_output) REFERENCES playback_vod_outputs(reservation_id,id) DEFERRABLE INITIALLY DEFERRED;

INSERT INTO playback_vod_outputs(id,reservation_id,generation_id,source_hash,policy_hash,delivery_mode,encodes,source_4k,state,result,published_at,resources_completed,completed_at,cleanup_uncertain)
SELECT uuidv7(),reservation_id,generation_id,source_hash,policy_hash,delivery_mode,encodes,source_4k,
    CASE WHEN resources_completed THEN 'complete' WHEN state='ready' THEN 'ready' WHEN state='preparing' THEN 'preparing' ELSE 'retiring' END,
    result,CASE WHEN result IS NOT NULL THEN clock_timestamp() END,resources_completed,CASE WHEN resources_completed THEN clock_timestamp() END,cleanup_uncertain
FROM playback_vod_commands;
INSERT INTO playback_vod_controls(reservation_id,current_output,claim_encodes,state)
SELECT c.reservation_id,o.id,c.encodes AND NOT c.resources_completed,
    CASE WHEN c.resources_completed OR c.state IN ('stopping','failed') THEN 'retiring' WHEN c.state='ready' THEN 'ready' ELSE 'preparing' END
FROM playback_vod_commands c JOIN playback_vod_outputs o ON o.reservation_id=c.reservation_id;

CREATE TABLE playback_vod_operation_retention (
    id BOOLEAN PRIMARY KEY DEFAULT true CHECK(id),
    cutoff TIMESTAMPTZ NOT NULL CHECK(isfinite(cutoff)),
    retained_count BIGINT NOT NULL DEFAULT 0 CHECK(retained_count BETWEEN 0 AND 131072),
    scan_after TIMESTAMPTZ,scan_id UUID,CHECK((scan_after IS NULL)=(scan_id IS NULL))
);
INSERT INTO playback_vod_operation_retention(id,cutoff) VALUES(true,date_trunc('milliseconds',clock_timestamp())-interval '5 minutes');

CREATE FUNCTION playback_vod_protocol_guard() RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    IF NEW.protocol IS DISTINCT FROM OLD.protocol THEN RAISE EXCEPTION 'VOD protocol attribution is immutable' USING ERRCODE='PBR04'; END IF;
    RETURN NEW;
END;
$$;
CREATE TRIGGER playback_vod_protocol_guard BEFORE UPDATE ON playback_vod_commands FOR EACH ROW EXECUTE FUNCTION playback_vod_protocol_guard();

CREATE FUNCTION playback_vod_operation_retention_guard() RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    IF TG_OP<>'UPDATE' THEN RAISE EXCEPTION 'VOD operation retention mutation refused' USING ERRCODE='PBR04'; END IF;
    IF NEW.id IS DISTINCT FROM OLD.id OR NEW.cutoff<OLD.cutoff
        OR (NEW.retained_count<>OLD.retained_count AND (pg_trigger_depth()<>2 OR abs(NEW.retained_count-OLD.retained_count)<>1)) THEN
        RAISE EXCEPTION 'VOD operation retention mutation refused' USING ERRCODE='PBR04';
    END IF;
    RETURN NEW;
END;
$$;
CREATE TRIGGER playback_vod_operation_retention_guard BEFORE UPDATE OR DELETE ON playback_vod_operation_retention FOR EACH ROW EXECUTE FUNCTION playback_vod_operation_retention_guard();
CREATE TRIGGER playback_vod_operation_retention_truncate BEFORE TRUNCATE ON playback_vod_operation_retention FOR EACH STATEMENT EXECUTE FUNCTION playback_vod_operation_retention_guard();

CREATE FUNCTION playback_vod_control_guard() RETURNS TRIGGER LANGUAGE plpgsql AS $$
DECLARE other_count BIGINT; cap INTEGER; policy JSONB;
BEGIN
    IF TG_OP='INSERT' AND (NEW.retained_operations<>0 OR NEW.pending_operation IS NOT NULL OR NEW.state<>'preparing'
        OR NEW.checkpoint_sequence<>0 OR (NEW.recovery_budget->>'attempts')::integer<>0
        OR (NEW.recovery_budget->>'restarted_stall')::boolean OR (NEW.recovery_budget->>'skipped_decode')::boolean OR (NEW.recovery_budget->>'exhausted')::boolean) THEN
        RAISE EXCEPTION 'VOD control requires initial attribution' USING ERRCODE='PBR04';
    END IF;
    IF TG_OP='DELETE' THEN
        IF EXISTS(SELECT 1 FROM playback_vod_commands WHERE reservation_id=OLD.reservation_id) THEN RAISE EXCEPTION 'VOD control deletion refused' USING ERRCODE='PBR04'; END IF;
        RETURN OLD;
    END IF;
    IF TG_OP='UPDATE' AND (NEW.reservation_id IS DISTINCT FROM OLD.reservation_id OR NEW.revision<OLD.revision
        OR (NEW.retained_operations<>OLD.retained_operations AND (pg_trigger_depth()<>2 OR abs(NEW.retained_operations-OLD.retained_operations)<>1))
        OR (OLD.state='retiring' AND NEW.state<>'retiring')
        OR (OLD.state='uncertain' AND NEW.state NOT IN ('uncertain','retiring'))
        OR (NEW.recovery_budget->>'attempts')::integer<(OLD.recovery_budget->>'attempts')::integer
        OR ((OLD.recovery_budget->>'exhausted')::boolean AND NOT (NEW.recovery_budget->>'exhausted')::boolean)
        OR ((OLD.recovery_budget->>'restarted_stall')::boolean AND NOT (NEW.recovery_budget->>'restarted_stall')::boolean)
        OR ((OLD.recovery_budget->>'skipped_decode')::boolean AND NOT (NEW.recovery_budget->>'skipped_decode')::boolean)) THEN
        RAISE EXCEPTION 'VOD control regression refused' USING ERRCODE='PBR04';
    END IF;
    IF NEW.current_output IS NOT NULL AND NOT EXISTS(SELECT 1 FROM playback_vod_outputs WHERE id=NEW.current_output AND reservation_id=NEW.reservation_id) THEN
        RAISE EXCEPTION 'VOD output attribution conflicts' USING ERRCODE='PBR04';
    END IF;
    IF NOT NEW.claim_encodes AND (EXISTS(SELECT 1 FROM playback_vod_outputs WHERE reservation_id=NEW.reservation_id AND encodes AND NOT resources_completed)
        OR (NEW.state<>'retiring' AND EXISTS(SELECT 1 FROM playback_vod_outputs WHERE id=NEW.current_output AND encodes))) THEN
        RAISE EXCEPTION 'VOD encoding obligation remains' USING ERRCODE='PBR04';
    END IF;
    IF NEW.state='ready' AND (NEW.pending_operation IS NOT NULL OR NOT EXISTS(SELECT 1 FROM playback_vod_outputs WHERE id=NEW.current_output AND state='ready' AND published_at IS NOT NULL AND NOT resources_completed)) THEN
        RAISE EXCEPTION 'VOD current output is unavailable' USING ERRCODE='PBR04';
    END IF;
    IF TG_OP='UPDATE' THEN
        IF NEW.current_output IS DISTINCT FROM OLD.current_output AND NEW.current_output IS NOT NULL AND
            NOT EXISTS(SELECT 1 FROM playback_vod_outputs WHERE id=NEW.current_output AND state='ready' AND NOT resources_completed) THEN
            RAISE EXCEPTION 'VOD current output cannot revive' USING ERRCODE='PBR04';
        END IF;
        IF NEW.checkpoint_sequence<>OLD.checkpoint_sequence OR NEW.checkpoint_output IS DISTINCT FROM OLD.checkpoint_output OR NEW.checkpoint_position IS DISTINCT FROM OLD.checkpoint_position THEN
            IF NEW.current_output IS DISTINCT FROM OLD.current_output THEN
                IF NEW.checkpoint_sequence<>0 THEN RAISE EXCEPTION 'VOD replacement checkpoint invalid' USING ERRCODE='PBR04'; END IF;
            ELSIF NEW.checkpoint_sequence<=OLD.checkpoint_sequence OR NEW.checkpoint_output IS DISTINCT FROM NEW.current_output OR NEW.pending_operation IS NOT NULL OR NEW.state NOT IN ('ready','unavailable') THEN
                RAISE EXCEPTION 'VOD checkpoint is stale' USING ERRCODE='PBR04';
            END IF;
        END IF;
        IF (NEW.state,NEW.current_output,NEW.pending_operation,NEW.recovery_budget,NEW.playback_state)
            IS DISTINCT FROM (OLD.state,OLD.current_output,OLD.pending_operation,OLD.recovery_budget,OLD.playback_state) THEN NEW.revision:=OLD.revision+1; END IF;
    END IF;
    IF NEW.claim_encodes AND (TG_OP='INSERT' OR NOT OLD.claim_encodes) THEN
        PERFORM id FROM users WHERE id=(SELECT household_id FROM playback_reservations WHERE id=NEW.reservation_id) FOR NO KEY UPDATE;
        SELECT playback_vod_policy(r.household_id) INTO policy FROM playback_reservations r WHERE r.id=NEW.reservation_id;
        IF NOT COALESCE((policy->>'valid')::boolean AND (policy->>'supported')::boolean AND (policy->>'allow_transcode')::boolean,false) THEN
            RAISE EXCEPTION 'VOD encoding policy unavailable' USING ERRCODE='PBR04';
        END IF;
        cap:=(policy->>'transcode')::integer;
        SELECT count(*) INTO other_count FROM playback_reservations r JOIN playback_vod_commands c ON c.reservation_id=r.id
            LEFT JOIN playback_vod_controls v ON v.reservation_id=r.id
            WHERE r.household_id=(SELECT household_id FROM playback_reservations WHERE id=NEW.reservation_id)
                AND r.id<>NEW.reservation_id AND r.state<>'released' AND COALESCE(v.claim_encodes,c.encodes);
        IF cap IS NOT NULL AND other_count>=cap THEN RAISE EXCEPTION 'VOD encoded viewer capacity unavailable' USING ERRCODE='PBR03'; END IF;
    END IF;
    RETURN NEW;
END;
$$;
CREATE TRIGGER playback_vod_control_guard BEFORE INSERT OR UPDATE OR DELETE ON playback_vod_controls FOR EACH ROW EXECUTE FUNCTION playback_vod_control_guard();

CREATE FUNCTION playback_vod_output_guard() RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    IF TG_OP='DELETE' THEN
        IF EXISTS(SELECT 1 FROM playback_vod_commands WHERE reservation_id=OLD.reservation_id) AND
            (OLD.operation_id IS NULL OR NOT OLD.resources_completed OR NOT EXISTS(SELECT 1 FROM playback_vod_operations WHERE id=OLD.operation_id AND prune_after<=clock_timestamp())
             OR EXISTS(SELECT 1 FROM playback_vod_controls WHERE current_output=OLD.id OR pending_operation=OLD.operation_id)) THEN
            RAISE EXCEPTION 'VOD output deletion refused' USING ERRCODE='PBR04';
        END IF;
        RETURN OLD;
    END IF;
    IF TG_OP='UPDATE' THEN
        IF (NEW.id,NEW.reservation_id,NEW.operation_id,NEW.generation_id,NEW.source_hash,NEW.policy_hash,NEW.delivery_mode,NEW.encodes,NEW.source_4k,NEW.configuration)
            IS DISTINCT FROM (OLD.id,OLD.reservation_id,OLD.operation_id,OLD.generation_id,OLD.source_hash,OLD.policy_hash,OLD.delivery_mode,OLD.encodes,OLD.source_4k,OLD.configuration)
            OR (OLD.result IS NOT NULL AND (NEW.result,NEW.published_at) IS DISTINCT FROM (OLD.result,OLD.published_at))
            OR (OLD.resources_completed AND NOT NEW.resources_completed) OR (OLD.cleanup_uncertain AND NOT NEW.cleanup_uncertain)
            OR (OLD.failure IS NOT NULL AND NEW.failure IS DISTINCT FROM OLD.failure)
            OR (OLD.state IN ('retiring','complete') AND NEW.state NOT IN ('retiring','complete'))
            OR (OLD.state='ready' AND NEW.state='preparing') OR (OLD.completed_at IS NOT NULL AND NEW.completed_at IS DISTINCT FROM OLD.completed_at) THEN
            RAISE EXCEPTION 'VOD output attribution is immutable' USING ERRCODE='PBR04';
        END IF;
    ELSE
        IF NOT EXISTS(SELECT 1 FROM playback_reservations WHERE id=NEW.reservation_id AND state='held') OR NEW.state<>'preparing' OR NEW.resources_completed OR NEW.result IS NOT NULL
            OR playback_receipt_time_error(NEW.id,clock_timestamp(),(SELECT cutoff FROM playback_vod_operation_retention WHERE id))<>0 THEN
            RAISE EXCEPTION 'VOD output requires live attribution' USING ERRCODE='PBR04';
        END IF;
    END IF;
    IF TG_OP='UPDATE' AND NEW.result IS NOT NULL AND OLD.result IS NULL THEN NEW.published_at:=clock_timestamp(); END IF;
    IF NEW.resources_completed AND NOT OLD.resources_completed THEN NEW.completed_at:=clock_timestamp(); END IF;
    RETURN NEW;
END;
$$;
CREATE TRIGGER playback_vod_output_guard BEFORE INSERT OR UPDATE OR DELETE ON playback_vod_outputs FOR EACH ROW EXECUTE FUNCTION playback_vod_output_guard();

CREATE FUNCTION playback_vod_operation_guard() RETURNS TRIGGER LANGUAGE plpgsql AS $$
DECLARE floor_at TIMESTAMPTZ; total BIGINT; timing SMALLINT; viewer_total INTEGER; household_total BIGINT;
BEGIN
    IF TG_OP='DELETE' THEN
        IF OLD.prune_after IS NULL OR OLD.prune_after>clock_timestamp()
            OR EXISTS(SELECT 1 FROM playback_vod_outputs WHERE operation_id=OLD.id)
            OR EXISTS(SELECT 1 FROM playback_vod_controls WHERE pending_operation=OLD.id) THEN
            RAISE EXCEPTION 'VOD operation deletion refused' USING ERRCODE='PBR04';
        END IF;
        SELECT cutoff INTO STRICT floor_at FROM playback_vod_operation_retention WHERE id FOR UPDATE;
        IF uuid_extract_timestamp(OLD.id)>=floor_at THEN RAISE EXCEPTION 'VOD operation replay fence unavailable' USING ERRCODE='PBR04'; END IF;
        RETURN OLD;
    END IF;
    IF TG_OP='INSERT' THEN
        IF NOT EXISTS(SELECT 1 FROM playback_reservations r JOIN playback_vod_commands c ON c.reservation_id=r.id
            WHERE r.id=NEW.reservation_id AND r.household_id=NEW.household_id AND r.state='held' AND c.protocol=2 AND c.history_id IS NOT NULL)
            OR NEW.state<>'accepted' OR NEW.terminal_at IS NOT NULL OR NEW.cleanup_at IS NOT NULL OR NEW.prune_after IS NOT NULL OR NEW.result IS NOT NULL THEN
            RAISE EXCEPTION 'VOD operation requires established attribution' USING ERRCODE='PBR04';
        END IF;
        SELECT retained_operations INTO STRICT viewer_total FROM playback_vod_controls WHERE reservation_id=NEW.reservation_id FOR UPDATE;
        IF NOT EXISTS(SELECT 1 FROM playback_vod_controls WHERE reservation_id=NEW.reservation_id AND current_output=NEW.expected_output AND pending_operation IS NULL AND state IN ('ready','unavailable')) THEN
            RAISE EXCEPTION 'VOD operation context changed' USING ERRCODE='PBR04';
        END IF;
        SELECT cutoff,retained_count INTO STRICT floor_at,total FROM playback_vod_operation_retention WHERE id FOR UPDATE;
        timing:=playback_receipt_time_error(NEW.id,clock_timestamp(),floor_at);
        IF timing=1 THEN RAISE EXCEPTION 'VOD operation expired' USING ERRCODE='PBR01'; END IF;
        IF timing=2 THEN RAISE EXCEPTION 'VOD operation time unavailable' USING ERRCODE='PBR02'; END IF;
        IF timing<>0 THEN RAISE EXCEPTION 'VOD operation identity conflicts' USING ERRCODE='PBR04'; END IF;
        SELECT count(*) INTO household_total FROM playback_vod_operations WHERE household_id=NEW.household_id;
        IF viewer_total>=256 OR household_total>=8192 OR total>=131072 THEN RAISE EXCEPTION 'VOD operation capacity unavailable' USING ERRCODE='PBR03'; END IF;
        UPDATE playback_vod_operation_retention SET cutoff=greatest(cutoff,date_trunc('milliseconds',clock_timestamp())-interval '5 minutes') WHERE id;
        NEW.created_at:=clock_timestamp();
    ELSE
        IF (NEW.id,NEW.reservation_id,NEW.household_id,NEW.intent_hash,NEW.condition,NEW.action,NEW.expected_output,NEW.candidate_output,NEW.created_at,NEW.evidence)
            IS DISTINCT FROM (OLD.id,OLD.reservation_id,OLD.household_id,OLD.intent_hash,OLD.condition,OLD.action,OLD.expected_output,OLD.candidate_output,OLD.created_at,OLD.evidence)
            OR (OLD.terminal_at IS NOT NULL AND (NEW.state,NEW.terminal_at,NEW.result) IS DISTINCT FROM (OLD.state,OLD.terminal_at,OLD.result))
            OR (OLD.cleanup_at IS NOT NULL AND NEW.cleanup_at IS DISTINCT FROM OLD.cleanup_at)
            OR (OLD.state='uncertain' AND NEW.state NOT IN ('uncertain','cancelled'))
            OR (OLD.state IN ('retiring','preparing') AND NEW.state='accepted') OR (OLD.state='preparing' AND NEW.state='retiring') THEN
            RAISE EXCEPTION 'VOD operation regression refused' USING ERRCODE='PBR04';
        END IF;
        IF NEW.state IN ('completed','failed','cancelled') AND OLD.terminal_at IS NULL THEN NEW.terminal_at:=clock_timestamp(); END IF;
        IF NEW.cleanup_at IS NOT NULL AND EXISTS(SELECT 1 FROM playback_vod_outputs WHERE operation_id=NEW.id AND NOT resources_completed) THEN
            RAISE EXCEPTION 'VOD output completion unavailable' USING ERRCODE='PBR04';
        END IF;
        IF NEW.cleanup_at IS NOT NULL AND OLD.cleanup_at IS NULL THEN NEW.cleanup_at:=clock_timestamp(); END IF;
        NEW.prune_after:=CASE WHEN NEW.terminal_at IS NOT NULL AND NEW.cleanup_at IS NOT NULL THEN greatest(NEW.terminal_at,NEW.cleanup_at)+interval '24 hours' END;
    END IF;
    RETURN NEW;
END;
$$;
CREATE TRIGGER playback_vod_operation_guard BEFORE INSERT OR UPDATE OR DELETE ON playback_vod_operations FOR EACH ROW EXECUTE FUNCTION playback_vod_operation_guard();

CREATE FUNCTION playback_vod_operation_account() RETURNS TRIGGER LANGUAGE plpgsql AS $$
DECLARE change INTEGER:=CASE WHEN TG_OP='INSERT' THEN 1 ELSE -1 END; viewer UUID;
BEGIN
    viewer:=CASE WHEN TG_OP='INSERT' THEN NEW.reservation_id ELSE OLD.reservation_id END;
    UPDATE playback_vod_operation_retention SET retained_count=retained_count+change WHERE id;
    UPDATE playback_vod_controls SET retained_operations=retained_operations+change WHERE reservation_id=viewer;
    IF NOT FOUND THEN RAISE EXCEPTION 'VOD operation counter unavailable' USING ERRCODE='PBR04'; END IF;
    RETURN NULL;
END;
$$;
CREATE TRIGGER playback_vod_operation_account AFTER INSERT OR DELETE ON playback_vod_operations FOR EACH ROW EXECUTE FUNCTION playback_vod_operation_account();
CREATE TRIGGER playback_vod_control_truncate BEFORE TRUNCATE ON playback_vod_controls FOR EACH STATEMENT EXECUTE FUNCTION playback_vod_no_truncate();
CREATE TRIGGER playback_vod_output_truncate BEFORE TRUNCATE ON playback_vod_outputs FOR EACH STATEMENT EXECUTE FUNCTION playback_vod_no_truncate();
CREATE TRIGGER playback_vod_operation_truncate BEFORE TRUNCATE ON playback_vod_operations FOR EACH STATEMENT EXECUTE FUNCTION playback_vod_no_truncate();

CREATE FUNCTION playback_vod_parent_retention_guard() RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    IF EXISTS(SELECT 1 FROM playback_vod_operations WHERE reservation_id=OLD.id) THEN RAISE EXCEPTION 'VOD operation receipts remain' USING ERRCODE='PBR04'; END IF;
    RETURN OLD;
END;
$$;
CREATE TRIGGER playback_vod_parent_retention_guard BEFORE DELETE ON playback_reservations FOR EACH ROW EXECUTE FUNCTION playback_vod_parent_retention_guard();

CREATE FUNCTION playback_vod_release_guard() RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    IF NEW.state='released' AND (EXISTS(SELECT 1 FROM playback_vod_outputs WHERE reservation_id=NEW.id AND NOT resources_completed)
        OR EXISTS(SELECT 1 FROM playback_vod_operations WHERE reservation_id=NEW.id AND state NOT IN ('completed','failed','cancelled'))) THEN
        RAISE EXCEPTION 'VOD output obligations remain' USING ERRCODE='PBR04';
    END IF;
    RETURN NEW;
END;
$$;
CREATE TRIGGER playback_vod_release_guard BEFORE UPDATE ON playback_reservations FOR EACH ROW EXECUTE FUNCTION playback_vod_release_guard();

CREATE FUNCTION playback_vod_graph_guard() RETURNS TRIGGER LANGUAGE plpgsql AS $$
DECLARE v playback_vod_controls%ROWTYPE; o playback_vod_outputs%ROWTYPE; p playback_vod_operations%ROWTYPE; root playback_vod_commands%ROWTYPE;
BEGIN
    SELECT * INTO v FROM playback_vod_controls WHERE reservation_id=NEW.reservation_id;
    IF NOT FOUND THEN
        IF EXISTS(SELECT 1 FROM playback_vod_commands WHERE reservation_id=NEW.reservation_id) THEN RAISE EXCEPTION 'VOD control ownership unavailable' USING ERRCODE='PBR04'; END IF;
        RETURN NULL;
    END IF;
    SELECT * INTO STRICT root FROM playback_vod_commands WHERE reservation_id=NEW.reservation_id;
    IF TG_TABLE_NAME='playback_vod_outputs' THEN
        SELECT * INTO o FROM playback_vod_outputs WHERE id=NEW.id;
        IF NOT FOUND THEN RETURN NULL; END IF;
        IF o.source_hash<>root.source_hash OR o.source_4k<>root.source_4k OR (o.encodes AND NOT o.resources_completed AND NOT v.claim_encodes) THEN
            RAISE EXCEPTION 'VOD output source or capacity attribution conflicts' USING ERRCODE='PBR04';
        END IF;
        IF o.operation_id IS NOT NULL AND NOT EXISTS(SELECT 1 FROM playback_vod_operations WHERE id=o.operation_id AND reservation_id=o.reservation_id AND candidate_output=o.id) THEN
            RAISE EXCEPTION 'VOD output command attribution conflicts' USING ERRCODE='PBR04';
        END IF;
    ELSIF TG_TABLE_NAME='playback_vod_operations' THEN
        SELECT * INTO p FROM playback_vod_operations WHERE id=NEW.id;
        IF NOT FOUND THEN RETURN NULL; END IF;
        SELECT * INTO STRICT o FROM playback_vod_outputs WHERE id=p.candidate_output;
        IF o.operation_id IS DISTINCT FROM p.id OR o.reservation_id<>p.reservation_id
            OR (p.terminal_at IS NULL AND (v.pending_operation IS DISTINCT FROM p.id OR v.state NOT IN ('changing','uncertain','retiring')))
            OR (p.terminal_at IS NOT NULL AND v.pending_operation=p.id)
            OR (p.state='completed' AND (o.published_at IS NULL OR p.result IS DISTINCT FROM o.result)) THEN
            RAISE EXCEPTION 'VOD command graph conflicts' USING ERRCODE='PBR04';
        END IF;
    ELSE
        IF v.current_output IS NULL AND EXISTS(SELECT 1 FROM playback_reservations WHERE id=v.reservation_id AND state<>'released') THEN
            RAISE EXCEPTION 'VOD current attribution cannot be discarded' USING ERRCODE='PBR04';
        END IF;
        IF v.pending_operation IS NOT NULL AND NOT EXISTS(SELECT 1 FROM playback_vod_operations WHERE id=v.pending_operation AND reservation_id=v.reservation_id AND terminal_at IS NULL) THEN
            RAISE EXCEPTION 'VOD pending attribution conflicts' USING ERRCODE='PBR04';
        END IF;
        IF NOT v.claim_encodes AND EXISTS(SELECT 1 FROM playback_vod_outputs WHERE reservation_id=v.reservation_id AND encodes AND NOT resources_completed) THEN
            RAISE EXCEPTION 'VOD encoded obligation is uncounted' USING ERRCODE='PBR04';
        END IF;
    END IF;
    RETURN NULL;
END;
$$;
CREATE CONSTRAINT TRIGGER playback_vod_control_graph AFTER INSERT OR UPDATE ON playback_vod_controls DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION playback_vod_graph_guard();
CREATE CONSTRAINT TRIGGER playback_vod_output_graph AFTER INSERT OR UPDATE ON playback_vod_outputs DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION playback_vod_graph_guard();
CREATE CONSTRAINT TRIGGER playback_vod_operation_graph AFTER INSERT OR UPDATE ON playback_vod_operations DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION playback_vod_graph_guard();

CREATE FUNCTION playback_vod_permitted(viewer UUID,policy JSONB) RETURNS BOOLEAN LANGUAGE SQL STABLE AS $$
    SELECT COALESCE(policy IS NOT NULL AND (policy->>'valid')::boolean AND (policy->>'supported')::boolean AND
        NOT EXISTS(SELECT 1 FROM (
            SELECT o.delivery_mode,o.encodes,o.source_4k FROM playback_vod_controls v JOIN playback_vod_outputs o ON o.reservation_id=v.reservation_id
            LEFT JOIN playback_vod_operations p ON p.id=v.pending_operation
            WHERE v.reservation_id=viewer AND (o.id=p.candidate_output OR (p.id IS NULL AND o.id=v.current_output))
            UNION ALL SELECT c.delivery_mode,c.encodes,c.source_4k FROM playback_vod_commands c WHERE c.reservation_id=viewer AND NOT EXISTS(SELECT 1 FROM playback_vod_controls WHERE reservation_id=viewer)
        ) output WHERE NOT COALESCE((policy->>('allow_'||output.delivery_mode))::boolean,false)
            OR (output.encodes AND NOT COALESCE((policy->>'allow_transcode')::boolean,false))
            OR (output.source_4k AND ((output.encodes AND NOT COALESCE((policy->>'allow_transcode_4k')::boolean,false))
                OR (output.delivery_mode<>'direct_play' AND COALESCE((policy->>'require_direct_play_4k')::boolean,true))))),false)
$$;
CREATE OR REPLACE FUNCTION playback_vod_policy_retirement() RETURNS TRIGGER LANGUAGE plpgsql AS $$
DECLARE household UUID; policy JSONB;
BEGIN
    FOR household IN SELECT DISTINCT r.household_id FROM playback_reservations r JOIN playback_vod_commands c ON c.reservation_id=r.id
        WHERE r.state='held' AND (TG_TABLE_NAME='streaming_policies' OR r.household_id=NEW.id) ORDER BY r.household_id LOOP
        PERFORM id FROM users WHERE id=household FOR NO KEY UPDATE;
        policy:=playback_vod_policy(household);
        UPDATE playback_reservations r SET state='retiring' WHERE r.household_id=household AND r.state='held'
            AND EXISTS(SELECT 1 FROM playback_vod_commands WHERE reservation_id=r.id) AND NOT playback_vod_permitted(r.id,policy);
    END LOOP;
    RETURN NULL;
END;
$$;

CREATE FUNCTION playback_vod_complete_outputs(viewer UUID) RETURNS VOID LANGUAGE plpgsql AS $$
BEGIN
    UPDATE playback_vod_outputs SET state='complete',resources_completed=true WHERE reservation_id=viewer AND NOT resources_completed;
    UPDATE playback_vod_operations SET state=CASE WHEN terminal_at IS NULL THEN 'cancelled' ELSE state END,cleanup_at=COALESCE(cleanup_at,clock_timestamp()) WHERE reservation_id=viewer;
    UPDATE playback_vod_controls SET state='retiring',pending_operation=NULL,claim_encodes=false WHERE reservation_id=viewer;
END;
$$;

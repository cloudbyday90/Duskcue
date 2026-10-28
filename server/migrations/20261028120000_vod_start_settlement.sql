-- Duskcue — Self-hosted media streaming server
-- Copyright (C) 2026-2026 Duskcue Contributors
-- Licensed under AGPL-3.0. See LICENSE for details.

LOCK TABLE playback_instances,playback_reservations,playback_vod_commands IN ACCESS EXCLUSIVE MODE;
ALTER TABLE playback_instances ADD COLUMN start_intent_protocol SMALLINT NOT NULL DEFAULT 0 CHECK (start_intent_protocol IN (0,2));
CREATE FUNCTION playback_instance_protocol_insert() RETURNS TRIGGER
LANGUAGE plpgsql SET search_path=pg_catalog,public,pg_temp AS $$
BEGIN
    IF NEW.start_intent_protocol<>2 THEN RAISE EXCEPTION USING ERRCODE='PBR04',MESSAGE='incompatible playback instance refused'; END IF;
    RETURN NEW;
END
$$;
CREATE TRIGGER playback_instance_protocol_insert BEFORE INSERT ON playback_instances FOR EACH ROW EXECUTE FUNCTION playback_instance_protocol_insert();
CREATE INDEX playback_instances_incompatible ON playback_instances(id) WHERE start_intent_protocol<>2 AND reconciled_at IS NULL;
CREATE TABLE playback_start_intents (
    id UUID PRIMARY KEY CHECK (uuid_extract_version(id)=7),
    household_id UUID NOT NULL REFERENCES playback_receipt_households(household_id),
    session_id UUID NOT NULL,
    credential_version BIGINT NOT NULL CHECK (credential_version>0),
    transport SMALLINT NOT NULL CHECK (transport IN (0,1)),
    profile_id UUID,
    profile_revision BIGINT CHECK (profile_revision>0),
    intent_hash BYTEA CHECK (octet_length(intent_hash)=32),
    instance_id UUID NOT NULL REFERENCES playback_instances(id),
    admission_fenced BOOLEAN NOT NULL DEFAULT false,
    preparation_complete BOOLEAN NOT NULL DEFAULT false,
    promoted BOOLEAN NOT NULL DEFAULT false,
    created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
    admission_deadline TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp()+interval '60 seconds',
    settled_at TIMESTAMPTZ,
    prune_after TIMESTAMPTZ,
    CHECK ((intent_hash IS NOT NULL)=(profile_id IS NOT NULL AND profile_revision IS NOT NULL)),
    CHECK (intent_hash IS NOT NULL OR admission_fenced),
    CHECK ((settled_at IS NULL)=(prune_after IS NULL)),
    CHECK (settled_at IS NULL OR (admission_fenced AND preparation_complete))
);
INSERT INTO playback_start_intents(id,household_id,session_id,credential_version,transport,profile_id,profile_revision,intent_hash,instance_id,preparation_complete,promoted)
SELECT r.id,r.household_id,(r.fingerprint::jsonb->>'session')::uuid,(r.fingerprint::jsonb->>'version')::bigint,(r.fingerprint::jsonb->>'transport')::smallint,
(r.fingerprint::jsonb->>'profile')::uuid,(r.fingerprint::jsonb->>'profile_revision')::bigint,c.intent_hash,r.instance_id,true,true
FROM playback_reservations r JOIN playback_vod_commands c ON c.reservation_id=r.id;
CREATE INDEX playback_start_intent_pruning ON playback_start_intents(prune_after,id) WHERE settled_at IS NOT NULL;
CREATE INDEX playback_start_intent_instance ON playback_start_intents(instance_id,id) WHERE NOT preparation_complete;

CREATE FUNCTION playback_start_intent_insert() RETURNS TRIGGER
LANGUAGE plpgsql SET search_path=pg_catalog,public,pg_temp AS $$
DECLARE floor_at TIMESTAMPTZ; total BIGINT; household_total BIGINT; observed_at TIMESTAMPTZ; timing SMALLINT;
BEGIN
    SELECT cutoff,retained_count INTO STRICT floor_at,total FROM playback_retention WHERE id FOR UPDATE;
    IF EXISTS(SELECT 1 FROM playback_reservations WHERE id=NEW.id) THEN
        RAISE EXCEPTION USING ERRCODE='PBR04',MESSAGE='unregistered playback identity conflicts';
    END IF;
    observed_at:=date_trunc('milliseconds',clock_timestamp());
    timing:=playback_receipt_time_error(NEW.id,observed_at,floor_at);
    IF timing=1 THEN RAISE EXCEPTION USING ERRCODE='PBR01',MESSAGE='playback request expired'; END IF;
    IF timing=2 THEN RAISE EXCEPTION USING ERRCODE='PBR02',MESSAGE='playback request time unavailable'; END IF;
    IF timing<>0 THEN RAISE EXCEPTION USING ERRCODE='PBR04',MESSAGE='playback request identity invalid'; END IF;
    IF total>=65536 THEN RAISE EXCEPTION USING ERRCODE='PBR03',MESSAGE='playback receipt capacity unavailable'; END IF;
    SELECT retained_count INTO household_total FROM playback_receipt_households WHERE household_id=NEW.household_id FOR UPDATE;
    IF NOT FOUND THEN
        INSERT INTO playback_receipt_households(household_id,retained_count) VALUES (NEW.household_id,0);
        household_total:=0;
    END IF;
    IF household_total>=4096 THEN RAISE EXCEPTION USING ERRCODE='PBR03',MESSAGE='playback receipt capacity unavailable'; END IF;
    UPDATE playback_retention SET cutoff=greatest(cutoff,observed_at-interval '5 minutes') WHERE id;
    NEW.created_at:=clock_timestamp();
    NEW.admission_deadline:=NEW.created_at+interval '60 seconds';
    NEW.settled_at:=NULL; NEW.prune_after:=NULL;
    IF NEW.admission_fenced AND NEW.preparation_complete THEN
        NEW.settled_at:=NEW.created_at;
        NEW.prune_after:=greatest(NEW.settled_at+interval '24 hours',uuid_extract_timestamp(NEW.id)+interval '5 minutes 1 millisecond');
    END IF;
    RETURN NEW;
END
$$;
CREATE FUNCTION playback_start_intent_update() RETURNS TRIGGER
LANGUAGE plpgsql SET search_path=pg_catalog,public,pg_temp AS $$
BEGIN
    IF (NEW.id,NEW.household_id,NEW.session_id,NEW.credential_version,NEW.transport,NEW.profile_id,NEW.profile_revision,NEW.intent_hash,NEW.instance_id,NEW.created_at,NEW.admission_deadline)
       IS DISTINCT FROM (OLD.id,OLD.household_id,OLD.session_id,OLD.credential_version,OLD.transport,OLD.profile_id,OLD.profile_revision,OLD.intent_hash,OLD.instance_id,OLD.created_at,OLD.admission_deadline)
       OR (OLD.admission_fenced AND NOT NEW.admission_fenced) OR (OLD.preparation_complete AND NOT NEW.preparation_complete)
       OR (OLD.promoted AND NOT NEW.promoted) OR (OLD.settled_at IS NOT NULL AND NEW.settled_at IS DISTINCT FROM OLD.settled_at) THEN
        RAISE EXCEPTION USING ERRCODE='PBR04',MESSAGE='playback intent mutation refused';
    END IF;
    IF NEW.admission_fenced AND NEW.preparation_complete AND NOT EXISTS(SELECT 1 FROM playback_reservations WHERE id=NEW.id AND state<>'released') THEN
        NEW.settled_at:=coalesce(OLD.settled_at,clock_timestamp());
        NEW.prune_after:=greatest(NEW.settled_at+interval '24 hours',uuid_extract_timestamp(NEW.id)+interval '5 minutes 1 millisecond');
    ELSE NEW.settled_at:=NULL; NEW.prune_after:=NULL;
    END IF;
    RETURN NEW;
END
$$;
CREATE FUNCTION playback_start_intent_delete() RETURNS TRIGGER
LANGUAGE plpgsql SET search_path=pg_catalog,public,pg_temp AS $$
DECLARE floor_at TIMESTAMPTZ;
BEGIN
    IF TG_OP='TRUNCATE' THEN RAISE EXCEPTION USING ERRCODE='PBR04',MESSAGE='playback intent truncation refused'; END IF;
    SELECT cutoff INTO STRICT floor_at FROM playback_retention WHERE id FOR UPDATE;
    IF OLD.settled_at IS NULL OR OLD.prune_after>clock_timestamp() OR uuid_extract_timestamp(OLD.id)>=floor_at
       OR EXISTS(SELECT 1 FROM playback_reservations WHERE id=OLD.id) THEN
        RAISE EXCEPTION USING ERRCODE='PBR04',MESSAGE='playback intent deletion refused';
    END IF;
    RETURN OLD;
END
$$;
CREATE TRIGGER playback_start_intent_insert BEFORE INSERT ON playback_start_intents FOR EACH ROW EXECUTE FUNCTION playback_start_intent_insert();
CREATE TRIGGER playback_start_intent_update BEFORE UPDATE ON playback_start_intents FOR EACH ROW EXECUTE FUNCTION playback_start_intent_update();
CREATE TRIGGER playback_start_intent_delete BEFORE DELETE ON playback_start_intents FOR EACH ROW EXECUTE FUNCTION playback_start_intent_delete();
CREATE TRIGGER playback_start_intent_truncate BEFORE TRUNCATE ON playback_start_intents FOR EACH STATEMENT EXECUTE FUNCTION playback_start_intent_delete();
CREATE OR REPLACE FUNCTION playback_receipt_insert() RETURNS TRIGGER
LANGUAGE plpgsql SET search_path=pg_catalog,public,pg_temp AS $$
DECLARE
    observed_at TIMESTAMPTZ;
    floor_at TIMESTAMPTZ;
    total BIGINT;
    household_total BIGINT;
    timing SMALLINT;
BEGIN
    IF EXISTS(SELECT 1 FROM playback_start_intents WHERE id=NEW.id) THEN
        PERFORM 1 FROM playback_start_intents i WHERE i.id=NEW.id AND i.household_id=NEW.household_id AND i.instance_id=NEW.instance_id
          AND NOT i.admission_fenced AND NOT i.promoted AND i.preparation_complete AND i.admission_deadline>clock_timestamp()
          AND (NEW.fingerprint::jsonb->>'session')::uuid=i.session_id AND (NEW.fingerprint::jsonb->>'version')::bigint=i.credential_version
          AND (NEW.fingerprint::jsonb->>'transport')::smallint=i.transport AND (NEW.fingerprint::jsonb->>'profile')::uuid=i.profile_id
          AND (NEW.fingerprint::jsonb->>'profile_revision')::bigint=i.profile_revision FOR UPDATE;
        IF NOT FOUND THEN RAISE EXCEPTION USING ERRCODE='PBR04',MESSAGE='playback start fenced'; END IF;
        UPDATE playback_start_intents SET promoted=true WHERE id=NEW.id;
        RETURN NEW;
    END IF;
    SELECT cutoff,retained_count INTO STRICT floor_at,total FROM playback_retention WHERE id FOR UPDATE;
    observed_at:=date_trunc('milliseconds',clock_timestamp());
    IF EXISTS(SELECT 1 FROM playback_reservations WHERE id=NEW.id) THEN RAISE EXCEPTION USING ERRCODE='PBR04',MESSAGE='playback request identity conflicts'; END IF;
    timing:=playback_receipt_time_error(NEW.id,observed_at,floor_at);
    IF timing=1 THEN RAISE EXCEPTION USING ERRCODE='PBR01',MESSAGE='playback request expired'; END IF;
    IF timing=2 THEN RAISE EXCEPTION USING ERRCODE='PBR02',MESSAGE='playback request time unavailable'; END IF;
    IF timing<>0 THEN RAISE EXCEPTION USING ERRCODE='PBR04',MESSAGE='playback request identity invalid'; END IF;
    IF total>=65536 THEN RAISE EXCEPTION USING ERRCODE='PBR03',MESSAGE='playback receipt capacity unavailable'; END IF;
    SELECT retained_count INTO household_total FROM playback_receipt_households WHERE household_id=NEW.household_id FOR UPDATE;
    IF NOT FOUND THEN
        INSERT INTO playback_receipt_households(household_id,retained_count) VALUES (NEW.household_id,0);
        household_total:=0;
    END IF;
    IF household_total>=4096 THEN RAISE EXCEPTION USING ERRCODE='PBR03',MESSAGE='playback receipt capacity unavailable'; END IF;
    UPDATE playback_retention SET cutoff=greatest(cutoff,observed_at-interval '5 minutes') WHERE id;
    IF NEW.state='released' THEN
        NEW.released_at:=clock_timestamp();
        NEW.prune_after:=greatest(NEW.released_at+interval '24 hours',uuid_extract_timestamp(NEW.id)+interval '5 minutes 1 millisecond');
    ELSE NEW.prune_after:=NULL;
    END IF;
    RETURN NEW;
END
$$;

CREATE OR REPLACE FUNCTION playback_receipt_account() RETURNS TRIGGER
LANGUAGE plpgsql SET search_path=pg_catalog,public,pg_temp AS $$
DECLARE household_total BIGINT;
BEGIN
    IF TG_TABLE_NAME='playback_reservations' AND EXISTS(SELECT 1 FROM playback_start_intents WHERE id=coalesce(NEW.id,OLD.id)) THEN
        IF TG_OP='INSERT' THEN RETURN NEW; ELSE RETURN OLD; END IF;
    END IF;
    IF TG_OP='INSERT' THEN
        UPDATE playback_retention SET retained_count=retained_count+1 WHERE id AND retained_count<65536;
        IF NOT FOUND THEN RAISE EXCEPTION USING ERRCODE='PBR05',MESSAGE='playback receipt accounting unavailable'; END IF;
        UPDATE playback_receipt_households SET retained_count=retained_count+1 WHERE household_id=NEW.household_id AND retained_count<4096;
        IF NOT FOUND THEN RAISE EXCEPTION USING ERRCODE='PBR05',MESSAGE='playback receipt accounting unavailable'; END IF;
        RETURN NEW;
    END IF;
    UPDATE playback_retention SET retained_count=retained_count-1 WHERE id AND retained_count>0;
    IF NOT FOUND THEN RAISE EXCEPTION USING ERRCODE='PBR05',MESSAGE='playback receipt accounting unavailable'; END IF;
    UPDATE playback_receipt_households SET retained_count=retained_count-1 WHERE household_id=OLD.household_id AND retained_count>0 RETURNING retained_count INTO household_total;
    IF NOT FOUND THEN RAISE EXCEPTION USING ERRCODE='PBR05',MESSAGE='playback receipt accounting unavailable'; END IF;
    IF household_total=0 THEN DELETE FROM playback_receipt_households WHERE household_id=OLD.household_id; END IF;
    RETURN OLD;
END
$$;
CREATE TRIGGER playback_start_intent_account AFTER INSERT OR DELETE ON playback_start_intents FOR EACH ROW EXECUTE FUNCTION playback_receipt_account();
CREATE FUNCTION playback_start_parent_completion() RETURNS TRIGGER
LANGUAGE plpgsql SET search_path=pg_catalog,public,pg_temp AS $$
BEGIN
    IF NEW.state='released' THEN UPDATE playback_start_intents SET admission_fenced=true WHERE id=NEW.id; END IF;
    RETURN NEW;
END
$$;
CREATE TRIGGER playback_start_parent_completion AFTER UPDATE ON playback_reservations FOR EACH ROW EXECUTE FUNCTION playback_start_parent_completion();
CREATE FUNCTION playback_start_command_binding() RETURNS TRIGGER
LANGUAGE plpgsql SET search_path=pg_catalog,public,pg_temp AS $$
BEGIN
    IF EXISTS(SELECT 1 FROM playback_start_intents WHERE id=NEW.reservation_id AND (NOT promoted OR intent_hash IS DISTINCT FROM NEW.intent_hash)) THEN
        RAISE EXCEPTION USING ERRCODE='PBR04',MESSAGE='playback start intent mismatch';
    END IF;
    RETURN NEW;
END
$$;
CREATE TRIGGER playback_start_command_binding BEFORE INSERT ON playback_vod_commands FOR EACH ROW EXECUTE FUNCTION playback_start_command_binding();
UPDATE playback_start_intents i SET admission_fenced=true WHERE EXISTS(SELECT 1 FROM playback_reservations r WHERE r.id=i.id AND r.state='released');
CREATE FUNCTION playback_start_prune(budget INTEGER) RETURNS INTEGER
LANGUAGE plpgsql SET search_path=pg_catalog,public,pg_temp AS $$
DECLARE candidate UUID; removed INTEGER:=0; observed_at TIMESTAMPTZ:=clock_timestamp();
BEGIN
    IF budget<0 OR budget>128 THEN RAISE EXCEPTION USING ERRCODE='PBR04',MESSAGE='playback pruning budget invalid'; END IF;
    FOR candidate IN SELECT id FROM playback_start_intents WHERE settled_at IS NOT NULL AND prune_after<=observed_at ORDER BY prune_after,id LIMIT budget FOR UPDATE SKIP LOCKED LOOP
        IF EXISTS(SELECT 1 FROM playback_vod_operations WHERE reservation_id=candidate) THEN CONTINUE; END IF;
        DELETE FROM playback_reservations WHERE id=candidate AND state='released' AND prune_after<=observed_at;
        IF EXISTS(SELECT 1 FROM playback_reservations WHERE id=candidate) THEN CONTINUE; END IF;
        DELETE FROM playback_start_intents WHERE id=candidate;
        removed:=removed+1;
    END LOOP;
    RETURN removed;
END
$$;
CREATE FUNCTION playback_start_promotion_complete() RETURNS TRIGGER
LANGUAGE plpgsql SET search_path=pg_catalog,public,pg_temp AS $$
BEGIN
    IF EXISTS(SELECT 1 FROM playback_start_intents i WHERE i.id=NEW.id)
       AND NOT EXISTS(SELECT 1 FROM playback_start_intents i JOIN playback_vod_commands c ON c.reservation_id=i.id
           WHERE i.id=NEW.id AND i.promoted AND i.preparation_complete AND c.intent_hash=i.intent_hash AND c.protocol=2 AND NEW.kind='vod') THEN
        RAISE EXCEPTION USING ERRCODE='PBR04',MESSAGE='playback promotion binding incomplete';
    END IF;
    RETURN NEW;
END
$$;
CREATE CONSTRAINT TRIGGER playback_start_promotion_complete AFTER INSERT ON playback_reservations DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION playback_start_promotion_complete();

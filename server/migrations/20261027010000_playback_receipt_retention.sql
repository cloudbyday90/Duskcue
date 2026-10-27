-- Duskcue — Self-hosted media streaming server
-- Copyright (C) 2026-2026 Duskcue Contributors
-- Licensed under AGPL-3.0. See LICENSE for details.

LOCK TABLE playback_reservations IN ACCESS EXCLUSIVE MODE;
CREATE TABLE playback_retention (
    id BOOLEAN PRIMARY KEY DEFAULT true CHECK (id),
    cutoff TIMESTAMPTZ NOT NULL CHECK (isfinite(cutoff)),
    retained_count BIGINT NOT NULL CHECK (retained_count >= 0)
);
INSERT INTO playback_retention(id,cutoff,retained_count)
SELECT true,date_trunc('milliseconds',clock_timestamp())-interval '5 minutes',count(*) FROM playback_reservations;
CREATE TABLE playback_receipt_households (
    household_id UUID PRIMARY KEY,
    retention_id BOOLEAN NOT NULL DEFAULT true REFERENCES playback_retention(id) CHECK (retention_id),
    retained_count BIGINT NOT NULL CHECK (retained_count >= 0)
);
INSERT INTO playback_receipt_households(household_id,retained_count)
SELECT household_id,count(*) FROM playback_reservations GROUP BY household_id;
ALTER TABLE playback_reservations ADD COLUMN prune_after TIMESTAMPTZ;
UPDATE playback_reservations SET prune_after=greatest(released_at+interval '24 hours',uuid_extract_timestamp(id)+interval '5 minutes 1 millisecond') WHERE state='released';
ALTER TABLE playback_reservations
    ADD CONSTRAINT playback_receipt_household FOREIGN KEY (household_id) REFERENCES playback_receipt_households(household_id),
    ADD CONSTRAINT playback_receipt_eligibility CHECK ((state='released')=(prune_after IS NOT NULL));
CREATE INDEX playback_receipt_pruning ON playback_reservations(prune_after,id) WHERE state='released';
CREATE INDEX playback_receipt_household_rows ON playback_reservations(household_id);

CREATE FUNCTION playback_receipt_time_error(request_id UUID, observed_at TIMESTAMPTZ, floor_at TIMESTAMPTZ)
RETURNS SMALLINT LANGUAGE SQL STABLE SET search_path=pg_catalog,public,pg_temp AS $$
    SELECT CASE
        WHEN uuid_extract_version(request_id) IS DISTINCT FROM 7 OR observed_at IS NULL OR NOT isfinite(observed_at) OR floor_at IS NULL OR NOT isfinite(floor_at) THEN 3
        WHEN uuid_extract_timestamp(request_id)<greatest(floor_at,date_trunc('milliseconds',observed_at)-interval '5 minutes') THEN 1
        WHEN uuid_extract_timestamp(request_id)>date_trunc('milliseconds',observed_at)+interval '2 minutes' THEN 2
        ELSE 0 END::smallint
$$;

CREATE FUNCTION playback_retention_protect() RETURNS TRIGGER
LANGUAGE plpgsql SET search_path=pg_catalog,public,pg_temp AS $$
BEGIN
    IF TG_OP<>'UPDATE' THEN RAISE EXCEPTION USING ERRCODE='PBR04',MESSAGE='playback retention reset refused'; END IF;
    IF NEW.id IS DISTINCT FROM OLD.id OR NEW.cutoff<OLD.cutoff THEN
        RAISE EXCEPTION USING ERRCODE='PBR04',MESSAGE='playback retention regression refused';
    END IF;
    IF NEW.retained_count<>OLD.retained_count AND (pg_trigger_depth()<>2 OR abs(NEW.retained_count-OLD.retained_count)<>1) THEN
        RAISE EXCEPTION USING ERRCODE='PBR05',MESSAGE='playback receipt accounting mutation refused';
    END IF;
    RETURN NEW;
END
$$;
CREATE TRIGGER playback_retention_update BEFORE UPDATE OR DELETE ON playback_retention FOR EACH ROW EXECUTE FUNCTION playback_retention_protect();
CREATE TRIGGER playback_retention_truncate BEFORE TRUNCATE ON playback_retention FOR EACH STATEMENT EXECUTE FUNCTION playback_retention_protect();

CREATE FUNCTION playback_household_accounting_protect() RETURNS TRIGGER
LANGUAGE plpgsql SET search_path=pg_catalog,public,pg_temp AS $$
BEGIN
    IF TG_OP='TRUNCATE' OR pg_trigger_depth()<>2 THEN
        RAISE EXCEPTION USING ERRCODE='PBR05',MESSAGE='playback receipt accounting mutation refused';
    END IF;
    IF TG_OP='INSERT' THEN
        IF NEW.retained_count<>0 THEN RAISE EXCEPTION USING ERRCODE='PBR05',MESSAGE='playback receipt accounting mutation refused'; END IF;
        RETURN NEW;
    ELSIF TG_OP='DELETE' THEN
        IF OLD.retained_count<>0 THEN RAISE EXCEPTION USING ERRCODE='PBR05',MESSAGE='playback receipt accounting mutation refused'; END IF;
        RETURN OLD;
    END IF;
    IF NEW.household_id IS DISTINCT FROM OLD.household_id OR NEW.retention_id IS DISTINCT FROM OLD.retention_id OR abs(NEW.retained_count-OLD.retained_count)<>1 THEN
        RAISE EXCEPTION USING ERRCODE='PBR05',MESSAGE='playback receipt accounting mutation refused';
    END IF;
    RETURN NEW;
END
$$;
CREATE TRIGGER playback_household_accounting BEFORE INSERT OR UPDATE OR DELETE ON playback_receipt_households FOR EACH ROW EXECUTE FUNCTION playback_household_accounting_protect();
CREATE TRIGGER playback_household_truncate BEFORE TRUNCATE ON playback_receipt_households FOR EACH STATEMENT EXECUTE FUNCTION playback_household_accounting_protect();

CREATE FUNCTION playback_receipt_insert() RETURNS TRIGGER
LANGUAGE plpgsql SET search_path=pg_catalog,public,pg_temp AS $$
DECLARE
    observed_at TIMESTAMPTZ;
    floor_at TIMESTAMPTZ;
    total BIGINT;
    household_total BIGINT;
    timing SMALLINT;
BEGIN
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

CREATE FUNCTION playback_receipt_update() RETURNS TRIGGER
LANGUAGE plpgsql SET search_path=pg_catalog,public,pg_temp AS $$
BEGIN
    IF (NEW.id,NEW.household_id,NEW.instance_id,NEW.fingerprint,NEW.kind,NEW.assignment_id,NEW.source_generation,NEW.channel_key,NEW.created_at)
       IS DISTINCT FROM (OLD.id,OLD.household_id,OLD.instance_id,OLD.fingerprint,OLD.kind,OLD.assignment_id,OLD.source_generation,OLD.channel_key,OLD.created_at)
       OR (OLD.state='released' AND NEW.state<>'released') OR (OLD.state='retiring' AND NEW.state='held')
       OR (OLD.released_at IS NOT NULL AND NEW.released_at IS DISTINCT FROM OLD.released_at) THEN
        RAISE EXCEPTION USING ERRCODE='PBR04',MESSAGE='playback receipt mutation refused';
    END IF;
    IF NEW.state='released' THEN
        IF OLD.state<>'released' THEN NEW.released_at:=clock_timestamp(); END IF;
        NEW.prune_after:=greatest(NEW.released_at+interval '24 hours',uuid_extract_timestamp(NEW.id)+interval '5 minutes 1 millisecond');
    ELSE NEW.prune_after:=NULL;
    END IF;
    RETURN NEW;
END
$$;

CREATE FUNCTION playback_receipt_delete() RETURNS TRIGGER
LANGUAGE plpgsql SET search_path=pg_catalog,public,pg_temp AS $$
DECLARE
    floor_at TIMESTAMPTZ;
    observed_at TIMESTAMPTZ:=date_trunc('milliseconds',clock_timestamp());
BEGIN
    IF TG_OP='TRUNCATE' THEN RAISE EXCEPTION USING ERRCODE='PBR04',MESSAGE='playback receipt truncation refused'; END IF;
    SELECT cutoff INTO STRICT floor_at FROM playback_retention WHERE id FOR UPDATE;
    IF OLD.state<>'released' OR OLD.released_at IS NULL OR OLD.released_at+interval '24 hours'>observed_at
       OR OLD.prune_after IS NULL OR OLD.prune_after>observed_at OR uuid_extract_timestamp(OLD.id)>=floor_at THEN
        RAISE EXCEPTION USING ERRCODE='PBR04',MESSAGE='playback receipt deletion refused';
    END IF;
    RETURN OLD;
END
$$;

CREATE FUNCTION playback_receipt_account() RETURNS TRIGGER
LANGUAGE plpgsql SET search_path=pg_catalog,public,pg_temp AS $$
DECLARE household_total BIGINT;
BEGIN
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
CREATE TRIGGER playback_receipt_insert BEFORE INSERT ON playback_reservations FOR EACH ROW EXECUTE FUNCTION playback_receipt_insert();
CREATE TRIGGER playback_receipt_update BEFORE UPDATE ON playback_reservations FOR EACH ROW EXECUTE FUNCTION playback_receipt_update();
CREATE TRIGGER playback_receipt_delete BEFORE DELETE ON playback_reservations FOR EACH ROW EXECUTE FUNCTION playback_receipt_delete();
CREATE TRIGGER playback_receipt_truncate BEFORE TRUNCATE ON playback_reservations FOR EACH STATEMENT EXECUTE FUNCTION playback_receipt_delete();
CREATE TRIGGER playback_receipt_account AFTER INSERT OR DELETE ON playback_reservations FOR EACH ROW EXECUTE FUNCTION playback_receipt_account();

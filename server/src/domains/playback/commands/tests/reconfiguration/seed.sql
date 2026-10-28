-- Duskcue — Self-hosted media streaming server
-- Copyright (C) 2026-2026 Duskcue Contributors
-- Licensed under AGPL-3.0. See LICENSE for details.

CREATE FUNCTION qualification_vod_seed(template UUID,household UUID,amount INTEGER,existing UUID DEFAULT NULL) RETURNS UUID LANGUAGE plpgsql AS $$
DECLARE r JSONB; c JSONB; o JSONB; viewer UUID; initial UUID; history UUID:=uuidv7(); operation UUID; candidate UUID; n INTEGER; stamp TIMESTAMPTZ:=clock_timestamp();
BEGIN
    IF amount NOT BETWEEN 0 AND 256 THEN RAISE EXCEPTION 'fixture receipt bound'; END IF;
    SELECT to_jsonb(x) INTO STRICT r FROM playback_reservations x WHERE id=template;
    SELECT to_jsonb(x) INTO STRICT c FROM playback_vod_commands x WHERE reservation_id=template;
    SELECT to_jsonb(x) INTO STRICT o FROM playback_vod_outputs x WHERE reservation_id=template AND operation_id IS NULL;
    IF existing IS NULL THEN
        viewer:=uuidv7();initial:=uuidv7();
        INSERT INTO playback_reservations(id,instance_id,household_id,fingerprint,kind) VALUES(viewer,(r->>'instance_id')::uuid,household,r->>'fingerprint','vod');
        INSERT INTO playback_vod_commands SELECT (jsonb_populate_record(NULL::playback_vod_commands,c||jsonb_build_object('reservation_id',viewer,'history_id',history,'generation_id',NULL,'result',(c->'result')||jsonb_build_object('session_id',history,'output_revision',initial),'resources_completed',false,'cleanup_uncertain',false))).*;
        INSERT INTO playback_vod_outputs SELECT (jsonb_populate_record(NULL::playback_vod_outputs,o||jsonb_build_object('id',initial,'reservation_id',viewer,'generation_id',NULL,'result',(o->'result')||jsonb_build_object('session_id',history,'output_revision',initial),'state','ready','resources_completed',false,'cleanup_uncertain',false,'completed_at',NULL,'published_at',stamp))).*;
        INSERT INTO playback_vod_controls(reservation_id,current_output,claim_encodes,state) VALUES(viewer,initial,false,'ready');
    ELSE
        viewer:=existing;
        SELECT current_output INTO STRICT initial FROM playback_vod_controls WHERE reservation_id=viewer;
    END IF;
    FOR n IN 1..amount LOOP
        operation:=uuidv7();candidate:=uuidv7();
        INSERT INTO playback_vod_operations(id,reservation_id,household_id,intent_hash,condition,action,expected_output,candidate_output,state,created_at,terminal_at,cleanup_at,prune_after)
            VALUES(operation,viewer,household,decode(repeat('11',32),'hex'),'"fixture"','seek',initial,candidate,'cancelled',stamp,stamp,stamp,stamp+interval '24 hours');
        INSERT INTO playback_vod_outputs SELECT (jsonb_populate_record(NULL::playback_vod_outputs,o||jsonb_build_object('id',candidate,'reservation_id',viewer,'operation_id',operation,'generation_id',NULL,'result',NULL,'published_at',NULL,'state','complete','resources_completed',true,'completed_at',stamp,'cleanup_uncertain',false))).*;
    END LOOP;
    UPDATE playback_vod_controls SET retained_operations=(SELECT count(*) FROM playback_vod_operations WHERE reservation_id=viewer) WHERE reservation_id=viewer;
    UPDATE playback_vod_operation_retention SET retained_count=(SELECT count(*) FROM playback_vod_operations) WHERE id;
    IF existing IS NULL AND amount>0 THEN
        PERFORM qualification_vod_finish(viewer);
    END IF;
    RETURN viewer;
END;
$$;
CREATE FUNCTION qualification_vod_finish(viewer UUID) RETURNS VOID LANGUAGE plpgsql AS $$
BEGIN
    UPDATE playback_vod_outputs SET state='complete',resources_completed=true,completed_at=COALESCE(completed_at,clock_timestamp()) WHERE reservation_id=viewer AND NOT resources_completed;
    PERFORM playback_vod_complete_outputs(viewer);
    UPDATE playback_vod_commands SET resources_completed=true,state='stopping' WHERE reservation_id=viewer;
    UPDATE playback_reservations SET state='retiring' WHERE id=viewer;
    UPDATE playback_reservations SET state='released' WHERE id=viewer;
END;
$$;

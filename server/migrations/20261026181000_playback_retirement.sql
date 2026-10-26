-- Duskcue — Self-hosted media streaming server
-- Copyright (C) 2026-2026 Duskcue Contributors
-- Licensed under AGPL-3.0. See LICENSE for details.

ALTER TABLE playback_reservations
    ADD COLUMN session_id UUID GENERATED ALWAYS AS ((fingerprint::jsonb->>'session')::uuid) STORED NOT NULL,
    ADD COLUMN profile_id UUID GENERATED ALWAYS AS ((fingerprint::jsonb->>'profile')::uuid) STORED NOT NULL,
    ADD COLUMN vod_item_id UUID GENERATED ALWAYS AS ((fingerprint::jsonb#>>'{subject,Vod,item}')::uuid) STORED,
    ADD COLUMN vod_file_id UUID GENERATED ALWAYS AS ((fingerprint::jsonb#>>'{subject,Vod,file}')::uuid) STORED;

CREATE INDEX playback_reservations_item ON playback_reservations(vod_item_id) WHERE kind='vod' AND state='held';
CREATE INDEX playback_reservations_file ON playback_reservations(vod_file_id) WHERE kind='vod' AND state='held';

CREATE FUNCTION retire_playback_viewers(household UUID, profile UUID, media_kind TEXT)
RETURNS VOID LANGUAGE plpgsql AS $$
BEGIN
    PERFORM id FROM users WHERE id=household FOR NO KEY UPDATE;
    UPDATE playback_reservations SET state='retiring'
    WHERE household_id=household AND state='held'
      AND (profile IS NULL OR profile_id=profile)
      AND (media_kind IS NULL OR kind=media_kind);
END;
$$;

CREATE FUNCTION playback_actor_retirement() RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    IF TG_TABLE_NAME='users' THEN
        IF TG_OP='UPDATE' THEN
            IF OLD.status IS NOT DISTINCT FROM NEW.status AND OLD.deleted_at IS NOT DISTINCT FROM NEW.deleted_at AND OLD.role IS NOT DISTINCT FROM NEW.role THEN
                PERFORM retire_playback_viewers(OLD.id,NULL,'vod');
                RETURN NULL;
            END IF;
        END IF;
        PERFORM retire_playback_viewers(OLD.id,NULL,NULL);
    ELSIF TG_TABLE_NAME='user_profiles' THEN
        PERFORM retire_playback_viewers(OLD.owner_user_id,OLD.id,NULL);
    ELSIF TG_TABLE_NAME='user_sessions' THEN
        UPDATE playback_reservations SET state='retiring'
        WHERE household_id=OLD.user_id AND session_id=OLD.id AND state='held';
    ELSIF TG_TABLE_NAME='user_capabilities' THEN
        IF TG_OP='UPDATE' THEN
            IF OLD.user_id=NEW.user_id AND OLD.capability=NEW.capability AND OLD.is_granted=NEW.is_granted THEN RETURN NULL; END IF;
        END IF;
        IF TG_OP<>'INSERT' THEN
            IF OLD.capability IN ('play_media','can_use_live_tv') THEN
                PERFORM retire_playback_viewers(OLD.user_id,NULL,CASE OLD.capability WHEN 'play_media' THEN 'vod' ELSE 'live' END);
            END IF;
        END IF;
        IF TG_OP<>'DELETE' THEN
            IF NEW.capability IN ('play_media','can_use_live_tv') THEN
                PERFORM retire_playback_viewers(NEW.user_id,NULL,CASE NEW.capability WHEN 'play_media' THEN 'vod' ELSE 'live' END);
            END IF;
        END IF;
    ELSIF TG_TABLE_NAME='profile_library_access' THEN
        IF TG_OP<>'INSERT' THEN
            PERFORM retire_playback_viewers(p.owner_user_id,p.id,'vod') FROM user_profiles p WHERE p.id=OLD.profile_id;
        END IF;
        IF TG_OP<>'DELETE' THEN
            PERFORM retire_playback_viewers(p.owner_user_id,p.id,'vod') FROM user_profiles p WHERE p.id=NEW.profile_id;
        END IF;
    ELSIF TG_TABLE_NAME='user_library_access' THEN
        IF TG_OP<>'INSERT' THEN PERFORM retire_playback_viewers(OLD.user_id,NULL,'vod'); END IF;
        IF TG_OP<>'DELETE' THEN PERFORM retire_playback_viewers(NEW.user_id,NULL,'vod'); END IF;
    ELSIF TG_TABLE_NAME='live_tv_profile_access' THEN
        PERFORM retire_playback_viewers(OLD.household_user_id,OLD.profile_id,'live');
    ELSIF TG_TABLE_NAME='live_tv_assignments' THEN
        PERFORM retire_playback_viewers(OLD.household_user_id,NULL,'live');
    END IF;
    IF TG_OP='DELETE' THEN RETURN OLD; END IF;
    RETURN NULL;
END;
$$;

CREATE TRIGGER playback_user_changed AFTER UPDATE ON users FOR EACH ROW
WHEN (OLD.status IS DISTINCT FROM NEW.status OR OLD.deleted_at IS DISTINCT FROM NEW.deleted_at
      OR OLD.role IS DISTINCT FROM NEW.role OR OLD.has_all_library_access IS DISTINCT FROM NEW.has_all_library_access)
EXECUTE FUNCTION playback_actor_retirement();
CREATE TRIGGER playback_user_deleted BEFORE DELETE ON users FOR EACH ROW EXECUTE FUNCTION playback_actor_retirement();
CREATE TRIGGER playback_profile_changed AFTER UPDATE ON user_profiles FOR EACH ROW
WHEN (OLD.profile_type IS DISTINCT FROM NEW.profile_type OR OLD.max_content_rating IS DISTINCT FROM NEW.max_content_rating
      OR OLD.owner_user_id IS DISTINCT FROM NEW.owner_user_id)
EXECUTE FUNCTION playback_actor_retirement();
CREATE TRIGGER playback_profile_deleted BEFORE DELETE ON user_profiles FOR EACH ROW EXECUTE FUNCTION playback_actor_retirement();
CREATE TRIGGER playback_session_changed AFTER UPDATE ON user_sessions FOR EACH ROW
WHEN (OLD.credential_version IS DISTINCT FROM NEW.credential_version OR OLD.profile_context_revision IS DISTINCT FROM NEW.profile_context_revision
      OR NEW.expires_at<OLD.expires_at OR OLD.user_id IS DISTINCT FROM NEW.user_id)
EXECUTE FUNCTION playback_actor_retirement();
CREATE TRIGGER playback_session_deleted BEFORE DELETE ON user_sessions FOR EACH ROW EXECUTE FUNCTION playback_actor_retirement();
CREATE TRIGGER playback_capability_changed AFTER INSERT OR UPDATE OR DELETE ON user_capabilities FOR EACH ROW EXECUTE FUNCTION playback_actor_retirement();
CREATE TRIGGER playback_profile_libraries_changed AFTER INSERT OR UPDATE OR DELETE ON profile_library_access FOR EACH ROW EXECUTE FUNCTION playback_actor_retirement();
CREATE TRIGGER playback_user_libraries_changed AFTER INSERT OR UPDATE OR DELETE ON user_library_access FOR EACH ROW EXECUTE FUNCTION playback_actor_retirement();
CREATE TRIGGER playback_live_access_changed AFTER UPDATE ON live_tv_profile_access FOR EACH ROW
WHEN (OLD.enabled AND (NOT NEW.enabled OR OLD.profile_id IS DISTINCT FROM NEW.profile_id)) EXECUTE FUNCTION playback_actor_retirement();
CREATE TRIGGER playback_live_access_deleted BEFORE DELETE ON live_tv_profile_access FOR EACH ROW EXECUTE FUNCTION playback_actor_retirement();
CREATE TRIGGER playback_source_changed AFTER UPDATE ON live_tv_assignments FOR EACH ROW
WHEN (OLD.source_generation IS DISTINCT FROM NEW.source_generation OR OLD.household_user_id IS DISTINCT FROM NEW.household_user_id
      OR (OLD.enabled AND NOT NEW.enabled) OR (OLD.playlist_destination IS NOT NULL AND NEW.playlist_destination IS NULL)
      OR (OLD.guide_destination IS NOT NULL AND NEW.guide_destination IS NULL)) EXECUTE FUNCTION playback_actor_retirement();
CREATE TRIGGER playback_source_deleted BEFORE DELETE ON live_tv_assignments FOR EACH ROW EXECUTE FUNCTION playback_actor_retirement();

CREATE FUNCTION playback_catalog_retirement() RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    PERFORM id FROM users WHERE id=NEW.household_user_id FOR NO KEY UPDATE;
    UPDATE playback_reservations r SET state='retiring'
    WHERE r.household_id=NEW.household_user_id AND r.assignment_id=NEW.assignment_id AND r.kind='live' AND r.state='held'
      AND NOT EXISTS (SELECT 1 FROM live_tv_catalog_channels c WHERE c.snapshot_id=NEW.current_snapshot_id
          AND c.household_user_id=r.household_id AND c.assignment_id=r.assignment_id AND c.channel_key=r.channel_key);
    IF TG_OP='DELETE' THEN RETURN OLD; END IF;
    RETURN NULL;
END;
$$;
CREATE TRIGGER playback_catalog_changed AFTER UPDATE OF current_snapshot_id ON live_tv_import_state FOR EACH ROW
WHEN (OLD.current_snapshot_id IS DISTINCT FROM NEW.current_snapshot_id) EXECUTE FUNCTION playback_catalog_retirement();

CREATE FUNCTION playback_content_retirement() RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    IF TG_TABLE_NAME='libraries' THEN
        UPDATE playback_reservations r SET state='retiring' WHERE r.kind='vod' AND r.state='held'
          AND r.vod_item_id IN (SELECT id FROM media_items WHERE library_id=OLD.id);
    ELSIF TG_TABLE_NAME='media_items' THEN
        UPDATE playback_reservations r SET state='retiring' WHERE r.kind='vod' AND r.state='held'
          AND r.vod_item_id=OLD.id;
    ELSE
        UPDATE playback_reservations r SET state='retiring' WHERE r.kind='vod' AND r.state='held'
          AND r.vod_file_id=OLD.id;
    END IF;
    IF TG_OP='DELETE' THEN RETURN OLD; END IF;
    RETURN NULL;
END;
$$;
CREATE TRIGGER playback_library_changed AFTER UPDATE ON libraries FOR EACH ROW
WHEN (OLD.deleted_at IS DISTINCT FROM NEW.deleted_at) EXECUTE FUNCTION playback_content_retirement();
CREATE TRIGGER playback_library_deleted BEFORE DELETE ON libraries FOR EACH ROW EXECUTE FUNCTION playback_content_retirement();
CREATE TRIGGER playback_item_changed AFTER UPDATE ON media_items FOR EACH ROW
WHEN (OLD.library_id IS DISTINCT FROM NEW.library_id OR OLD.content_rating IS DISTINCT FROM NEW.content_rating)
EXECUTE FUNCTION playback_content_retirement();
CREATE TRIGGER playback_item_deleted BEFORE DELETE ON media_items FOR EACH ROW EXECUTE FUNCTION playback_content_retirement();
CREATE TRIGGER playback_file_changed AFTER UPDATE ON media_files FOR EACH ROW
WHEN (OLD.media_item_id IS DISTINCT FROM NEW.media_item_id OR OLD.file_path IS DISTINCT FROM NEW.file_path) EXECUTE FUNCTION playback_content_retirement();
CREATE TRIGGER playback_file_deleted BEFORE DELETE ON media_files FOR EACH ROW EXECUTE FUNCTION playback_content_retirement();

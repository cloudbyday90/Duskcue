CREATE OR REPLACE FUNCTION audit_trigger_fn()
RETURNS TRIGGER
LANGUAGE plpgsql
SECURITY DEFINER
AS $$
DECLARE
    v_old_data JSONB;
    v_new_data JSONB;
    v_changed TEXT[];
    v_row_id UUID;
    v_user_id UUID;
BEGIN
    BEGIN
        v_user_id := current_setting('app.current_user_id', TRUE)::UUID;
    EXCEPTION WHEN OTHERS THEN
        v_user_id := NULL;
    END;

    IF TG_OP = 'INSERT' THEN
        v_new_data := to_jsonb(NEW);
        v_old_data := NULL;
        v_row_id := NEW.id;
        v_changed := NULL;
    ELSIF TG_OP = 'UPDATE' THEN
        v_old_data := to_jsonb(OLD);
        v_new_data := to_jsonb(NEW);
        v_row_id := NEW.id;

        SELECT array_agg(key ORDER BY key) INTO v_changed
        FROM (
            SELECT key
            FROM jsonb_each(v_new_data) n
            WHERE n.value IS DISTINCT FROM (v_old_data -> n.key)
        ) changed;
    ELSIF TG_OP = 'DELETE' THEN
        v_old_data := to_jsonb(OLD);
        v_new_data := NULL;
        v_row_id := OLD.id;
        v_changed := NULL;
    END IF;

    IF TG_OP = 'UPDATE' AND (v_changed IS NULL OR array_length(v_changed, 1) = 0) THEN
        RETURN NULL;
    END IF;

    v_old_data := v_old_data
        - 'password_hash' - 'access_token' - 'refresh_token'
        - 'secret' - 'key_hash' - 'token_hash'
        - 'backup_codes';
    v_new_data := v_new_data
        - 'password_hash' - 'access_token' - 'refresh_token'
        - 'secret' - 'key_hash' - 'token_hash'
        - 'backup_codes';

    INSERT INTO audit_log (
        table_name, row_id, operation,
        old_data, new_data, changed_fields,
        user_id, db_user, client_addr, application_name
    ) VALUES (
        TG_TABLE_NAME, v_row_id, TG_OP,
        v_old_data, v_new_data, v_changed,
        v_user_id, session_user,
        inet_client_addr(),
        current_setting('application_name', TRUE)
    );

    RETURN NULL;
END;
$$;

DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_trigger WHERE tgname = 'audit_users') THEN
        CREATE TRIGGER audit_users
            AFTER INSERT OR UPDATE OR DELETE ON users
            FOR EACH ROW EXECUTE FUNCTION audit_trigger_fn();
    END IF;

    IF NOT EXISTS (SELECT 1 FROM pg_trigger WHERE tgname = 'audit_user_passkeys') THEN
        CREATE TRIGGER audit_user_passkeys
            AFTER INSERT OR UPDATE OR DELETE ON user_passkeys
            FOR EACH ROW EXECUTE FUNCTION audit_trigger_fn();
    END IF;

    IF NOT EXISTS (SELECT 1 FROM pg_trigger WHERE tgname = 'audit_user_totp') THEN
        CREATE TRIGGER audit_user_totp
            AFTER INSERT OR UPDATE OR DELETE ON user_totp
            FOR EACH ROW EXECUTE FUNCTION audit_trigger_fn();
    END IF;

    IF NOT EXISTS (SELECT 1 FROM pg_trigger WHERE tgname = 'audit_user_capabilities') THEN
        CREATE TRIGGER audit_user_capabilities
            AFTER INSERT OR UPDATE OR DELETE ON user_capabilities
            FOR EACH ROW EXECUTE FUNCTION audit_trigger_fn();
    END IF;

    IF NOT EXISTS (SELECT 1 FROM pg_trigger WHERE tgname = 'audit_user_library_access') THEN
        CREATE TRIGGER audit_user_library_access
            AFTER INSERT OR UPDATE OR DELETE ON user_library_access
            FOR EACH ROW EXECUTE FUNCTION audit_trigger_fn();
    END IF;

    IF NOT EXISTS (SELECT 1 FROM pg_trigger WHERE tgname = 'audit_api_keys') THEN
        CREATE TRIGGER audit_api_keys
            AFTER INSERT OR UPDATE OR DELETE ON api_keys
            FOR EACH ROW EXECUTE FUNCTION audit_trigger_fn();
    END IF;

    IF NOT EXISTS (SELECT 1 FROM pg_trigger WHERE tgname = 'audit_invitations') THEN
        CREATE TRIGGER audit_invitations
            AFTER INSERT OR UPDATE OR DELETE ON invitations
            FOR EACH ROW EXECUTE FUNCTION audit_trigger_fn();
    END IF;

    IF NOT EXISTS (SELECT 1 FROM pg_trigger WHERE tgname = 'audit_server_config') THEN
        CREATE TRIGGER audit_server_config
            AFTER INSERT OR UPDATE ON server_config
            FOR EACH ROW EXECUTE FUNCTION audit_trigger_fn();
    END IF;

    IF NOT EXISTS (SELECT 1 FROM pg_trigger WHERE tgname = 'audit_scheduled_tasks') THEN
        CREATE TRIGGER audit_scheduled_tasks
            AFTER INSERT OR UPDATE ON scheduled_tasks
            FOR EACH ROW EXECUTE FUNCTION audit_trigger_fn();
    END IF;

    IF NOT EXISTS (SELECT 1 FROM pg_trigger WHERE tgname = 'audit_libraries') THEN
        CREATE TRIGGER audit_libraries
            AFTER INSERT OR UPDATE OR DELETE ON libraries
            FOR EACH ROW EXECUTE FUNCTION audit_trigger_fn();
    END IF;
END $$;

CREATE EXTENSION IF NOT EXISTS pg_trgm;
CREATE EXTENSION IF NOT EXISTS pgstattuple;

CREATE TABLE IF NOT EXISTS audit_log (
    id UUID DEFAULT uuidv7(),
    created_at TIMESTAMPTZ GENERATED ALWAYS AS (uuid_extract_timestamp(id)) STORED,

    table_name TEXT NOT NULL,
    row_id UUID NOT NULL,
    operation TEXT NOT NULL CHECK (operation IN ('INSERT', 'UPDATE', 'DELETE')),

    old_data JSONB,
    new_data JSONB,
    changed_fields TEXT[],

    user_id UUID,
    db_user TEXT NOT NULL DEFAULT current_user,
    client_addr INET,
    application_name TEXT,

    changed_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    transaction_id BIGINT NOT NULL DEFAULT txid_current()
) PARTITION BY RANGE (changed_at);

CREATE TABLE IF NOT EXISTS audit_log_2026_06 PARTITION OF audit_log
    FOR VALUES FROM ('2026-06-01') TO ('2026-07-01');

CREATE TABLE IF NOT EXISTS audit_log_2026_07 PARTITION OF audit_log
    FOR VALUES FROM ('2026-07-01') TO ('2026-08-01');

CREATE INDEX IF NOT EXISTS idx_audit_log_table_row ON audit_log (table_name, row_id, changed_at DESC);
CREATE INDEX IF NOT EXISTS idx_audit_log_id ON audit_log (id);
CREATE INDEX IF NOT EXISTS idx_audit_log_user ON audit_log (user_id, changed_at DESC) WHERE user_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_audit_log_time ON audit_log (changed_at DESC);
CREATE INDEX IF NOT EXISTS idx_audit_log_transaction ON audit_log (transaction_id);

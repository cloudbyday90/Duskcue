ALTER TABLE migration_import_log
    ADD COLUMN IF NOT EXISTS import_batch_id UUID;

ALTER TABLE migration_import_log
    ADD COLUMN IF NOT EXISTS previous_user_item_data JSONB;

ALTER TABLE migration_import_log
    ADD COLUMN IF NOT EXISTS imported_at TIMESTAMPTZ;

ALTER TABLE migration_import_log
    ADD COLUMN IF NOT EXISTS rolled_back_at TIMESTAMPTZ;

ALTER TABLE migration_import_log
    ADD COLUMN IF NOT EXISTS rollback_detail TEXT;

ALTER TABLE migration_import_log
    DROP CONSTRAINT IF EXISTS migration_import_log_status_check;

ALTER TABLE migration_import_log
    ADD CONSTRAINT migration_import_log_status_check CHECK (status IN (
        'discovered',
        'matched',
        'unmatched',
        'imported',
        'rolled_back',
        'skipped',
        'error'
    ));

CREATE INDEX IF NOT EXISTS idx_migration_import_log_import_batch
    ON migration_import_log (migration_source_id, import_batch_id)
    WHERE import_batch_id IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_migration_import_log_rollback
    ON migration_import_log (migration_source_id, status, imported_at)
    WHERE imported_user_item_data_id IS NOT NULL;

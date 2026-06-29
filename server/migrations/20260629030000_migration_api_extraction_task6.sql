ALTER TABLE migration_import_log
    ADD COLUMN IF NOT EXISTS source_is_watched BOOLEAN NOT NULL DEFAULT FALSE;

ALTER TABLE migration_import_log
    ADD COLUMN IF NOT EXISTS source_play_count INT NOT NULL DEFAULT 0;

ALTER TABLE migration_import_log
    ADD COLUMN IF NOT EXISTS source_resume_position_ms BIGINT NOT NULL DEFAULT 0;

ALTER TABLE migration_import_log
    ADD COLUMN IF NOT EXISTS source_last_played_at TIMESTAMPTZ;

ALTER TABLE migration_import_log
    ADD COLUMN IF NOT EXISTS source_item_metadata JSONB NOT NULL DEFAULT '{}';

ALTER TABLE migration_import_log
    DROP CONSTRAINT IF EXISTS migration_import_log_status_check;

ALTER TABLE migration_import_log
    ADD CONSTRAINT migration_import_log_status_check CHECK (status IN (
        'discovered',
        'matched',
        'unmatched',
        'imported',
        'skipped',
        'error'
    ));

ALTER TABLE migration_import_log
    DROP CONSTRAINT IF EXISTS migration_import_log_source_play_count_check;

ALTER TABLE migration_import_log
    ADD CONSTRAINT migration_import_log_source_play_count_check CHECK (source_play_count >= 0);

ALTER TABLE migration_import_log
    DROP CONSTRAINT IF EXISTS migration_import_log_source_resume_position_check;

ALTER TABLE migration_import_log
    ADD CONSTRAINT migration_import_log_source_resume_position_check CHECK (source_resume_position_ms >= 0);

CREATE INDEX IF NOT EXISTS idx_migration_import_log_source_status
    ON migration_import_log (migration_source_id, status);

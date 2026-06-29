ALTER TABLE migration_user_mapping
    ALTER COLUMN platform_user_id DROP NOT NULL;

ALTER TABLE migration_user_mapping
    DROP CONSTRAINT IF EXISTS migration_user_mapping_status_check;

ALTER TABLE migration_user_mapping
    ADD CONSTRAINT migration_user_mapping_status_check CHECK (status IN (
        'pending',
        'skipped',
        'imported',
        'failed'
    ));

ALTER TABLE migration_user_mapping
    DROP CONSTRAINT IF EXISTS migration_user_mapping_platform_user_required_check;

ALTER TABLE migration_user_mapping
    ADD CONSTRAINT migration_user_mapping_platform_user_required_check CHECK (
        (status = 'skipped' AND platform_user_id IS NULL)
        OR (status <> 'skipped' AND platform_user_id IS NOT NULL)
    );

CREATE UNIQUE INDEX IF NOT EXISTS idx_migration_user_mapping_platform_unique
    ON migration_user_mapping (migration_source_id, platform_user_id)
    WHERE platform_user_id IS NOT NULL;

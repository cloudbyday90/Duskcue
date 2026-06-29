ALTER TABLE migration_import_log
    ADD COLUMN IF NOT EXISTS match_confidence TEXT;

ALTER TABLE migration_import_log
    DROP CONSTRAINT IF EXISTS migration_import_log_match_method_check;

ALTER TABLE migration_import_log
    ADD CONSTRAINT migration_import_log_match_method_check CHECK (
        match_method IN ('tmdb_id', 'imdb_id', 'tvdb_id', 'title_year', 'series_episode', 'unmatched')
    );

ALTER TABLE migration_import_log
    DROP CONSTRAINT IF EXISTS migration_import_log_match_confidence_check;

ALTER TABLE migration_import_log
    ADD CONSTRAINT migration_import_log_match_confidence_check CHECK (
        match_confidence IS NULL
        OR match_confidence IN ('high', 'medium', 'low', 'unmatched')
    );

CREATE INDEX IF NOT EXISTS idx_migration_import_log_match_confidence
    ON migration_import_log (match_confidence)
    WHERE match_confidence IS NOT NULL;

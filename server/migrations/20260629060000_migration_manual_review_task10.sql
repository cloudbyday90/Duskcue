ALTER TABLE migration_import_log
    DROP CONSTRAINT IF EXISTS migration_import_log_match_method_check;

ALTER TABLE migration_import_log
    ADD CONSTRAINT migration_import_log_match_method_check CHECK (
        match_method IN ('tmdb_id', 'imdb_id', 'tvdb_id', 'title_year', 'series_episode', 'manual', 'unmatched')
    );

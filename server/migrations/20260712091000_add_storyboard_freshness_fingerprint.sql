ALTER TABLE storyboards
    ALTER COLUMN file_hash DROP NOT NULL;

UPDATE storyboards
SET file_hash = NULL
WHERE file_hash = '';

ALTER TABLE storyboards
    ADD COLUMN IF NOT EXISTS config_fingerprint TEXT;

ALTER TABLE storyboards
    ADD COLUMN IF NOT EXISTS artifact_id UUID;

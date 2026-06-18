DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'user_item_data' AND column_name = 'metadata'
    ) THEN
        ALTER TABLE user_item_data ADD COLUMN metadata JSONB NOT NULL DEFAULT '{}'::jsonb;
    END IF;
END $$;

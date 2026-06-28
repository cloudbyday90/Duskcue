-- Phase 13b Task 2: Add push_enabled column to user_notification_preferences
--
-- Per MOBILE_PUSH.md §Per-User Channel Preferences:
--   ALTER TABLE user_notification_preferences
--   ADD COLUMN push_enabled BOOLEAN NOT NULL DEFAULT false,
--   ADD COLUMN webhook_enabled BOOLEAN NOT NULL DEFAULT true;
--
-- webhook_enabled already exists from Phase 2 (default false). Only push_enabled
-- is added here. The existing webhook_enabled default (false) is kept — users opt
-- in per notification type via the preferences UI (Phase 13b Task 6).
--
-- Idempotent: uses DO $$ ... $$ to check information_schema before adding.

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'user_notification_preferences'
          AND column_name = 'push_enabled'
    ) THEN
        ALTER TABLE user_notification_preferences
            ADD COLUMN push_enabled BOOLEAN NOT NULL DEFAULT false;
    END IF;
END $$;

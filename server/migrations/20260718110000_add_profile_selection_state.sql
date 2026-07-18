ALTER TABLE user_sessions
    ADD COLUMN IF NOT EXISTS profile_selection_required BOOLEAN NOT NULL DEFAULT false;

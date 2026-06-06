CREATE TABLE IF NOT EXISTS streaming_policies (
    id UUID DEFAULT uuidv7() PRIMARY KEY,
    created_at TIMESTAMPTZ GENERATED ALWAYS AS (uuid_extract_timestamp(id)) STORED,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),

    name TEXT NOT NULL UNIQUE,
    description TEXT,

    max_streams INT,
    max_transcode_streams INT,
    bandwidth_limit_bps BIGINT,

    allow_direct_play BOOLEAN NOT NULL DEFAULT true,
    allow_direct_stream BOOLEAN NOT NULL DEFAULT true,
    allow_transcode BOOLEAN NOT NULL DEFAULT true,

    max_transcode_resolution TEXT CHECK (max_transcode_resolution IN ('480p', '720p', '1080p', '4k')),
    allow_transcode_4k BOOLEAN NOT NULL DEFAULT true,
    require_direct_play_4k BOOLEAN NOT NULL DEFAULT false,

    allowed_ip_ranges JSONB NOT NULL DEFAULT '[]',
    blocked_ip_ranges JSONB NOT NULL DEFAULT '[]',

    auto_terminate_paused_minutes INT,

    is_default BOOLEAN NOT NULL DEFAULT false,
    is_system BOOLEAN NOT NULL DEFAULT false,

    metadata JSONB NOT NULL DEFAULT '{}'
);

CREATE INDEX IF NOT EXISTS idx_streaming_policies_is_default ON streaming_policies (is_default) WHERE is_default = true;

DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'users' AND column_name = 'avatar_url') THEN
        ALTER TABLE users ADD COLUMN avatar_url TEXT;
    END IF;

    IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'users' AND column_name = 'password_hash') THEN
        ALTER TABLE users ADD COLUMN password_hash TEXT;
    END IF;

    IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'users' AND column_name = 'role') THEN
        ALTER TABLE users ADD COLUMN role TEXT NOT NULL DEFAULT 'member' CHECK (role IN ('owner', 'admin', 'member', 'guest'));
    END IF;

    IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'users' AND column_name = 'status') THEN
        ALTER TABLE users ADD COLUMN status TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'disabled', 'locked', 'pending'));
    END IF;

    IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'users' AND column_name = 'failed_login_attempts') THEN
        ALTER TABLE users ADD COLUMN failed_login_attempts INT NOT NULL DEFAULT 0;
    END IF;

    IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'users' AND column_name = 'locked_until') THEN
        ALTER TABLE users ADD COLUMN locked_until TIMESTAMPTZ;
    END IF;

    IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'users' AND column_name = 'last_login_at') THEN
        ALTER TABLE users ADD COLUMN last_login_at TIMESTAMPTZ;
    END IF;

    IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'users' AND column_name = 'last_login_ip') THEN
        ALTER TABLE users ADD COLUMN last_login_ip INET;
    END IF;

    IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'users' AND column_name = 'has_all_library_access') THEN
        ALTER TABLE users ADD COLUMN has_all_library_access BOOLEAN NOT NULL DEFAULT true;
    END IF;

    IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'users' AND column_name = 'streaming_policy_id') THEN
        ALTER TABLE users ADD COLUMN streaming_policy_id UUID REFERENCES streaming_policies(id) ON DELETE SET NULL;
    END IF;

    IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'users' AND column_name = 'max_streams') THEN
        ALTER TABLE users ADD COLUMN max_streams INT;
    END IF;

    IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'users' AND column_name = 'max_transcode_streams') THEN
        ALTER TABLE users ADD COLUMN max_transcode_streams INT;
    END IF;

    IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'users' AND column_name = 'bandwidth_limit_bps') THEN
        ALTER TABLE users ADD COLUMN bandwidth_limit_bps BIGINT;
    END IF;

    IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'users' AND column_name = 'deleted_at') THEN
        ALTER TABLE users ADD COLUMN deleted_at TIMESTAMPTZ;
    END IF;
END $$;

DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_indexes WHERE indexname = 'users_username_active') THEN
        CREATE UNIQUE INDEX users_username_active ON users (username) WHERE deleted_at IS NULL;
    END IF;

    IF NOT EXISTS (SELECT 1 FROM pg_indexes WHERE indexname = 'users_email_active') THEN
        CREATE UNIQUE INDEX users_email_active ON users (email) WHERE email IS NOT NULL AND deleted_at IS NULL;
    END IF;

    IF NOT EXISTS (SELECT 1 FROM pg_indexes WHERE indexname = 'idx_users_role') THEN
        CREATE INDEX idx_users_role ON users (role);
    END IF;

    IF NOT EXISTS (SELECT 1 FROM pg_indexes WHERE indexname = 'idx_users_status') THEN
        CREATE INDEX idx_users_status ON users (status);
    END IF;

    IF NOT EXISTS (SELECT 1 FROM pg_indexes WHERE indexname = 'idx_users_email') THEN
        CREATE INDEX idx_users_email ON users (email) WHERE email IS NOT NULL AND deleted_at IS NULL;
    END IF;
END $$;

CREATE TABLE IF NOT EXISTS user_passkeys (
    id UUID DEFAULT uuidv7() PRIMARY KEY,
    created_at TIMESTAMPTZ GENERATED ALWAYS AS (uuid_extract_timestamp(id)) STORED,

    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,

    credential_id BYTEA NOT NULL UNIQUE,
    public_key BYTEA NOT NULL,
    sign_count BIGINT NOT NULL DEFAULT 0,
    transports JSONB NOT NULL DEFAULT '[]',
    attestation_type TEXT,
    aaguid UUID,
    name TEXT NOT NULL,

    last_used_at TIMESTAMPTZ,

    UNIQUE(user_id, credential_id)
);

CREATE INDEX IF NOT EXISTS idx_user_passkeys_user_id ON user_passkeys (user_id);
CREATE INDEX IF NOT EXISTS idx_user_passkeys_credential_id ON user_passkeys (credential_id);

CREATE TABLE IF NOT EXISTS user_totp (
    id UUID DEFAULT uuidv7() PRIMARY KEY,
    created_at TIMESTAMPTZ GENERATED ALWAYS AS (uuid_extract_timestamp(id)) STORED,

    user_id UUID NOT NULL UNIQUE REFERENCES users(id) ON DELETE CASCADE,

    secret TEXT NOT NULL,
    backup_codes JSONB NOT NULL DEFAULT '[]',
    is_verified BOOLEAN NOT NULL DEFAULT false,

    last_used_at TIMESTAMPTZ
);

CREATE INDEX IF NOT EXISTS idx_user_totp_user_id ON user_totp (user_id);

CREATE TABLE IF NOT EXISTS user_capabilities (
    id UUID DEFAULT uuidv7() PRIMARY KEY,
    created_at TIMESTAMPTZ GENERATED ALWAYS AS (uuid_extract_timestamp(id)) STORED,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),

    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    capability TEXT NOT NULL,
    is_granted BOOLEAN NOT NULL DEFAULT true,

    UNIQUE(user_id, capability)
);

CREATE INDEX IF NOT EXISTS idx_user_capabilities_user_id ON user_capabilities (user_id);

CREATE TABLE IF NOT EXISTS user_library_access (
    id UUID DEFAULT uuidv7() PRIMARY KEY,
    created_at TIMESTAMPTZ GENERATED ALWAYS AS (uuid_extract_timestamp(id)) STORED,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),

    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    library_id UUID NOT NULL REFERENCES libraries(id) ON DELETE CASCADE,

    UNIQUE(user_id, library_id)
);

CREATE INDEX IF NOT EXISTS idx_user_library_access_user_id ON user_library_access (user_id);
CREATE INDEX IF NOT EXISTS idx_user_library_access_library_id ON user_library_access (library_id);

CREATE TABLE IF NOT EXISTS user_sessions (
    id UUID DEFAULT uuidv7() PRIMARY KEY,
    created_at TIMESTAMPTZ GENERATED ALWAYS AS (uuid_extract_timestamp(id)) STORED,

    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,

    token_hash TEXT NOT NULL UNIQUE,
    device_id TEXT,
    device_name TEXT,
    client_name TEXT,
    client_version TEXT,
    client_platform TEXT,

    ip_address INET,
    user_agent TEXT,
    is_secure BOOLEAN NOT NULL DEFAULT false,

    expires_at TIMESTAMPTZ NOT NULL,
    last_active_at TIMESTAMPTZ NOT NULL DEFAULT now(),

    metadata JSONB NOT NULL DEFAULT '{}'
);

CREATE INDEX IF NOT EXISTS idx_user_sessions_user_id ON user_sessions (user_id);
CREATE INDEX IF NOT EXISTS idx_user_sessions_token_hash ON user_sessions (token_hash);
CREATE INDEX IF NOT EXISTS idx_user_sessions_expires_at ON user_sessions (expires_at);
CREATE INDEX IF NOT EXISTS idx_user_sessions_device ON user_sessions (user_id, device_id);

CREATE TABLE IF NOT EXISTS api_keys (
    id UUID DEFAULT uuidv7() PRIMARY KEY,
    created_at TIMESTAMPTZ GENERATED ALWAYS AS (uuid_extract_timestamp(id)) STORED,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),

    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,

    name TEXT NOT NULL,
    key_prefix TEXT NOT NULL,
    key_hash TEXT NOT NULL UNIQUE,

    capabilities JSONB NOT NULL DEFAULT '[]',
    is_active BOOLEAN NOT NULL DEFAULT true,

    last_used_at TIMESTAMPTZ,
    expires_at TIMESTAMPTZ,

    metadata JSONB NOT NULL DEFAULT '{}'
);

CREATE INDEX IF NOT EXISTS idx_api_keys_user_id ON api_keys (user_id);
CREATE INDEX IF NOT EXISTS idx_api_keys_key_hash ON api_keys (key_hash);
CREATE INDEX IF NOT EXISTS idx_api_keys_key_prefix ON api_keys (key_prefix);
CREATE INDEX IF NOT EXISTS idx_api_keys_active ON api_keys (is_active) WHERE is_active = true;

CREATE TABLE IF NOT EXISTS invitations (
    id UUID DEFAULT uuidv7() PRIMARY KEY,
    created_at TIMESTAMPTZ GENERATED ALWAYS AS (uuid_extract_timestamp(id)) STORED,

    created_by_user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    user_id UUID REFERENCES users(id) ON DELETE SET NULL,

    code_hash TEXT NOT NULL UNIQUE,
    code_prefix TEXT NOT NULL,

    email TEXT NOT NULL,
    display_name TEXT,

    role TEXT NOT NULL DEFAULT 'member' CHECK (role IN ('admin', 'member', 'guest')),

    capabilities JSONB NOT NULL DEFAULT '[]',
    library_ids JSONB NOT NULL DEFAULT '[]',
    has_all_library_access BOOLEAN NOT NULL DEFAULT false,
    streaming_policy_id UUID REFERENCES streaming_policies(id) ON DELETE SET NULL,

    max_uses INT NOT NULL DEFAULT 1,
    use_count INT NOT NULL DEFAULT 0,

    expires_at TIMESTAMPTZ,
    is_revoked BOOLEAN NOT NULL DEFAULT false,

    metadata JSONB NOT NULL DEFAULT '{}'
);

CREATE INDEX IF NOT EXISTS idx_invitations_code_hash ON invitations (code_hash);
CREATE INDEX IF NOT EXISTS idx_invitations_code_prefix ON invitations (code_prefix);
CREATE INDEX IF NOT EXISTS idx_invitations_created_by ON invitations (created_by_user_id);
CREATE INDEX IF NOT EXISTS idx_invitations_user_id ON invitations (user_id);
CREATE INDEX IF NOT EXISTS idx_invitations_email ON invitations (email);
CREATE INDEX IF NOT EXISTS idx_invitations_expires ON invitations (expires_at) WHERE expires_at IS NOT NULL AND is_revoked = false;

CREATE TABLE IF NOT EXISTS device_linking_codes (
    id UUID DEFAULT uuidv7() PRIMARY KEY,
    created_at TIMESTAMPTZ GENERATED ALWAYS AS (uuid_extract_timestamp(id)) STORED,

    user_code TEXT NOT NULL UNIQUE,
    device_code TEXT NOT NULL UNIQUE,

    client_name TEXT,
    client_platform TEXT,
    client_version TEXT,

    ip_address INET,
    user_agent TEXT,

    expires_at TIMESTAMPTZ NOT NULL,
    is_approved BOOLEAN NOT NULL DEFAULT false,
    approved_by_user_id UUID REFERENCES users(id) ON DELETE CASCADE,
    approved_at TIMESTAMPTZ,

    resulting_session_id UUID REFERENCES user_sessions(id) ON DELETE SET NULL
);

CREATE INDEX IF NOT EXISTS idx_device_linking_user_code ON device_linking_codes (user_code);
CREATE INDEX IF NOT EXISTS idx_device_linking_device_code ON device_linking_codes (device_code);
CREATE INDEX IF NOT EXISTS idx_device_linking_expires ON device_linking_codes (expires_at) WHERE is_approved = false;

CREATE TABLE IF NOT EXISTS reauth_codes (
    id UUID DEFAULT uuidv7() PRIMARY KEY,
    created_at TIMESTAMPTZ GENERATED ALWAYS AS (uuid_extract_timestamp(id)) STORED,

    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    requested_by_user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,

    code_hash TEXT NOT NULL UNIQUE,
    code_prefix TEXT NOT NULL,

    ip_address INET,

    expires_at TIMESTAMPTZ NOT NULL,
    is_used BOOLEAN NOT NULL DEFAULT false,
    used_at TIMESTAMPTZ,

    resulting_session_id UUID REFERENCES user_sessions(id) ON DELETE SET NULL,

    metadata JSONB NOT NULL DEFAULT '{}'
);

CREATE INDEX IF NOT EXISTS idx_reauth_codes_user_id ON reauth_codes (user_id);
CREATE INDEX IF NOT EXISTS idx_reauth_codes_code_hash ON reauth_codes (code_hash);
CREATE INDEX IF NOT EXISTS idx_reauth_codes_expires ON reauth_codes (expires_at) WHERE is_used = false;

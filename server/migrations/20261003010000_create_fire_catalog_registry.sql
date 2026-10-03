CREATE TABLE IF NOT EXISTS fire_catalog_mappings (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    media_item_id UUID REFERENCES media_items(id) ON DELETE SET NULL,
    original_media_item_id UUID NOT NULL,
    media_type TEXT NOT NULL CHECK (media_type IN ('movie', 'episode')),
    catalog_reference TEXT NOT NULL CHECK (length(catalog_reference) BETWEEN 1 AND 200),
    amazon_content_id TEXT NOT NULL CHECK (length(amazon_content_id) BETWEEN 1 AND 512),
    acceptance_reference TEXT NOT NULL CHECK (length(acceptance_reference) BETWEEN 1 AND 200),
    distribution_rights_reference TEXT NOT NULL CHECK (length(distribution_rights_reference) BETWEEN 1 AND 200),
    rights_expires_at TIMESTAMPTZ NOT NULL,
    is_enabled BOOLEAN NOT NULL DEFAULT false,
    revision BIGINT NOT NULL DEFAULT 1 CHECK (revision > 0),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (catalog_reference, amazon_content_id),
    UNIQUE (catalog_reference, original_media_item_id),
    CHECK (media_item_id IS NULL OR media_item_id = original_media_item_id)
);

CREATE TABLE IF NOT EXISTS fire_catalog_mapping_events (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    mapping_id UUID NOT NULL REFERENCES fire_catalog_mappings(id),
    revision BIGINT NOT NULL CHECK (revision > 0),
    actor_user_id UUID REFERENCES users(id) ON DELETE SET NULL,
    snapshot JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (mapping_id, revision)
);

CREATE TABLE IF NOT EXISTS fire_profile_keys (
    profile_id UUID PRIMARY KEY REFERENCES user_profiles(id) ON DELETE CASCADE,
    opaque_key TEXT NOT NULL UNIQUE CHECK (opaque_key ~ '^[A-Za-z0-9_-]{43}$'),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_fire_catalog_mappings_media ON fire_catalog_mappings(media_item_id);

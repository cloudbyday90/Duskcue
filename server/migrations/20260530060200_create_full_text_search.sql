CREATE OR REPLACE FUNCTION rebuild_media_search_vector()
RETURNS TRIGGER AS $$
DECLARE
    target_id UUID;
    lang TEXT;
    cfg REGCONFIG;
BEGIN
    IF TG_TABLE_NAME = 'media_items' THEN
        target_id := COALESCE(NEW.id, OLD.id);
    ELSIF TG_TABLE_NAME IN ('media_credits', 'media_genres', 'media_tags') THEN
        target_id := COALESCE(NEW.media_item_id, OLD.media_item_id);
    END IF;

    SELECT COALESCE(metadata_language, 'en') INTO lang
    FROM media_items mi JOIN libraries l ON mi.library_id = l.id
    WHERE mi.id = target_id;

    cfg := lang::REGCONFIG;

    UPDATE media_items SET search_vector =
        setweight(to_tsvector(cfg, COALESCE(title, '')), 'A') ||
        setweight(to_tsvector(cfg, COALESCE(original_title, '')), 'A') ||
        setweight(to_tsvector(cfg, COALESCE(overview, '')), 'B') ||
        setweight(to_tsvector(cfg, COALESCE(
            (SELECT string_agg(p.name, ' ')
             FROM media_credits mc JOIN people p ON mc.person_id = p.id
             WHERE mc.media_item_id = target_id), '')), 'C') ||
        setweight(to_tsvector(cfg, COALESCE(
            (SELECT string_agg(g.name, ' ')
             FROM media_genres mg JOIN genres g ON mg.genre_id = g.id
             WHERE mg.media_item_id = target_id), '')), 'D') ||
        setweight(to_tsvector(cfg, COALESCE(
            (SELECT string_agg(t.name, ' ')
             FROM media_tags mt JOIN tags t ON mt.tag_id = t.id
             WHERE mt.media_item_id = target_id), '')), 'D')
    WHERE id = target_id;

    RETURN COALESCE(NEW, OLD);
END;
$$ LANGUAGE plpgsql;

DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_trigger WHERE tgname = 'media_items_search_vector') THEN
        CREATE TRIGGER media_items_search_vector
            AFTER INSERT OR UPDATE OF title, original_title, overview ON media_items
            FOR EACH ROW EXECUTE FUNCTION rebuild_media_search_vector();
    END IF;

    IF NOT EXISTS (SELECT 1 FROM pg_trigger WHERE tgname = 'media_credits_search_vector') THEN
        CREATE TRIGGER media_credits_search_vector
            AFTER INSERT OR UPDATE OR DELETE ON media_credits
            FOR EACH ROW EXECUTE FUNCTION rebuild_media_search_vector();
    END IF;

    IF NOT EXISTS (SELECT 1 FROM pg_trigger WHERE tgname = 'media_genres_search_vector') THEN
        CREATE TRIGGER media_genres_search_vector
            AFTER INSERT OR UPDATE OR DELETE ON media_genres
            FOR EACH ROW EXECUTE FUNCTION rebuild_media_search_vector();
    END IF;

    IF NOT EXISTS (SELECT 1 FROM pg_trigger WHERE tgname = 'media_tags_search_vector') THEN
        CREATE TRIGGER media_tags_search_vector
            AFTER INSERT OR UPDATE OR DELETE ON media_tags
            FOR EACH ROW EXECUTE FUNCTION rebuild_media_search_vector();
    END IF;
END $$;

CREATE INDEX IF NOT EXISTS idx_media_items_title_trgm ON media_items USING GIN (title gin_trgm_ops);

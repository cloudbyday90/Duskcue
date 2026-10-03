CREATE OR REPLACE FUNCTION rebuild_media_search_vector()
RETURNS TRIGGER AS $$
DECLARE
    target_id UUID;
    lang TEXT;
    cfg REGCONFIG;
    config_name TEXT;
BEGIN
    IF TG_TABLE_NAME = 'media_items' THEN
        target_id := COALESCE(NEW.id, OLD.id);
    ELSIF TG_TABLE_NAME IN ('media_credits', 'media_genres', 'media_tags') THEN
        target_id := COALESCE(NEW.media_item_id, OLD.media_item_id);
    END IF;

    SELECT COALESCE(metadata_language, 'en') INTO lang
    FROM media_items mi JOIN libraries l ON mi.library_id = l.id
    WHERE mi.id = target_id;

    config_name := CASE split_part(lower(replace(COALESCE(lang, 'en'), '_', '-')), '-', 1)
        WHEN 'en' THEN 'english' WHEN 'english' THEN 'english'
        WHEN 'de' THEN 'german' WHEN 'german' THEN 'german'
        WHEN 'fr' THEN 'french' WHEN 'french' THEN 'french'
        WHEN 'es' THEN 'spanish' WHEN 'spanish' THEN 'spanish'
        WHEN 'it' THEN 'italian' WHEN 'italian' THEN 'italian'
        WHEN 'pt' THEN 'portuguese' WHEN 'portuguese' THEN 'portuguese'
        WHEN 'ru' THEN 'russian' WHEN 'russian' THEN 'russian'
        WHEN 'nl' THEN 'dutch' WHEN 'dutch' THEN 'dutch'
        WHEN 'da' THEN 'danish' WHEN 'danish' THEN 'danish'
        WHEN 'fi' THEN 'finnish' WHEN 'finnish' THEN 'finnish'
        WHEN 'sv' THEN 'swedish' WHEN 'swedish' THEN 'swedish'
        WHEN 'no' THEN 'norwegian' WHEN 'nb' THEN 'norwegian' WHEN 'nn' THEN 'norwegian' WHEN 'norwegian' THEN 'norwegian'
        WHEN 'ar' THEN 'arabic' WHEN 'arabic' THEN 'arabic'
        WHEN 'tr' THEN 'turkish' WHEN 'turkish' THEN 'turkish'
        WHEN 'hu' THEN 'hungarian' WHEN 'hungarian' THEN 'hungarian'
        WHEN 'ro' THEN 'romanian' WHEN 'romanian' THEN 'romanian'
        WHEN 'el' THEN 'greek' WHEN 'greek' THEN 'greek'
        WHEN 'id' THEN 'indonesian' WHEN 'indonesian' THEN 'indonesian'
        WHEN 'ga' THEN 'irish' WHEN 'irish' THEN 'irish'
        WHEN 'lt' THEN 'lithuanian' WHEN 'lithuanian' THEN 'lithuanian'
        WHEN 'ne' THEN 'nepali' WHEN 'nepali' THEN 'nepali'
        WHEN 'sr' THEN 'serbian' WHEN 'serbian' THEN 'serbian'
        WHEN 'ta' THEN 'tamil' WHEN 'tamil' THEN 'tamil'
        WHEN 'hy' THEN 'armenian' WHEN 'armenian' THEN 'armenian'
        WHEN 'eu' THEN 'basque' WHEN 'basque' THEN 'basque'
        WHEN 'ca' THEN 'catalan' WHEN 'catalan' THEN 'catalan'
        WHEN 'yi' THEN 'yiddish' WHEN 'yiddish' THEN 'yiddish'
        ELSE 'simple'
    END;
    SELECT c.oid::regconfig INTO cfg
    FROM pg_catalog.pg_ts_config c
    JOIN pg_catalog.pg_namespace n ON n.oid = c.cfgnamespace
    WHERE n.nspname = 'pg_catalog' AND c.cfgname = config_name;
    cfg := COALESCE(cfg, 'pg_catalog.simple'::regconfig);

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

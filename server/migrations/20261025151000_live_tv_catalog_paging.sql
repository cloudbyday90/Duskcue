-- Duskcue — Self-hosted media streaming server
-- Copyright (C) 2026-2026 Duskcue Contributors
-- Licensed under AGPL-3.0. See LICENSE for details.

CREATE INDEX live_tv_catalog_programmes_guide_page
    ON live_tv_catalog_programmes(guide_id,starts_at,id) INCLUDE(stops_at);
DROP INDEX live_tv_catalog_programmes_guide_time;

CREATE INDEX live_tv_catalog_programmes_unpositionable
    ON live_tv_catalog_programmes(guide_id,id)
    WHERE starts_at IS NULL OR stops_at IS NULL;

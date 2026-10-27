// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

pub(crate) const PRUNE: &str = "WITH candidates AS MATERIALIZED (SELECT id,prune_after FROM playback_reservations WHERE state='released' AND prune_after<=$1 ORDER BY prune_after,id LIMIT 128), locked AS MATERIALIZED (SELECT r.id FROM playback_reservations r JOIN candidates c ON c.id=r.id ORDER BY c.prune_after,c.id FOR UPDATE OF r SKIP LOCKED) DELETE FROM playback_reservations r USING locked l WHERE r.id=l.id RETURNING r.id";

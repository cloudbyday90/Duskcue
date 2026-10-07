/*
 * Duskcue — Self-hosted media streaming server
 * Copyright (C) 2026-2026 Duskcue Contributors
 * Licensed under AGPL-3.0. See LICENSE for details.
 */

import { createHash } from 'node:crypto';

const frozen = new Map([
    ['server/migrations/20261003000000_fix_search_language_configs.sql', '4b979c532546adf09585b762a59a5d22a209b29fda819fbd63864d4a75ca4a67'],
    ['server/migrations/20261003010000_create_fire_catalog_registry.sql', '0ba4dc76f38f8ead3c0cf3acf90d1a94369ac7cd4971166522b70065fc094341'],
]);

export function preserveMigrationCopyright(file, content) {
    const path = file.replaceAll('\\', '/');
    const version = /^server\/migrations\/(\d+)_/.exec(path)?.[1];
    if (version && BigInt(version) <= 20260701050000n) return true;
    const checksum = frozen.get(path);
    if (!checksum) return false;
    if (createHash('sha256').update(content).digest('hex') !== checksum) throw new Error(`Frozen migration bytes changed: ${path}.`);
    return true;
}

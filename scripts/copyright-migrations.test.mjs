/*
 * Duskcue — Self-hosted media streaming server
 * Copyright (C) 2026-2026 Duskcue Contributors
 * Licensed under AGPL-3.0. See LICENSE for details.
 */

import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { test } from 'node:test';
import { preserveMigrationCopyright } from './copyright-migrations.mjs';

for (const file of ['server/migrations/20261003000000_fix_search_language_configs.sql', 'server/migrations/20261003010000_create_fire_catalog_registry.sql']) {
    test(`preserves exact existing migration bytes: ${file}`, async () => {
        const content = await readFile(new URL(`../${file}`, import.meta.url));
        assert.equal(preserveMigrationCopyright(file, content), true);
        assert.equal(preserveMigrationCopyright(file.replaceAll('/', '\\'), content), true);
        assert.throws(() => preserveMigrationCopyright(file, Buffer.concat([content, Buffer.from('\n')])));
        assert.throws(() => preserveMigrationCopyright(file, Buffer.concat([Buffer.from('-- New header\n'), content])));
    });
}

test('only the original legacy cutoff and exact frozen paths are exempt', () => {
    assert.equal(preserveMigrationCopyright('server/migrations/20260701050000_existing.sql', ''), true);
    assert.equal(preserveMigrationCopyright('server/migrations/20260701050001_new.sql', ''), false);
    assert.equal(preserveMigrationCopyright('server/migrations/20261003000000_different.sql', ''), false);
    assert.equal(preserveMigrationCopyright('server/migrations/20261008000000_future.sql', ''), false);
    assert.equal(preserveMigrationCopyright('server/src/example.rs', ''), false);
});

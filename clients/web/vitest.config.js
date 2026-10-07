/*
 * Duskcue — Self-hosted media streaming server
 * Copyright (C) 2026-2026 Duskcue Contributors
 * Licensed under AGPL-3.0. See LICENSE for details.
 */

import { fileURLToPath } from 'node:url';
import { defineConfig } from 'vitest/config';

export default defineConfig({
    test: {
        include: ['tests/unit/**/*.test.{js,ts}'],
        environment: 'node',
        maxWorkers: 1,
        restoreMocks: true,
        unstubGlobals: true,
        coverage: {
            provider: 'v8',
            include: ['src/lib/api/core.js', 'src/lib/api/artwork.js', 'src/lib/navigation/routes.js', 'src/lib/browsing/*.js'],
            reporter: ['text', 'html'],
            reportsDirectory: fileURLToPath(new URL('../../.cache/tonight-web/coverage', import.meta.url)),
        },
    },
});

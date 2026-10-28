// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import { defineConfig, devices } from '@playwright/test';

export default defineConfig({
    testDir: './tests/vod-commands', workers: 1, retries: 0, maxFailures: 1, timeout: 120_000,
    outputDir: '../../.cache/vod-client-migration/browser', reporter: 'list', expect: { timeout: 15000 },
    use: { baseURL: 'http://localhost:48037', locale: 'en-US', serviceWorkers: 'block', ...devices['Desktop Chrome'] },
    webServer: { command: 'npm run dev -- --host 127.0.0.1 --port 48037 --strictPort', url: 'http://localhost:48037/auth/login', reuseExistingServer: false, timeout: 120_000 },
});

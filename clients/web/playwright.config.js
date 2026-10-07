import { fileURLToPath } from 'node:url';
import { defineConfig, devices } from '@playwright/test';

const baseURL = 'http://127.0.0.1:48037';

export default defineConfig({
    testDir: './tests/e2e',
    fullyParallel: true,
    forbidOnly: !!process.env.CI,
    retries: process.env.CI ? 1 : 0,
    workers: 1,
    outputDir: fileURLToPath(new URL('../../.cache/tonight-web/playwright/results', import.meta.url)),
    reporter: [
        ['list'],
        ['html', {
            outputFolder: fileURLToPath(new URL('../../.cache/tonight-web/playwright/report', import.meta.url)),
            open: 'never',
        }],
    ],
    use: {
        baseURL,
        locale: 'en-US',
        serviceWorkers: 'block',
        trace: 'on-first-retry',
        screenshot: 'only-on-failure',
    },
    projects: [{ name: 'chromium', use: { ...devices['Desktop Chrome'] } }],
    webServer: {
        command: 'npm run dev -- --host 127.0.0.1 --port 48037 --strictPort',
        url: `${baseURL}/auth/login`,
        reuseExistingServer: false,
        timeout: 120_000,
    },
});

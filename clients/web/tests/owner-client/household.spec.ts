/*
 * Duskcue — Self-hosted media streaming server
 * Copyright (C) 2026-2026 Duskcue Contributors
 * Licensed under AGPL-3.0. See LICENSE for details.
 */
import { readFile } from 'node:fs/promises';
import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import { join } from 'node:path';
import { test, expect } from '@playwright/test';

test('actual browser registration, signed unlock, operator recovery and replacement', async ({ page, context }) => {
    const fixture = JSON.parse(await readFile(process.env.DUSKCUE_OWNER_FIXTURE!, 'utf8'));
    await context.addCookies([{ name: 'session', value: fixture.token, domain: 'localhost', path: '/', httpOnly: true, sameSite: 'Strict' }]);
    await page.addInitScript((user) => { localStorage.setItem('duskcue_user', JSON.stringify(user)); localStorage.setItem('duskcue_device_id', 'owner-browser-qualification'); }, fixture.user);
    await page.route('**/api/v1/events', (route) => route.fulfill({ contentType: 'text/event-stream', body: ': finite root fixture\n\n' }));
    const errors: string[] = [];
    page.on('pageerror', (error) => errors.push(error.message));
    const cdp = await context.newCDPSession(page);
    await cdp.send('WebAuthn.enable', { enableUI: false });
    const options = { protocol: 'ctap2', transport: 'internal', hasResidentKey: true, hasUserVerification: true, isUserVerified: true, automaticPresenceSimulation: true } as const;
    let { authenticatorId } = await cdp.send('WebAuthn.addVirtualAuthenticator', { options });
    try {
        await page.goto('/settings/household');
        await expect(page.getByText('Owner-trusted passkeys: 0', { exact: true })).toBeVisible();
        const enroll = async (handoff: string, name: string) => {
            await page.getByText('Enroll or recover with an operator handoff', { exact: true }).click();
            await page.getByLabel('Proof method', { exact: true }).selectOption('register');
            await page.getByLabel('Operator handoff', { exact: true }).fill(handoff);
            await page.getByLabel('Passkey name', { exact: true }).fill(name);
            await page.getByRole('button', { name: /Enable owner protection|Complete owner recovery/ }).click();
            await expect(page.getByRole('status').filter({ hasText: 'Management verified for this session' })).toBeVisible();
        };
        await enroll(fixture.handoff, 'Browser owner');
        await expect(page.getByText('Owner-trusted passkeys: 1', { exact: true })).toBeVisible();
        const cookie = (await context.cookies()).find((entry) => entry.name === 'session')!;
        expect(cookie.httpOnly && cookie.sameSite === 'Strict' && cookie.value !== fixture.token).toBe(true);
        await page.getByRole('button', { name: 'Lock management', exact: true }).click();
        await expect(page.getByRole('status').filter({ hasText: 'Management is locked' })).toBeVisible();
        await cdp.send('WebAuthn.setResponseOverrideBits', { authenticatorId, isBogusSignature: true });
        await page.getByRole('button', { name: 'Verify owner passkey', exact: true }).click();
        await expect(page.getByRole('alert')).toContainText('viewing session has not been signed out');
        await cdp.send('WebAuthn.setResponseOverrideBits', { authenticatorId, isBogusSignature: false });
        await page.getByRole('button', { name: 'Check status', exact: true }).click();
        await expect(page.getByRole('button', { name: 'Verify owner passkey', exact: true })).toBeVisible();
        await page.getByRole('button', { name: 'Verify owner passkey', exact: true }).click();
        await expect(page.getByRole('status').filter({ hasText: 'Management verified for this session' })).toBeVisible();
        await page.screenshot({ path: test.info().outputPath('real-owner-passkey.png') });
        const operator = await promisify(execFile)(fixture.operator, ['--config', join(fixture.root, 'unused.toml'), 'owner-handoff', '--user', fixture.user.id], {
            env: { ...process.env, DUSKCUE_DATABASE_URL: fixture.databaseUrl, DUSKCUE_ENVIRONMENT: 'production', DUSKCUE_DATA_DIR: join(fixture.root, 'data'), DUSKCUE_CACHE_DIR: join(fixture.root, 'cache') },
            timeout: 30_000, maxBuffer: 4096, windowsHide: true,
        });
        if (operator.stderr || !/^duskcue-owner-[A-Za-z0-9_-]{43}$/.test(operator.stdout.trim())) throw new Error('Operator fixture failed');
        await page.getByRole('button', { name: 'Check status', exact: true }).click();
        await expect(page.getByText('Owner-trusted passkeys: 0', { exact: true })).toBeVisible();
        await expect(page.getByRole('status').filter({ hasText: 'Management is locked' })).toBeVisible();
        await cdp.send('WebAuthn.removeVirtualAuthenticator', { authenticatorId });
        ({ authenticatorId } = await cdp.send('WebAuthn.addVirtualAuthenticator', { options }));
        await enroll(operator.stdout.trim(), 'Replacement owner');
        await page.getByRole('button', { name: 'Lock management', exact: true }).click();
        await expect(page.getByRole('status').filter({ hasText: 'Management is locked' })).toBeVisible();
        let finishes = 0;
        const other = await context.newPage();
        other.on('pageerror', (error) => errors.push(error.message));
        await other.addInitScript((user) => localStorage.setItem('duskcue_user', JSON.stringify(user)), fixture.user);
        await other.route('**/api/v1/events', (route) => route.fulfill({ contentType: 'text/event-stream', body: ': finite root fixture\n\n' }));
        await other.goto('/settings/household');
        await expect(other.getByRole('status').filter({ hasText: 'Management is locked' })).toBeVisible();
        await page.route('**/api/v1/household-management/passkey/finish', async (route) => {
            finishes += 1;
            const response = await route.fetch();
            if (response.status() !== 200) throw new Error('Lost-body fixture did not commit a real proof');
            await route.fulfill({ response, body: '{' });
        });
        await page.getByRole('button', { name: 'Verify owner passkey', exact: true }).click();
        await expect(page.getByRole('alert')).toContainText('could not be confirmed');
        await expect(other.getByText('Owner-trusted passkeys: 1', { exact: true })).toHaveCount(0);
        await expect(other.getByRole('status').filter({ hasText: 'Management verified for this session' })).toHaveCount(0);
        await other.getByRole('button', { name: 'Check status', exact: true }).click();
        await expect(other.getByRole('status').filter({ hasText: 'Management verified for this session' })).toBeVisible();
        await other.close();
        await page.unroute('**/api/v1/household-management/passkey/finish');
        await page.getByRole('button', { name: 'Check status', exact: true }).click();
        await expect(page.getByRole('status').filter({ hasText: 'Management verified for this session' })).toBeVisible();
        expect(finishes).toBe(1);
        const stored = await page.evaluate(() => JSON.stringify(Object.entries(localStorage)));
        expect(stored.includes(fixture.handoff) || stored.includes('credential_revision')).toBe(false);
        expect(errors).toEqual([]);
    } finally { await cdp.send('WebAuthn.removeVirtualAuthenticator', { authenticatorId }); await cdp.detach(); }
});

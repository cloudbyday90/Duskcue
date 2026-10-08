// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026 Duskcue Contributors
//
// This program is free software: licensed under AGPL-3.0
// See LICENSE file for details.

import { expect } from '@playwright/test';
import { invokeNative } from './qualification-api.mjs';
import { assertBackgroundEvidence, countdownSeconds } from './qualification-background-policy.mjs';
import { keyboardReach } from './qualification-focus.mjs';

async function readCountdown(page) {
    return page.evaluate(() => ({ visibility: document.visibilityState, hidden: document.hidden, path: location.pathname,
        text: document.querySelector('.autoplay-card .countdown')?.textContent?.trim() || '',
        cardFocused: !!document.querySelector('.autoplay-card')?.contains(document.activeElement),
        menuOpen: !!document.querySelector('.player-popover:not([hidden])'), events: window.__nativeBackgroundVisibility || [] }));
}

export async function exerciseRealBackgroundCountdown({ page, api, control, screenshot }) {
    const proof = { status: 'in_progress', transitions: [], focus: {}, restored: false, method: 'exact-owned hosted Win32 window minimize/restore and actual document visibility' };
    const playback = api.getPlayback();
    const starts = playback.starts.length;
    try {
        await page.evaluate(() => {
            window.__nativeBackgroundVisibility = [];
            window.__nativeBackgroundObserver = (event) => window.__nativeBackgroundVisibility.push({ visibility: document.visibilityState, hidden: document.hidden, trusted: event.isTrusted, time: performance.now() });
            document.addEventListener('visibilitychange', window.__nativeBackgroundObserver);
        });
        await page.getByRole('region', { name: 'Media player', exact: true }).focus();
        proof.before = await readCountdown(page);
        expect(proof.before.visibility).toBe('visible');
        expect(proof.before.cardFocused).toBe(false);
        expect(proof.before.menuOpen).toBe(false);
        const initial = countdownSeconds(proof.before);
        await expect.poll(async () => {
            const state = await readCountdown(page);
            proof.running = state;
            return state.visibility === 'visible' && !state.cardFocused && !state.menuOpen && !state.text.includes('paused') && countdownSeconds(state) < initial;
        }, { timeout: 3000 }).toBe(true);
        expect(countdownSeconds(proof.running)).toBeGreaterThan(4);
        proof.transitions.push(await control.action('minimize'));
        expect(await invokeNative(page, 'plugin:window|is_minimized', { label: 'main' })).toBe(true);
        await expect.poll(async () => {
            proof.hidden = await readCountdown(page);
            return proof.hidden.visibility === 'hidden' && proof.hidden.hidden === true && proof.hidden.text.includes('paused');
        }, { timeout: 5000 }).toBe(true);
        proof.holdStartedAt = Date.now();
        await new Promise((resolve) => setTimeout(resolve, 11000));
        proof.holdElapsedMs = Date.now() - proof.holdStartedAt;
        proof.afterHiddenHold = await readCountdown(page);
        assertBackgroundEvidence(proof.hidden, proof.afterHiddenHold, starts, playback.starts.length);
        expect(proof.holdElapsedMs).toBeGreaterThan(10000);
        proof.transitions.push(await control.action('restore'));
        expect(await invokeNative(page, 'plugin:window|is_minimized', { label: 'main' })).toBe(false);
        await expect.poll(async () => (await readCountdown(page)).visibility, { timeout: 5000 }).toBe('visible');
        await page.getByRole('region', { name: 'Media player', exact: true }).focus();
        proof.visibleAgain = await readCountdown(page);
        const remaining = countdownSeconds(proof.hidden);
        expect(playback.starts.length).toBe(starts);
        expect(countdownSeconds(proof.visibleAgain)).toBeGreaterThanOrEqual(remaining - 1);
        await expect.poll(async () => {
            proof.resumed = await readCountdown(page);
            return !proof.resumed.cardFocused && !proof.resumed.menuOpen && !proof.resumed.text.includes('paused') && countdownSeconds(proof.resumed) < remaining;
        }, { timeout: 3000 }).toBe(true);
        expect(proof.resumed.events.some((event) => event.visibility === 'visible' && event.trusted === true)).toBe(true);
        expect(playback.starts.length).toBe(starts);
        await keyboardReach(page, page.getByRole('button', { name: 'Play now', exact: true }), 'background-restored-card-focus', proof);
        await screenshot('native-400-background-restored');
        proof.status = 'passed';
        return proof;
    } catch (error) { proof.status = 'failed'; proof.error = error.message; error.backgroundProof = proof; throw error; }
    finally {
        let failure;
        try { await control.action('restore'); }
        catch (error) { failure = error; }
        try { await control.close(); }
        catch (error) { failure ||= error; }
        await page.evaluate(() => { document.removeEventListener('visibilitychange', window.__nativeBackgroundObserver); delete window.__nativeBackgroundObserver; }).catch(() => {});
        proof.restored = !failure;
        if (failure) { proof.restoreError = failure.message; proof.status = 'failed'; failure.backgroundProof = proof; throw failure; }
    }
}

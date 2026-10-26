/*
 * Duskcue — Self-hosted media streaming server
 * Copyright (C) 2026-2026 Duskcue Contributors
 * Licensed under AGPL-3.0. See LICENSE for details.
 */
import { afterEach, describe, expect, it, vi } from 'vitest';
import { createCatalogController } from '../../src/lib/live-tv/controller.js';
import { decodeCatalogPage } from '../../src/lib/live-tv/decode.js';
import { createCatalogPaging } from '../../src/lib/live-tv/paging.js';
import { instant, sixHourWindow, moveWindow } from '../../src/lib/live-tv/instant.js';
import { preferredText, textLanguage, formatInstant } from '../../src/lib/live-tv/presentation.js';
import { createApiScenario } from '../fixtures/api.js';
import { liveTvId, liveTvResponse } from '../fixtures/live-tv.js';

afterEach(() => vi.useRealTimers());
const now = () => Date.parse('2026-11-01T05:00:00Z');
function setup() {
    const scenario = createApiScenario();
    const fetchPage = vi.fn(async (lane, query, _signal) => {
        const url = new URL(`https://fixture.invalid/api/v1/live-tv/catalog/channels${lane === 'channels' ? '' : `/${query.channelId}/programmes${lane === 'unpositionable' ? '/unpositionable' : ''}`}`);
        for (const [key, value] of Object.entries(query)) if (value != null) url.searchParams.set(key, String(value));
        const response = liveTvResponse(scenario, url);
        if (response.status !== 200) throw { status: response.status, retryAfter: Number(scenario.liveTv.retryAfter) };
        return response.body as any;
    });
    const revalidate = vi.fn(async () => {});
    const controller = createCatalogController({ profileId: scenario.activeProfileId, fetchPage, revalidate, now });
    return { scenario, fetchPage, revalidate, controller };
}

describe('catalog response fences', () => {
    it('projects safe fields and keeps a revision above JavaScript integer precision', async () => {
        const { controller } = setup();
        await controller.reload();
        expect(controller.getState().context.profile_context_revision).toBe('9007199254740993');
        expect(controller.getState().channels[0]).not.toHaveProperty('stream_url');
        expect(controller.getState().channels[0]).not.toHaveProperty('logo_url');
        expect(controller.getState().selected).toBeNull();
        controller.dispose();
    });
    it.each(['revision', 'profile', 'uuid', 'duplicate', 'oversize', 'items'])('rejects invalid %s', async (kind) => {
        const { scenario, fetchPage } = setup();
        const body = await fetchPage('channels', {}, null);
        if (kind === 'revision') body.context.profile_context_revision = 3;
        if (kind === 'profile') body.context.profile_id = liveTvId(999);
        if (kind === 'uuid') body.context.snapshot_id = '00000000-0000-4000-8000-000000000001';
        if (kind === 'duplicate') body.items[1].id = body.items[0].id;
        if (kind === 'oversize') body.items[0].name = '🙂'.repeat(1025);
        if (kind === 'items') body.items.push(body.items[0]);
        expect(() => decodeCatalogPage(body, { profileId: scenario.activeProfileId })).toThrow();
    });
    it('clears every lane on stale snapshot or profile revision during guide delivery', async () => {
        for (const field of ['snapshot', 'revision']) {
            const { controller, scenario } = setup();
            await controller.reload();
            scenario.liveTv[field] = field === 'snapshot' ? liveTvId(8) : '2';
            await controller.select(liveTvId(100));
            expect(controller.getState()).toMatchObject({ context: null, channels: [], programmes: [], selected: null, error: { kind: 'failed' } });
            controller.dispose();
        }
    });
    it('checks programme channel, view and exact microsecond window', async () => {
        const { scenario, fetchPage } = setup();
        const window = sixHourWindow(now());
        const body = await fetchPage('window', { ...window, channelId: liveTvId(100) }, null);
        const options = { profileId: scenario.activeProfileId, lane: 'window', channelId: liveTvId(100), window };
        expect(() => decodeCatalogPage(body, options)).not.toThrow();
        body.window.from = body.window.from.replace('.000Z', '.000001Z');
        expect(() => decodeCatalogPage(body, options)).toThrow();
        body.window.from = window.from;
        expect(() => decodeCatalogPage(body, { ...options, channelId: liveTvId(101) })).toThrow();
        expect(() => decodeCatalogPage(body, { ...options, lane: 'unpositionable' })).toThrow();
    });
});

describe('catalog request and state ownership', () => {
    it('replaces pages, preserves the window across selection and resets guide pages', async () => {
        const { controller } = setup();
        await controller.reload();
        await controller.select(liveTvId(100));
        const window = controller.getState().window;
        await controller.programmes('next');
        expect(controller.getState().programmes).toHaveLength(5);
        await controller.select(liveTvId(101));
        expect(controller.getState().window).toEqual(window);
        expect(controller.getState().guidePage.index).toBe(0);
        await controller.channels('next');
        expect(controller.getState().channels).toHaveLength(20);
        expect(controller.getState().channels[0].id).toBe(liveTvId(120));
        expect(controller.getState().selected).toBeNull();
        await controller.channels('previous');
        expect(controller.getState().channels[0].id).toBe(liveTvId(100));
        controller.dispose();
    });
    it('allows empty snapshots and offers incomplete evidence without an invented window', async () => {
        const { controller, scenario } = setup();
        await controller.reload(); await controller.select(liveTvId(100));
        await controller.setView('unpositionable');
        expect(controller.getState().programmes[0]).toMatchObject({ starts_at: null, stops_at: null, start_raw: '202610' });
        scenario.liveTv.channelCount = 0;
        await controller.reload();
        expect(controller.getState()).toMatchObject({ channels: [], error: null });
        controller.dispose();
    });
    it.each([401, 403, 404, 409, 429, 503])('clears data and never automatically replays HTTP %s', async (status) => {
        const { controller, scenario, fetchPage, revalidate } = setup();
        await controller.reload(); await controller.select(liveTvId(100));
        const before = fetchPage.mock.calls.length;
        scenario.liveTv.status = status;
        await controller.programmes('next');
        expect(controller.getState()).toMatchObject({ channels: [], programmes: [], context: null, selected: null });
        expect(controller.getState().error).not.toBeNull();
        expect(fetchPage).toHaveBeenCalledTimes(before + 1);
        expect(revalidate).toHaveBeenCalledTimes(status === 401 ? 1 : 0);
        controller.dispose();
    });
    it('honors Retry-After admission and makes explicit reload start at First', async () => {
        let clock = now();
        const { scenario, fetchPage } = setup();
        const controller = createCatalogController({ profileId: scenario.activeProfileId, fetchPage, now: () => clock });
        scenario.liveTv.status = 503; scenario.liveTv.retryAfter = '5';
        await controller.reload(); await controller.reload();
        expect(fetchPage).toHaveBeenCalledTimes(1);
        clock += 5000; scenario.liveTv.status = 200;
        await controller.reload();
        expect(fetchPage).toHaveBeenCalledTimes(2);
        expect(fetchPage.mock.calls[1][1].cursor).toBeNull();
        controller.dispose();
    });
    it('does not publish stale success or failure after selection cancellation or disposal', async () => {
        const { controller, fetchPage } = setup();
        await controller.reload();
        let finish;
        fetchPage.mockImplementationOnce(() => new Promise((resolve) => { finish = resolve; }));
        const obsolete = controller.select(liveTvId(100));
        const signal = fetchPage.mock.calls.at(-1)[2];
        await controller.select(liveTvId(101));
        expect(signal.aborted).toBe(true);
        finish({ bad: 'obsolete' }); await obsolete;
        expect(controller.getState().selected.id).toBe(liveTvId(101));
        expect(controller.getState().error).toBeNull();
        controller.dispose();
        await controller.reload();
        expect(controller.getState().context).toBeNull();
    });
    it('ends a hung operation at ten seconds and clears the view', async () => {
        vi.useFakeTimers();
        const { controller, fetchPage } = setup();
        fetchPage.mockImplementationOnce(() => new Promise(() => {}));
        const pending = controller.reload();
        await vi.advanceTimersByTimeAsync(10_000);
        await pending;
        expect(fetchPage.mock.calls[0][2].aborted).toBe(true);
        expect(controller.getState()).toMatchObject({ channels: [], channelsLoading: false, error: { kind: 'failed' } });
        controller.dispose();
    });
    it('ignores overlapping page actions without cancelling their current request', async () => {
        const { controller, fetchPage } = setup();
        await controller.reload(); await controller.select(liveTvId(100));
        let finish;
        const body = await fetchPage('channels', { cursor: 'fixture-channels:20' }, null);
        fetchPage.mockImplementationOnce(() => new Promise((resolve) => { finish = resolve; }));
        const pending = controller.channels('next');
        const before = fetchPage.mock.calls.length;
        const signal = fetchPage.mock.calls.at(-1)[2];
        await controller.channels('next');
        expect(fetchPage).toHaveBeenCalledTimes(before);
        expect(signal.aborted).toBe(false);
        finish(body); await pending;
        expect(controller.getState().channelPage.index).toBe(1);
        controller.dispose();
    });
    it('ignores a late 401 rejection after a newer guide request and after disposal', async () => {
        const { controller, fetchPage, revalidate } = setup();
        await controller.reload();
        let reject;
        fetchPage.mockImplementationOnce(() => new Promise((_resolve, fail) => { reject = fail; }));
        const obsolete = controller.select(liveTvId(100));
        await controller.select(liveTvId(101));
        reject({ status: 401 }); await obsolete;
        expect(revalidate).not.toHaveBeenCalled();
        expect(controller.getState().selected.id).toBe(liveTvId(101));
        fetchPage.mockImplementationOnce(() => new Promise((_resolve, fail) => { reject = fail; }));
        const leaving = controller.select(liveTvId(100));
        controller.dispose(); reject({ status: 401 }); await leaving;
        expect(revalidate).not.toHaveBeenCalled();
        expect(controller.getState().context).toBeNull();
    });
});

describe('bounded history and faithful presentation', () => {
    it('bounds history to 64 entries, rejects repeated cursors and permits Previous', () => {
        const pager = createCatalogPaging();
        for (let n = 0; n < 63; n++) { pager.accept(`cursor-${n}`); pager.cursor('next'); }
        pager.accept('cursor-63');
        expect(pager.canNext).toBe(false); expect(pager.atLimit).toBe(true);
        expect(pager.cursor('next')).toBe('cursor-62');
        expect(() => pager.accept('cursor-62')).toThrow();
        pager.cursor('previous'); expect(() => pager.accept('cursor-62')).not.toThrow();
        expect(pager.cursor('first')).toBeNull(); expect(pager.index).toBe(0);
    });
    it('preserves microseconds, rejects impossible instants and uses six elapsed DST hours', () => {
        expect(instant('2026-01-01T00:00:00.000001Z').micros - instant('2026-01-01T00:00:00Z').micros).toBe(1n);
        for (const value of ['2026-02-30T00:00:00Z', '2026-01-01T24:00:00Z', '2026-01-01T00:00:60Z', '202610', '2026-01-01T00:00:00.1234567Z']) expect(() => instant(value)).toThrow();
        const window = sixHourWindow(now());
        expect(instant(window.to).micros - instant(window.from).micros).toBe(21_600_000_000n);
        expect(moveWindow(window, 1).from).toBe(window.to);
        expect(moveWindow(window, -1).to).toBe(window.from);
        expect(formatInstant('202610', 'en')).toBe('202610');
    });
    it('uses exact, base, untagged then first text while preserving raw unknown labels', () => {
        const variants = [{ language: 'fr', text: 'Premier' }, { language: 'en-GB', text: 'British' }, { language: null, text: 'Untagged' }];
        expect(preferredText(variants, 'en-US').text).toBe('British');
        expect(preferredText(variants, 'ar').text).toBe('Untagged');
        expect(preferredText(variants.slice(0, 2), 'ar').text).toBe('Premier');
        expect(preferredText([], 'en')).toBeNull();
        expect(textLanguage('EN-gb')).toBe('en-GB');
        expect(textLanguage('english')).toBe('english');
        for (const label of [null, '<ar>', 'en_US', 'x-private']) expect(textLanguage(label)).toBe('');
    });
});

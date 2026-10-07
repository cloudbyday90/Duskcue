import { randomUUID } from 'node:crypto';
import { expect } from '@playwright/test';

export function createQualificationEvents({ tokens, scenario }) {
    const records = [];
    const pending = new Map();
    const armed = new Map();
    let disposed = false;
    const pendingRecords = (origin) => [...pending.values()].filter((entry) => entry.record.origin === origin).map((entry) => entry.record);

    return {
        records,
        arm(origin) {
            if (!tokens.has(origin) || disposed) throw new Error('Arm only an active qualification origin.');
            const payload = {
                id: randomUUID(), notification_type: 'fixture_delivery', category: 'system', priority: 'low',
                title: 'Native SSE fixture delivery', body: `Finite event from ${origin}`, link: null,
                created_at: new Date().toISOString(),
            };
            const eventId = `native-fixture-${randomUUID()}`;
            armed.set(origin, { payload, eventId });
            return { payload, eventId };
        },
        pendingRecords,
        pendingCount: (origin) => pendingRecords(origin).length,
        deliver(origin) {
            const entries = [...pending.entries()].filter(([_request, entry]) => entry.record.origin === origin);
            const frame = armed.get(origin);
            if (entries.length !== 1 || !frame) throw new Error('Deliver only an armed frame to exactly one live selected-origin SSE request.');
            armed.delete(origin);
            pending.delete(entries[0][0]);
            entries[0][1].release({ kind: 'frame', frame });
        },
        requestFailed(request) {
            const waiting = pending.get(request);
            if (!waiting) return;
            pending.delete(request);
            waiting.record.aborted = true;
            waiting.record.failure = request.failure()?.errorText || 'request_failed';
            waiting.release({ kind: 'cancelled' });
        },
        async handle(route) {
            const request = route.request();
            const url = new URL(request.url());
            const path = url.pathname.replace('/api/v1', '');
            if (request.method() !== 'GET' || !['/events', '/notifications', '/notifications/unread-count'].includes(path)) return false;
            const headers = await request.allHeaders();
            const authenticated = headers.authorization === `Bearer ${tokens.get(url.origin)}`;
            const queryKeys = [...url.searchParams.keys()].sort();
            const record = { origin: url.origin, path, authenticated, accept: headers.accept || null, lastEventId: headers['last-event-id'] || null, queryKeys, status: authenticated ? 200 : 401 };
            records.push(record);
            scenario.requests.push({ method: 'GET', path, body: null, query: Object.fromEntries(queryKeys.map((key) => [key, '[redacted]'])) });
            if (!authenticated || !tokens.has(url.origin)) {
                await route.fulfill({ status: 401, contentType: 'application/problem+json', body: JSON.stringify({ status: 401, title: 'FIXTURE_AUTH', detail: 'Selected-origin SSE fixture bearer required' }) });
                return true;
            }
            if (path !== '/events') {
                const body = path === '/notifications' ? { items: [], cursor: null, has_more: false } : { unread_count: 0 };
                record.returnedItemCount = 0;
                await route.fulfill({ contentType: 'application/json', body: JSON.stringify(body) });
                return true;
            }
            if (disposed) return true;
            record.requestId = randomUUID();
            const resolution = await new Promise((release) => { pending.set(request, { release, record }); });
            if (resolution.kind !== 'frame' || disposed) return true;
            const { frame } = resolution;
            const body = `retry: 1000\nid: ${frame.eventId}\nevent: notification\ndata: ${JSON.stringify(frame.payload)}\n\n`;
            record.eventId = frame.eventId;
            record.notificationId = frame.payload.id;
            try {
                await route.fulfill({ contentType: 'text/event-stream', headers: { 'Cache-Control': 'no-store' }, body });
                record.fulfilled = true;
            } catch (error) {
                record.fulfilled = false;
                if (!disposed) throw error;
            }
            return true;
        },
        dispose() {
            disposed = true;
            armed.clear();
            for (const waiting of pending.values()) waiting.release({ kind: 'cancelled' });
            pending.clear();
        },
    };
}

export async function exerciseNativeEvents(page, api, origin, screenshot, suffix) {
    const frame = api.events.arm(origin);
    await expect(page.getByRole('heading', { name: 'Continue watching', exact: true })).toBeVisible();
    await expect.poll(() => api.events.pendingCount(origin)).toBe(1);
    const bell = page.getByRole('button', { name: 'Notifications', exact: true });
    await bell.click();
    await expect(page.getByText("You're all caught up", { exact: true })).toBeVisible();
    const metadataRequests = api.events.records.filter((request) => request.origin === origin && request.path === '/notifications');
    expect(metadataRequests.length).toBeGreaterThan(0);
    for (const request of metadataRequests) expect(request).toMatchObject({ origin, authenticated: true, status: 200, returnedItemCount: 0 });
    api.events.deliver(origin);
    await expect(bell.getByText('1', { exact: true })).toBeVisible();
    await expect(page.getByText(frame.payload.title, { exact: true })).toBeVisible();
    await expect(page.getByText(frame.payload.body, { exact: true })).toBeVisible();
    await expect.poll(() => api.events.records.some((request) => request.eventId === frame.eventId && request.fulfilled)).toBe(true);
    const request = api.events.records.find((entry) => entry.eventId === frame.eventId);
    expect(request).toMatchObject({ origin, path: '/events', authenticated: true, accept: 'text/event-stream', lastEventId: null, queryKeys: [], fulfilled: true });
    await screenshot(`native-sse-${suffix}`);
    await page.keyboard.press('Escape');
    await expect(bell).toHaveAttribute('aria-expanded', 'false');
    await expect.poll(() => api.events.pendingCount(origin), { timeout: 5_000 }).toBe(1);
    expect(api.events.pendingRecords(origin)[0]).toMatchObject({ origin, authenticated: true, lastEventId: frame.eventId });
    return { origin, eventId: frame.eventId, notificationId: frame.payload.id, authenticatedSelectedOrigin: true, actualFiniteFrameParsed: true, rootNotificationBadgeAndContent: true, metadataListEmpty: true, actualReconnectPending: true, clientLastEventIdRetained: true, replayQualification: false };
}

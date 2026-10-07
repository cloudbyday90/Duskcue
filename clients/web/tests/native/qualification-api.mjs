import { randomUUID } from 'node:crypto';
import { createApiScenario, installApiFixture } from '../fixtures/api.js';
import { ensurePlaybackMedia } from '../fixtures/playback-media.mjs';
import { installPlaybackApi } from '../fixtures/playback-api.mjs';
import { createQualificationEvents } from './qualification-events.mjs';
import { createNativePreferenceFixture } from './qualification-viewing-preferences.mjs';

export async function installQualificationApi(page, manifest) {
    const scenario = createApiScenario({ authenticated: false });
    for (const files of Object.values(scenario.files)) {
        for (const file of files) {
            file.audio_bitrate = 48_000;
            file.additional_streams = { audio: [{ index: 1, codec: 'aac', channels: 2, language: 'eng', title: null, bitrate: 48_000, is_default: null, is_audio_description: null }], subtitles: [] };
        }
    }
    const tokens = new Map(manifest.origins.map((origin) => [origin, `tonight-fixture-${randomUUID()}`]));
    const authRequests = [];
    const transportRequests = [];
    const transportErrors = [];
    const headerReads = new Set();
    let observing = true;
    const credentials = { username: 'tonight-native-fixture', password: randomUUID() };
    const events = createQualificationEvents({ tokens, scenario });
    await installApiFixture({ route: page.route.bind(page), addInitScript: async () => {} }, scenario);
    const media = await ensurePlaybackMedia();
    const playback = await installPlaybackApi(page, scenario, media, { autoplay: false, mode: 'transcode' });
    const viewing = createNativePreferenceFixture(scenario, manifest.origins);
    const failedRequest = (request) => events.requestFailed(request);
    const observedRequest = (request) => {
        const url = new URL(request.url());
        if (!url.pathname.startsWith('/api/v1/')) return;
        const task = request.allHeaders().then((headers) => {
            transportRequests.push({ method: request.method(), path: url.pathname.replace('/api/v1', ''), origin: url.origin, originAllowed: tokens.has(url.origin), authenticated: headers.authorization === `Bearer ${tokens.get(url.origin)}` });
        }).catch((error) => { if (observing) transportErrors.push({ path: url.pathname, message: error.message }); });
        headerReads.add(task);
        task.finally(() => headerReads.delete(task));
    };
    page.on('requestfailed', failedRequest);
    page.on('request', observedRequest);

    await page.route(/\/api\/v1(?:\/|$)/, async (route) => {
        const request = route.request();
        const url = new URL(request.url());
        const path = url.pathname.replace('/api/v1', '');
        const method = request.method();
        const headers = await request.allHeaders();
        const authorization = headers.authorization;
        const originAllowed = tokens.has(url.origin);
        if (!originAllowed) throw new Error(`The native client requested an unexpected API origin: ${url.origin}`);
        const json = (body, status = 200) => route.fulfill({ status, contentType: 'application/json', body: JSON.stringify(body) });
        if (await events.handle(route)) return;
        if (await viewing.handle(route)) return;
        if (method === 'POST' && path === '/auth/login') {
            const body = request.postDataJSON();
            authRequests.push({ origin: url.origin, clientPlatform: body.client_platform, deviceIdPresent: typeof body.device_id === 'string', deviceId: body.device_id ?? null });
            if (body.username !== credentials.username || body.password !== credentials.password || body.client_platform !== 'desktop') return json({ status: 401, title: 'FIXTURE_AUTH', detail: 'Fixture credentials required' }, 401);
            return json({ session_token: tokens.get(url.origin), user: scenario.user });
        }
        if (method === 'GET' && path === '/user/sessions') {
            if (authorization !== `Bearer ${tokens.get(url.origin)}`) return json({ status: 401, title: 'FIXTURE_AUTH', detail: 'Fixture desktop bearer required' }, 401);
            return json({ items: [] });
        }
        if (path === '/user/preferences' && ['GET', 'PATCH'].includes(method)) return json({ locale: 'en', available_locales: [{ tag: 'en', name: 'English' }] });
        return route.fallback();
    });
    return {
        scenario, credentials, tokens, authRequests, transportRequests, transportErrors, media, events, viewing,
        getPlayback: () => playback,
        async disposeObservation() {
            observing = false;
            page.off('request', observedRequest);
            page.off('requestfailed', failedRequest);
            let timer;
            try {
                await Promise.race([Promise.all([...headerReads]), new Promise((_resolve, reject) => { timer = setTimeout(() => reject(new Error('Native request observations did not drain.')), 2_000); })]);
            } finally { clearTimeout(timer); }
        },
    };
}

export function invokeNative(page, command, args = {}) {
    return page.evaluate(({ command, args }) => window.__TAURI_INTERNALS__.invoke(command, args), { command, args });
}

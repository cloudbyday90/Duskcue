import { createCatalog, mediaIds, profileIds } from './catalog.js';
import { accessibleItems, browsingResponse } from './browsing.js';
import { searchResponse } from './searching.js';

export function createApiScenario(options = {}) {
    const catalog = createCatalog();
    const activeProfileId = options.activeProfileId || profileIds.alex;
    return {
        ...catalog,
        activeProfileId,
        user: { ...catalog.user, role: options.userRole || catalog.user.role, active_profile_id: activeProfileId, profile_selection_required: !!options.selectionRequired },
        selectionRequired: !!options.selectionRequired,
        parentUnlockRequired: activeProfileId === profileIds.kids,
        rememberedProfileId: null,
        deviceCanRemember: true,
        emptyCatalog: !!options.emptyCatalog,
        browsePageSize: options.browsePageSize || 24,
        authenticated: options.authenticated !== false,
        preferencesByProfile: Object.fromEntries(catalog.profiles.map((profile) => [profile.id, {
            profile_id: profile.id,
            has_saved_preferences: false,
            viewing_preferences: { autoplay_next_episode: true, audio_language: null, prefer_audio_description: false, subtitle_mode: 'none', subtitle_language: null, prefer_sdh: false },
        }])),
        failures: new Map(),
        requests: [],
        unhandledRequests: [],
    };
}

function problem(status, detail, title = 'FIXTURE_ERROR') {
    return { type: '/errors/fixture', title, status, detail, trace_id: 'fixture-trace' };
}

export async function installApiFixture(page, scenario) {
    await page.addInitScript(({ user, authenticated }) => {
        if (authenticated && !localStorage.getItem('duskcue_user')) {
            localStorage.setItem('duskcue_user', JSON.stringify(user));
        }
        localStorage.setItem('duskcue_device_id', 'fixture-web-device');
    }, { user: scenario.user, authenticated: scenario.authenticated });

    await page.route(/\/api\/v1(?:\/|$)/, async (route) => {
        const request = route.request();
        const url = new URL(request.url());
        const path = url.pathname.replace(/^\/api\/v1/, '');
        const method = request.method();
        const body = request.postData() ? request.postDataJSON() : null;
        scenario.requests.push({ method, path, body, query: Object.fromEntries(url.searchParams) });
        const json = (value, status = 200) => route.fulfill({
            status,
            contentType: status >= 400 ? 'application/problem+json' : 'application/json',
            body: JSON.stringify(value),
        });
        const failure = scenario.failures.get(`${method} ${path}`);
        if (failure) return json(problem(failure.status, failure.detail), failure.status);

        if (path === '/setup/status' && method === 'GET') return json({ setup_required: false });
        if (path === '/events' && method === 'GET') {
            return route.fulfill({ contentType: 'text/event-stream', body: ': fixture connection\n\n' });
        }
        if (path === '/notifications/unread-count' && method === 'GET') return json({ unread_count: 0 });
        if (path === '/notifications' && method === 'GET') return json({ items: [], cursor: null, has_more: false });
        if (path === '/libraries' && method === 'GET') return json({ items: scenario.libraries, cursor: null, has_more: false });
        if (path === '/search' && method === 'GET') {
            const response = searchResponse(scenario, url);
            return json(response.body, response.status || 200);
        }
        if (/^\/libraries\/[^/]+\/scan$/.test(path) && method === 'POST') return json({ status: 'scan_completed' });
        if (path === '/auth/logout' && method === 'POST') {
            scenario.authenticated = false;
            return route.fulfill({ status: 204 });
        }
        if (path === '/profiles' && method === 'GET') {
            return json({
                active_profile_id: scenario.activeProfileId,
                profile_selection_required: scenario.selectionRequired,
                remembered_profile_id: scenario.rememberedProfileId,
                device_can_remember_profile: scenario.deviceCanRemember,
                parent_unlock_required: scenario.parentUnlockRequired,
                parent_unlock_expires_at: null,
                items: scenario.profiles,
            });
        }
        if (path === '/profiles/current/viewing-preferences' && ['GET', 'PATCH'].includes(method)) {
            if (scenario.selectionRequired) return json(problem(409, 'Fixture profile selection required'), 409);
            const preferences = scenario.preferencesByProfile[scenario.activeProfileId];
            if (method === 'PATCH') {
                if (body?.expected_profile_id !== scenario.activeProfileId) return json(problem(409, 'Fixture profile changed'), 409);
                preferences.viewing_preferences = { ...body.viewing_preferences };
                preferences.has_saved_preferences = true;
            }
            return json(preferences);
        }
        if (path === '/profiles/parent-unlock' && method === 'POST') {
            if (body?.pin !== '1357') return json(problem(403, 'Fixture parent PIN rejected', 'PARENT_PIN_INVALID'), 403);
            scenario.parentUnlockRequired = false;
            return json({ unlocked_until: '2030-01-01T00:10:00Z' });
        }
        const switchMatch = path.match(/^\/profiles\/([^/]+)\/switch$/);
        if (switchMatch && method === 'POST') {
            const profile = scenario.profiles.find((entry) => entry.id === switchMatch[1]);
            if (!profile) return json(problem(404, 'Fixture profile not found'), 404);
            if (scenario.parentUnlockRequired && profile.profile_type === 'standard') return json(problem(403, 'Parent unlock required'), 403);
            scenario.activeProfileId = profile.id;
            scenario.selectionRequired = false;
            scenario.parentUnlockRequired = profile.profile_type === 'kids';
            if (body?.remember_on_device === true) scenario.rememberedProfileId = profile.id;
            if (body?.remember_on_device === false) scenario.rememberedProfileId = null;
            return json({
                active_profile: profile,
                profile_selection_required: false,
                remembered_profile_id: scenario.rememberedProfileId,
                device_can_remember_profile: scenario.deviceCanRemember,
                parent_unlock_required: scenario.parentUnlockRequired,
                parent_unlock_expires_at: null,
            });
        }

        const allowedItems = accessibleItems(scenario);
        const browse = method === 'GET' ? browsingResponse(scenario, path, url) : null;
        if (browse) return json(browse.body, browse.status || 200);
        const mediaMatch = path.match(/^\/media-items\/([^/]+)(\/files)?$/);
        if (mediaMatch && method === 'GET') {
            const item = allowedItems.find((entry) => entry.id === mediaMatch[1]);
            if (!item) return json(problem(404, 'Fixture media unavailable'), 404);
            return json(mediaMatch[2] ? { items: scenario.files[item.id] } : item);
        }
        const watchMatch = path.match(/^\/items\/([^/]+)\/watch-data$/);
        if (watchMatch && ['GET', 'PUT'].includes(method)) {
            const item = allowedItems.find((entry) => entry.id === watchMatch[1]);
            if (!item) return json(problem(404, 'Fixture watch data unavailable'), 404);
            const watch = scenario.watchByProfile[scenario.activeProfileId][item.id];
            if (method === 'PUT') Object.assign(watch, body);
            return json(watch);
        }
        if (/^\/items\/[^/]+\/artwork\/[^/]+$/.test(path) && method === 'GET') {
            return route.fulfill({ status: 404 });
        }

        scenario.unhandledRequests.push(`${method} ${url.pathname}${url.search}`);
        return json(problem(501, 'Unhandled fixture API request'), 501);
    });
}

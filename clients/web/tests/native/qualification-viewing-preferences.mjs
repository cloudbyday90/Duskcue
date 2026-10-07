import { profileIds } from '../fixtures/catalog.js';

export function createNativePreferenceFixture(scenario, origins) {
    const byOrigin = new Map(origins.map((origin, index) => [origin, index === 0 ? scenario.preferencesByProfile : structuredClone(scenario.preferencesByProfile)]));
    byOrigin.get(origins[1])[profileIds.alex].has_saved_preferences = true;
    Object.assign(byOrigin.get(origins[1])[profileIds.alex].viewing_preferences, { autoplay_next_episode: true, audio_language: 'fr' });
    return {
        byOrigin,
        async handle(route) {
            const request = route.request();
            const url = new URL(request.url());
            const path = url.pathname.replace('/api/v1', '');
            const method = request.method();
            if (path !== '/profiles/current/viewing-preferences' || !['GET', 'PATCH'].includes(method)) return false;
            const body = method === 'PATCH' ? request.postDataJSON() : null;
            scenario.requests.push({ method, path, body, query: {} });
            const row = byOrigin.get(url.origin)?.[scenario.activeProfileId];
            if (!row || scenario.selectionRequired || (body && body.expected_profile_id !== scenario.activeProfileId)) {
                await route.fulfill({ status: 409, contentType: 'application/problem+json', body: JSON.stringify({ status: 409, title: 'FIXTURE_PROFILE_SCOPE', detail: 'The fixture profile must be selected before reading or saving defaults.' }) });
                return true;
            }
            if (body) {
                row.has_saved_preferences = true;
                row.viewing_preferences = { ...body.viewing_preferences };
            }
            await route.fulfill({ contentType: 'application/json', body: JSON.stringify(row) });
            return true;
        },
    };
}

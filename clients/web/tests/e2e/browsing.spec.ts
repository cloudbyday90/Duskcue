import { test, expect } from './fixtures';
import { collectionId, libraryId, mediaIds, profileIds } from '../fixtures/catalog.js';

test.describe('catalog pagination context', () => {
    test.use({ scenarioOptions: { browsePageSize: 1 } });

    test('retains the complete origin and replays pages when returning from a title', async ({ page, api }) => {
        await page.goto('/media?type=movie');
        await expect(page.getByRole('heading', { name: 'Movies', exact: true })).toBeVisible();
        await expect(page.getByRole('link', { name: /^The Lighthouse Archive/ })).toBeVisible();
        await page.getByRole('button', { name: 'Load more', exact: true }).click();
        await expect(page.getByRole('link', { name: /^Completed Journey/ })).toBeVisible();
        await expect(page).toHaveURL(/pages=2/);
        const origin = new URL(page.url()).pathname + new URL(page.url()).search;
        const titleLink = page.getByRole('link', { name: /^Completed Journey/ });
        expect(new URL(await titleLink.getAttribute('href') as string, page.url()).searchParams.get('from')).toBe(origin);

        await titleLink.click();
        await expect(page.getByRole('heading', { name: 'Completed Journey', exact: true })).toBeVisible();
        await page.getByRole('link', { name: /Back to browsing$/ }).click();

        await expect(page.getByRole('link', { name: /^The Lighthouse Archive/ })).toBeVisible();
        await expect(page.getByRole('link', { name: /^Completed Journey/ })).toBeVisible();
        await page.reload();
        await expect(page.locator('.result-status')).toContainText('2 titles loaded.');
        expect(api.requests.filter((request) => request.path === '/media-items' && request.query.cursor).length).toBeGreaterThanOrEqual(3);
    });

    test('keeps loaded cards usable when another page fails and retries its cursor', async ({ page, api }) => {
        await page.goto('/media?type=movie');
        await expect(page.getByRole('link', { name: /^The Lighthouse Archive/ })).toBeVisible();
        api.failures.set('GET /media-items', { status: 503, detail: 'Fixture continuation unavailable' });
        await page.getByRole('button', { name: 'Load more', exact: true }).click();
        await expect(page.getByRole('alert')).toContainText('Your current titles are still here.');
        await expect(page.getByRole('link', { name: /^The Lighthouse Archive/ })).toBeVisible();

        api.failures.delete('GET /media-items');
        await page.getByRole('button', { name: 'Try again', exact: true }).click();

        await expect(page.getByRole('link', { name: /^Completed Journey/ })).toBeVisible();
        await expect(page.getByRole('alert')).toHaveCount(0);
        await expect(page).toHaveURL(/pages=2/);
    });
});

test('native catalog controls send server filters and keep keyboard focus', async ({ page, api }) => {
    api.watchByProfile[profileIds.alex][mediaIds.completedMovie].is_favorite = true;
    await page.goto('/media?type=movie&pages=3');
    await expect(page.getByRole('link', { name: /^Completed Journey/ })).toBeVisible();
    const watch = page.getByLabel('Watch status', { exact: true });
    await watch.focus();
    await watch.selectOption('watched');
    await expect(watch).toBeFocused();
    await expect(page.getByRole('link', { name: /^Completed Journey/ })).toBeVisible();
    await expect(page.getByRole('link', { name: /^The Lighthouse Archive/ })).toHaveCount(0);
    await page.getByLabel('Favorites', { exact: true }).selectOption('true');
    await page.getByLabel('Sort by', { exact: true }).selectOption('title:asc');
    await expect(page).toHaveURL(/sort=title/);
    expect(new URL(page.url()).searchParams.has('pages')).toBe(false);
    await expect.poll(() => api.requests.some((request) => request.path === '/media-items' && request.query.watch === 'watched' && request.query.favorite === 'true' && request.query.sort === 'title' && request.query.order === 'asc')).toBe(true);
});

test('accessible library choices traverse all pages and publish the selected library in the URL', async ({ page, api }) => {
    api.browsePageSize = 1;
    await page.goto('/media');
    const library = page.getByLabel('Library', { exact: true });
    await expect(library).toBeEnabled();
    await expect(library.locator('option')).toHaveCount(3);
    await library.selectOption(libraryId);
    await expect(page).toHaveURL(new RegExp(`library_id=${libraryId}`));
    await expect.poll(() => api.requests.some((request) => request.path === '/media-items' && request.query.library_id === libraryId)).toBe(true);
    expect(api.requests.some((request) => request.path === '/browse/libraries' && request.query.cursor)).toBe(true);
});

test('ordinary library list uses scoped reads and restores list pagination after detail navigation', async ({ page, api }) => {
    api.browsePageSize = 1;
    await page.goto('/libraries');
    await expect(page.getByRole('heading', { name: 'Libraries', exact: true })).toBeVisible();
    await expect(page.getByRole('link', { name: 'Library Management', exact: true })).toHaveCount(0);
    await page.getByRole('button', { name: 'Load more', exact: true }).click();
    await expect(page.getByRole('link', { name: /^Fixture TV/ })).toBeVisible();
    await page.getByRole('link', { name: /^Fixture movies/ }).click();
    await expect(page.getByRole('heading', { name: 'Fixture movies', exact: true })).toBeVisible();
    await page.locator('.scope-page').getByRole('link', { name: /Libraries$/ }).click();
    await expect(page.getByRole('link', { name: /^Fixture TV/ })).toBeVisible();
    await expect(page).toHaveURL(/\/libraries\?pages=2$/);
    expect(api.requests.some((request) => request.path === '/browse/libraries' && request.query.cursor)).toBe(true);
    expect(api.requests.some((request) => request.path === '/libraries')).toBe(false);
});

test.describe('submitted search and real facets', () => {
    test.use({ scenarioOptions: { browsePageSize: 1 } });

    test('fetches only submitted queries and preserves facets, sorting, pagination and title origin', async ({ page, api }) => {
        await page.goto('/search?q=Harbor&sort=relevance&order=asc');
        await expect(page.getByRole('heading', { name: 'Results for “Harbor”', exact: true })).toBeVisible();
        await expect(page.getByLabel('Sort by', { exact: true })).toHaveValue('relevance:asc');
        await expect(page.getByLabel('Genre', { exact: true }).locator('option[value="drama"]')).toHaveText('Drama (3)');
        await page.getByRole('button', { name: 'Load more', exact: true }).click();
        await expect(page.locator('.result-status')).toContainText('2 titles loaded.');
        await expect(page).toHaveURL(/pages=2/);
        const input = page.getByRole('searchbox', { name: 'Search your library', exact: true });
        const count = api.requests.filter((request) => request.path === '/search').length;
        await input.fill('Cloud');
        expect(api.requests.filter((request) => request.path === '/search')).toHaveLength(count);
        await expect(page.getByRole('heading', { name: 'Results for “Harbor”', exact: true })).toBeVisible();

        await input.press('Enter');

        await expect(page.getByRole('heading', { name: 'Results for “Cloud”', exact: true })).toBeVisible();
        await expect(page.getByRole('link', { name: /^Cloud Parade/ })).toBeVisible();
        expect(new URL(page.url()).searchParams.has('pages')).toBe(false);
        await page.getByLabel('Genre', { exact: true }).selectOption('family');
        await page.getByLabel('Release year', { exact: true }).selectOption('2026');
        await page.getByLabel('Minimum rating', { exact: true }).selectOption('7');
        await page.getByLabel('Sort by', { exact: true }).selectOption('title:asc');
        await expect.poll(() => api.requests.some((request) => request.path === '/search' && request.query.q === 'Cloud' && request.query.genre === 'family' && request.query.year === '2026' && request.query.rating_min === '7' && request.query.sort === 'title')).toBe(true);
        const title = page.getByRole('link', { name: /^Cloud Parade/ });
        const origin = new URL(page.url()).pathname + new URL(page.url()).search;
        expect(new URL(await title.getAttribute('href') as string, page.url()).searchParams.get('from')).toBe(origin);
        await title.click();
        await expect(page.getByRole('heading', { name: 'Cloud Parade', exact: true })).toBeVisible();
        await page.getByRole('link', { name: /Back to browsing$/ }).click();
        await expect(page.getByLabel('Genre', { exact: true })).toHaveValue('family');
        await expect(page.getByLabel('Sort by', { exact: true })).toHaveValue('title:asc');
    });
});

test('search empty results and request failures have distinct states and retry', async ({ page, api }) => {
    await page.goto('/search?q=no-such-fixture-title');
    await expect(page.getByRole('heading', { name: 'No titles match your search.', exact: true })).toBeVisible();
    await expect(page.getByRole('alert')).toHaveCount(0);
    api.failures.set('GET /search', { status: 503, detail: 'Fixture search unavailable' });
    await page.goto('/search?q=Cloud');
    await expect(page.getByRole('alert')).toContainText('These titles couldn’t be loaded.');
    api.failures.delete('GET /search');

    await page.getByRole('button', { name: 'Try again', exact: true }).click();

    await expect(page.getByRole('link', { name: /^Cloud Parade/ })).toBeVisible();
});

test('disabled profile search is explained before requesting results', async ({ page, api }) => {
    api.profiles.find((profile) => profile.id === profileIds.alex)!.allow_search = false;
    await page.goto('/search?q=Cloud');
    await expect(page.getByText('Search is unavailable for this profile.', { exact: true })).toBeVisible();
    await expect(page.getByRole('searchbox', { name: 'Search your library', exact: true })).toHaveCount(0);
    expect(api.requests.some((request) => request.path === '/search')).toBe(false);
});

test('search and native filters reflow at 320 CSS pixels with RTL and reduced motion', async ({ page, api }) => {
    await page.setViewportSize({ width: 320, height: 900 });
    await page.emulateMedia({ reducedMotion: 'reduce' });
    await page.goto('/search?q=Cloud');
    await expect(page.getByRole('link', { name: /^Cloud Parade/ })).toBeVisible();
    await page.evaluate(() => document.documentElement.setAttribute('dir', 'rtl'));
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
    const format = page.getByLabel('Format', { exact: true });
    await format.focus();
    await format.selectOption('movie');
    await expect(format).toBeFocused();
    await expect(page.getByRole('link', { name: /^Cloud Parade/ })).toBeVisible();
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
});

test.describe('authorized library scan', () => {
    test.use({ scenarioOptions: { userRole: 'owner' } });

test('library pages read scoped metadata and preserve scan for authorized standard profiles', async ({ page, api }) => {
    await page.goto(`/libraries/${libraryId}?type=movie`);
    await expect(page.getByRole('heading', { name: 'Fixture movies', exact: true })).toBeVisible();
    await expect(page.getByRole('link', { name: /^Harbor Stories/ })).toHaveCount(0);
    expect(api.requests.some((request) => request.path === `/browse/libraries/${libraryId}`)).toBe(true);
    expect(api.requests.some((request) => request.path === '/media-items' && request.query.library_id === libraryId)).toBe(true);
    expect(api.requests.some((request) => request.path === '/libraries')).toBe(false);
    const before = api.requests.filter((request) => request.path === '/media-items').length;

    await page.getByRole('button', { name: 'Scan library', exact: true }).click();

    await expect(page.getByText('Library scan complete.', { exact: true })).toBeVisible();
    await expect.poll(() => api.requests.filter((request) => request.path === '/media-items').length).toBeGreaterThan(before);
});

test('library management remains reachable for an authorized standard profile', async ({ page, api }) => {
    await page.goto('/libraries');
    await expect(page.getByRole('link', { name: 'Library Management', exact: true })).toHaveAttribute('href', '/settings/libraries');
});
});

test('collections list opens actual scoped membership pages with return context', async ({ page, api }) => {
    api.browsePageSize = 1;
    api.collections.push({ ...api.collections[0], id: collectionId.replace(/600$/, '601'), name: 'Another evening' });
    await page.goto('/collections');
    await expect(page.getByRole('heading', { name: 'Collections', exact: true })).toBeVisible();
    await page.getByRole('button', { name: 'Load more', exact: true }).click();
    await expect(page.getByRole('link', { name: /^Another evening/ })).toBeVisible();
    await expect(page).toHaveURL(/pages=2/);
    await page.getByRole('link', { name: /^Weekend stories/ }).click();
    await expect(page.getByRole('heading', { name: 'Weekend stories', exact: true })).toBeVisible();
    await expect(page.getByLabel('Sort by', { exact: true })).toHaveValue('position:asc');
    await expect(page.getByRole('link', { name: /^The Lighthouse Archive/ })).toBeVisible();
    const membership = api.requests.find((request) => request.path === `/browse/collections/${collectionId}/items`);
    expect(membership?.query.sort).toBeUndefined();
    await page.getByRole('button', { name: 'Load more', exact: true }).click();
    await expect(page.getByRole('link', { name: /^Completed Journey/ })).toBeVisible();
    const origin = new URL(page.url()).pathname + new URL(page.url()).search;
    expect(new URL(await page.getByRole('link', { name: /^Completed Journey/ }).getAttribute('href') as string, page.url()).searchParams.get('from')).toBe(origin);
    await page.getByRole('link', { name: /All collections$/ }).click();
    await expect(page.getByRole('link', { name: /^Another evening/ })).toBeVisible();
    await expect(page).toHaveURL(/\/collections\?pages=2$/);
});

test.describe('Kids scoped browsing', () => {
    test.use({ scenarioOptions: { activeProfileId: profileIds.kids, userRole: 'owner' } });

    test('does not expose scan or restricted collection titles even for the owner account', async ({ page, api }) => {
        await page.goto(`/libraries/${libraryId}`);
        await expect(page.getByRole('link', { name: /^Cloud Parade/ })).toBeVisible();
        await expect(page.getByRole('button', { name: 'Scan library', exact: true })).toHaveCount(0);
        await page.goto(`/collections/${collectionId}`);
        await expect(page.getByRole('link', { name: /^Cloud Parade/ })).toBeVisible();
        await expect(page.getByRole('link', { name: /^Completed Journey/ })).toHaveCount(0);
        await expect(page.getByText('1 titles', { exact: true })).toBeVisible();
    });
});

test('library metadata failure has an independent retry and does not become an empty catalog', async ({ page, api }) => {
    api.failures.set(`GET /browse/libraries/${libraryId}`, { status: 503, detail: 'Fixture metadata unavailable' });
    await page.goto(`/libraries/${libraryId}`);
    await expect(page.getByRole('alert')).toContainText('This library couldn’t be loaded.');
    await expect(page.getByRole('heading', { name: 'No titles match these filters.', exact: true })).toHaveCount(0);
    api.failures.delete(`GET /browse/libraries/${libraryId}`);

    await page.getByRole('button', { name: 'Try again', exact: true }).click();

    await expect(page.getByRole('heading', { name: 'Fixture movies', exact: true })).toBeVisible();
    await expect(page.getByRole('link', { name: /^The Lighthouse Archive/ })).toBeVisible();
});

test.describe('empty browsing', () => {
    test.use({ scenarioOptions: { emptyCatalog: true } });

    test('distinguishes an empty catalog and empty collections from API failures', async ({ page, api }) => {
        await page.goto('/media?type=movie');
        await expect(page.getByRole('heading', { name: 'No titles match these filters.', exact: true })).toBeVisible();
        await expect(page.getByRole('alert')).toHaveCount(0);
        await page.goto('/collections');
        await expect(page.getByRole('heading', { name: 'No collections are available for this profile yet.', exact: true })).toBeVisible();
        await expect(page.getByRole('alert')).toHaveCount(0);
    });
});

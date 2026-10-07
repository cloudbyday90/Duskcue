import { test, expect } from './playback-fixtures';
import { expectDecodedPlayback } from './playback-journey';
import fr from '../../messages/fr.json' with { type: 'json' };
import ar from '../../messages/ar.json' with { type: 'json' };
import { mediaIds } from '../fixtures/catalog.js';

for (const [locale, messages] of [['fr', fr], ['ar', ar]] as const) {
    test(`${locale} preview player keeps translated speed labels and identifies mixed names and ended status`, async ({ page, api, playback }) => {
        api.files[mediaIds.episodeOne][0].additional_streams = {
            audio: [{ index: 1, language: 'eng', codec: 'aac', disposition: { default: true, visual_impaired: true } }], subtitles: [],
        };
        await page.addInitScript(() => {
            document.addEventListener('ended', (event) => {
                if (event.isTrusted && event.target instanceof HTMLVideoElement) event.target.dataset.nativeEnded = 'true';
            }, true);
        });
        await page.goto(`/${locale}/play/${mediaIds.episodeOne}`);
        const region = page.getByRole('region', { name: messages.lib_components_player_media_player, exact: true });
        await expect(region).toBeVisible();
        await expect(page.getByRole('banner')).toBeHidden();
        const video = await expectDecodedPlayback(page);
        await expect(page.locator('html')).toHaveAttribute('lang', locale);
        await expect(page.locator('html')).toHaveAttribute('dir', locale === 'ar' ? 'rtl' : 'ltr');
        await region.getByRole('button', { name: messages.routes_settings_page_settings, exact: true }).click();
        const settings = region.getByRole('region', { name: messages.routes_settings_page_settings, exact: true });
        await expect(settings).toHaveAttribute('lang', locale);
        await expect(settings.getByText(messages.lib_components_player_playback_speed, { exact: true })).toHaveAttribute('lang', locale);
        await page.keyboard.press('Escape');
        const tracks = region.getByRole('button', { name: 'Audio & subtitles', exact: true });
        await expect(tracks).toHaveAttribute('lang', 'en');
        await tracks.click();
        const trackMenu = region.getByRole('region', { name: 'Audio & subtitles', exact: true });
        await expect(trackMenu).toHaveAttribute('lang', 'en');
        const languageName = new Intl.DisplayNames([locale], { type: 'language' }).of('en');
        await expect(trackMenu.getByText(languageName!, { exact: true })).toHaveAttribute('lang', locale);
        await expect(trackMenu.getByText('Audio description', { exact: true })).toHaveAttribute('lang', 'en');
        await page.keyboard.press('Escape');
        await video.evaluate((element: HTMLVideoElement) => { element.currentTime = element.duration - 0.25; });
        await expect(video).toHaveAttribute('data-native-ended', 'true', { timeout: 10_000 });
        const playNow = region.getByRole('button', { name: 'Play now', exact: true });
        await expect(playNow).toBeVisible();
        await playNow.focus();
        const announcement = region.getByRole('status').filter({ hasText: 'Automatic playback is paused while you interact.' });
        await expect(announcement).toHaveText('Automatic playback is paused while you interact.', { timeout: 10_000 });
        await expect(announcement).toHaveAttribute('lang', 'en');
        const languageEvidence = await page.evaluate(async () => {
            const runtimePath = '/src/lib/paraglide/runtime.js';
            const messagesPath = '/src/lib/paraglide/messages.js';
            const runtime = await import(runtimePath);
            const messages = await import(messagesPath);
            const buttons = [...document.querySelectorAll('.autoplay-card button')].map((element) => ({ text: element.textContent, lang: element.getAttribute('lang') }));
            return { pathname: window.location.pathname, locale: runtime.getLocale(), documentLanguage: document.documentElement.lang, cancelMessage: messages.tonight_autoplay_cancel(), buttons };
        });
        await test.info().attach('countdown-language', { body: JSON.stringify(languageEvidence, null, 2), contentType: 'application/json' });
        expect(languageEvidence.pathname).toBe(`/${locale}/play/${mediaIds.episodeOne}`);
        expect(languageEvidence.locale).toBe(locale);
        expect(languageEvidence.cancelMessage).toBe(messages.routes_settings_collections_page_cancel);
        const cancel = region.getByRole('button', { name: messages.routes_settings_collections_page_cancel, exact: true });
        await expect(cancel).toHaveAttribute('lang', locale);
        await cancel.click();
        const playNext = region.getByRole('button', { name: 'Play next', exact: true });
        await expect(playNext).toHaveAttribute('lang', 'en');
        await playNext.click();
        await expect.poll(() => new URL(page.url()).pathname).toBe(`/${locale}/play/${mediaIds.episodeTwo}`);
        await expectDecodedPlayback(page);
        expect(await page.evaluate(async () => {
            const runtimePath = '/src/lib/paraglide/runtime.js';
            return (await import(runtimePath)).getLocale();
        })).toBe(locale);
        const returnTo = new URL(new URL(page.url()).searchParams.get('return_to')!, page.url());
        expect(returnTo.pathname).toBe(`/media/${mediaIds.series}`);
        expect(returnTo.searchParams.get('season')).toBe(mediaIds.seasonOne);
        expect(returnTo.searchParams.get('episode')).toBe(mediaIds.episodeTwo);
        expect(playback.starts.map((start) => start.media_item_id)).toEqual([mediaIds.episodeOne, mediaIds.episodeTwo]);
        expect(playback.stops.map((stop) => stop.session_id)).toEqual([playback.starts[0].session_id]);
        await region.getByRole('button', { name: messages.lib_components_player_close_player, exact: true }).click();
        await expect.poll(() => new URL(page.url()).pathname).toBe(returnTo.pathname);
        expect([...new URL(page.url()).searchParams.entries()].sort()).toEqual([...returnTo.searchParams.entries()].sort());
        await expect(page.locator('#selected-episode-heading')).toHaveText('Episode 2 · The Last Ferry');
        await expect(page.locator('.selected-episode').getByRole('button', { name: /^(Play|Resume)$/ })).toBeEnabled();
        expect(playback.starts).toHaveLength(2);
        expect(playback.stops.map((stop) => stop.session_id)).toEqual(playback.starts.map((start) => start.session_id));
    });
}

import { join } from 'node:path';
import { recordNativeIconContrast, recordNativeOutlineContrast, recordNativeTextContrast } from './qualification-contrast.mjs';
import { captureNativeScreenshot } from './qualification-screenshot.mjs';

export function createNativeCaptureInventory() {
    return { captures: [], contrast: [], manualReview: ['Artwork/video/gradient text backgrounds, icons, focus-ring contrast, assistive-technology speech and physical native interactions require direct review.'] };
}

export async function captureNativeArtifact(page, manifest, inventory, name, fullPage = true) {
    const path = join(manifest.directory, `${name}.png`);
    const viewport = await page.evaluate(() => ({ width: innerWidth, height: innerHeight, dpr: devicePixelRatio }));
    const capture = await captureNativeScreenshot(page, path, fullPage, viewport);
    inventory.captures.push({ name, path, route: new URL(page.url()).pathname + new URL(page.url()).search, fullPage, viewport, ...capture });
    const samples = [
        ['page-heading', page.getByRole('main').getByRole('heading', { level: 1 }).first()],
        ['home-description', page.locator('.page-intro > p').first()],
        ['gallery-title', page.locator('.card-title').first()],
        ['profile-choice', page.locator('#profile-disclosure .option-name').first()],
        ['required-profile-choice', page.locator('.profile-gate-option > span:not(.avatar)').first()],
        ['preference-field', page.getByLabel('Preferred audio language', { exact: true })],
        ['popover-choice', page.locator('.player-popover:not([hidden]) .choice-label').first()],
        ['next-action', page.locator('.autoplay-card .play-next').first()],
        ['save-action', page.getByRole('button', { name: 'Save changes', exact: true })],
        ['visible-error', page.getByRole('alert').first()],
    ];
    for (const [label, target] of samples) {
        if (await target.count() !== 1 || !await target.isVisible()) continue;
        await recordNativeTextContrast(target, `${name}:${label}`, inventory.contrast);
    }
    const focused = page.locator(':focus');
    if (await focused.count() === 1 && await focused.isVisible()) await recordNativeOutlineContrast(focused, `${name}:keyboard-outline`, inventory.contrast);
    const close = page.getByRole('button', { name: 'Close player', exact: true });
    if (await close.count() === 1 && await close.isVisible()) await recordNativeIconContrast(close, `${name}:close-icon`, inventory.contrast);
    const search = page.locator('.search-submit').first();
    if (await search.count() === 1 && await search.isVisible()) await recordNativeIconContrast(search, `${name}:search-icon`, inventory.contrast);
    return path;
}

import { writeFile } from 'node:fs/promises';

export function pngDimensions(buffer) {
    if (buffer.length < 24 || buffer.toString('hex', 0, 8) !== '89504e470d0a1a0a' || buffer.toString('ascii', 12, 16) !== 'IHDR') throw new Error('Native capture did not return a PNG image.');
    return { width: buffer.readUInt32BE(16), height: buffer.readUInt32BE(20) };
}

export async function captureNativeScreenshot(page, path, fullPage, viewport) {
    if (fullPage) {
        const buffer = await page.screenshot({ path, fullPage: true });
        return { method: 'playwright_full_page', pixels: pngDimensions(buffer) };
    }
    const session = await page.context().newCDPSession(page);
    try {
        const { data } = await session.send('Page.captureScreenshot', { format: 'png', fromSurface: true, captureBeyondViewport: false });
        const buffer = Buffer.from(data, 'base64');
        await writeFile(path, buffer);
        const pixels = pngDimensions(buffer);
        const expected = { width: Math.round(viewport.width * viewport.dpr), height: Math.round(viewport.height * viewport.dpr) };
        if (Math.abs(pixels.width - expected.width) > 1 || Math.abs(pixels.height - expected.height) > 1) throw new Error(`Native viewport capture has ${pixels.width}×${pixels.height} pixels; actual ${viewport.width}×${viewport.height} CSS viewport at DPR ${viewport.dpr} requires ${expected.width}×${expected.height}.`);
        return { method: 'cdp_unclipped_viewport_surface', pixels };
    } finally {
        await session.detach();
    }
}

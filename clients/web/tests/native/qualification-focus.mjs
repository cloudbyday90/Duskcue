import { expect } from '@playwright/test';
import { recordNativeIconContrast, recordNativeOutlineContrast } from './qualification-contrast.mjs';

export async function focusDetails(locator) {
    return locator.evaluate((element) => {
        const rect = element.getBoundingClientRect();
        const hit = document.elementFromPoint(rect.x + rect.width / 2, rect.y + rect.height / 2);
        const dx = Math.min(4, rect.width / 4);
        const dy = Math.min(4, rect.height / 4);
        const exposedPoints = [[rect.x + dx, rect.y + dy], [rect.right - dx, rect.y + dy], [rect.x + dx, rect.bottom - dy], [rect.right - dx, rect.bottom - dy]].map(([x, y]) => {
            const at = document.elementFromPoint(x, y);
            return !!at && (at === element || element.contains(at));
        });
        let visibleFocus = false;
        for (let indicator = element; indicator; indicator = indicator.parentElement) {
            const style = getComputedStyle(indicator);
            if (style.outlineStyle === 'none' || parseFloat(style.outlineWidth) < 2 || style.outlineColor === 'transparent' || style.outlineColor === 'rgba(0, 0, 0, 0)') continue;
            let opacity = 1;
            let exposed = true;
            for (let ancestor = indicator; ancestor; ancestor = ancestor.parentElement) {
                const ancestorStyle = getComputedStyle(ancestor);
                opacity *= Number(ancestorStyle.opacity);
                if (ancestorStyle.visibility !== 'visible' || ancestorStyle.display === 'none') exposed = false;
            }
            if (exposed && opacity >= 0.95) { visibleFocus = true; break; }
        }
        return {
            focused: document.activeElement === element,
            withinViewport: rect.width > 0 && rect.height > 0 && rect.left >= -1 && rect.top >= -1 && rect.right <= innerWidth + 1 && rect.bottom <= innerHeight + 1,
            centerExposed: !!hit && (hit === element || element.contains(hit)),
            fullyExposed: exposedPoints.every(Boolean),
            exposedPoints,
            visibleFocus,
            rect: { x: rect.x, y: rect.y, width: rect.width, height: rect.height },
        };
    });
}

export async function keyboardReach(page, target, label, evidence) {
    await expect(target).toHaveCount(1);
    let reached = false;
    for (let tabs = 0; tabs <= 64; tabs += 1) {
        if (await target.evaluate((element) => document.activeElement === element)) { reached = true; break; }
        await page.keyboard.press('Tab');
    }
    if (!reached) throw new Error(`At 400% zoom, Tab did not reach ${label}.`);
    await expect.poll(async () => {
        const details = await focusDetails(target);
        evidence.focus[label] = details;
        return { focused: details.focused, withinViewport: details.withinViewport, centerExposed: details.centerExposed, fullyExposed: details.fullyExposed, visibleFocus: details.visibleFocus };
    }, { message: `${label} must remain visible and exposed after keyboard focus scrolls to it`, timeout: 3_000 }).toEqual({ focused: true, withinViewport: true, centerExposed: true, fullyExposed: true, visibleFocus: true });
    if (evidence.contrast) {
        await recordNativeOutlineContrast(target, `${label}:keyboard-outline`, evidence.contrast);
        if (await target.locator('svg').count() > 0) await recordNativeIconContrast(target, `${label}:icon`, evidence.contrast);
    }
    return evidence.focus[label];
}

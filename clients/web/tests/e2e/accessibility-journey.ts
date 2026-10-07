import { expect, type Locator, type Page } from '@playwright/test';

export async function keyboardReach(page: Page, control: Locator) {
    await expect(control).toHaveCount(1);
    for (let index = 0; index < 64; index += 1) {
        if (await control.evaluate((element) => document.activeElement === element)) break;
        await page.keyboard.press('Tab');
    }
    await expect(control).toBeFocused();
    await expect.poll(() => control.evaluate((element) => {
        const rect = element.getBoundingClientRect();
        const hit = document.elementFromPoint(rect.x + rect.width / 2, rect.y + rect.height / 2);
        const style = getComputedStyle(element);
        return {
            withinViewport: rect.left >= -1 && rect.top >= -1 && rect.right <= innerWidth + 1 && rect.bottom <= innerHeight + 1,
            exposed: !!hit && (hit === element || element.contains(hit)),
            visibleFocus: style.outlineStyle !== 'none' && parseFloat(style.outlineWidth) >= 2 && style.outlineColor !== 'transparent' && style.outlineColor !== 'rgba(0, 0, 0, 0)',
        };
    })).toEqual({ withinViewport: true, exposed: true, visibleFocus: true });
}

export async function noHorizontalOverflow(page: Page) {
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= document.documentElement.clientWidth + 1)).toBe(true);
}

export async function textContrast(control: Locator) {
    return control.evaluate((element) => {
        const parse = (value: string) => {
            if (!/^rgba?\(/.test(value)) throw new Error(`Unsupported computed color: ${value}`);
            const parts = value.match(/[\d.]+/g)!.map(Number);
            return [parts[0], parts[1], parts[2], parts[3] ?? 1];
        };
        const layers: number[][] = [];
        for (let owner: Element | null = element; owner; owner = owner.parentElement) {
            const style = getComputedStyle(owner);
            if (style.backgroundImage !== 'none' || Number(style.opacity) !== 1) throw new Error('Contrast sample requires a solid surface without group opacity.');
            const background = parse(style.backgroundColor);
            layers.push(background);
            if (background[3] === 1) break;
        }
        if (layers.at(-1)?.[3] !== 1) throw new Error('Contrast sample has no opaque background.');
        const blend = (foreground: number[], background: number[]) => foreground.slice(0, 3).map((value, index) => value * foreground[3] + background[index] * (1 - foreground[3]));
        const background = layers.reverse().reduce((surface, layer) => blend(layer, surface), [0, 0, 0]);
        const foreground = blend(parse(getComputedStyle(element).color), background);
        const luminance = (rgb: number[]) => rgb.map((value) => {
            const component = value / 255;
            return component <= 0.04045 ? component / 12.92 : ((component + 0.055) / 1.055) ** 2.4;
        }).reduce((value, component, index) => value + component * [0.2126, 0.7152, 0.0722][index], 0);
        const first = luminance(foreground); const second = luminance(background);
        return { foreground, background, ratio: (Math.max(first, second) + 0.05) / (Math.min(first, second) + 0.05) };
    });
}

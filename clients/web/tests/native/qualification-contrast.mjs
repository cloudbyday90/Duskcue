import { expect } from '@playwright/test';

export function rgbContrast(first, second) {
    const luminance = (color) => color.slice(0, 3).map((channel) => {
        const value = channel / 255;
        return value <= 0.04045 ? value / 12.92 : ((value + 0.055) / 1.055) ** 2.4;
    }).reduce((sum, value, index) => sum + value * [0.2126, 0.7152, 0.0722][index], 0);
    const values = [luminance(first), luminance(second)].sort((a, b) => a - b);
    return (values[1] + 0.05) / (values[0] + 0.05);
}

async function measureNativePaint(target, kind) {
    return target.evaluate((original, kind) => {
        const rgba = (value) => {
            const match = /^rgba?\((.*)\)$/.exec(value);
            if (!match) return null;
            const parts = match[1].split(/[\s,/]+/).filter(Boolean);
            const color = parts.slice(0, 3).map((part) => part.endsWith('%') ? parseFloat(part) * 2.55 : Number(part));
            color.push(parts[3] === undefined ? 1 : parts[3].endsWith('%') ? parseFloat(parts[3]) / 100 : Number(parts[3]));
            return color.every(Number.isFinite) ? color : null;
        };
        const over = (top, bottom) => {
            const alpha = top[3] + bottom[3] * (1 - top[3]);
            return [...top.slice(0, 3).map((value, index) => alpha ? (value * top[3] + bottom[index] * bottom[3] * (1 - top[3])) / alpha : 0), alpha];
        };
        if (original.matches(':disabled') || original.getAttribute('aria-disabled') === 'true') return { supported: false, reason: 'inactive_component_exempt' };
        let element = original;
        let foreground;
        if (kind === 'outline') {
            for (let candidate = original; candidate; candidate = candidate.parentElement) {
                const style = getComputedStyle(candidate);
                if (style.outlineStyle === 'none' || parseFloat(style.outlineWidth) < 2) continue;
                foreground = rgba(style.outlineColor);
                element = parseFloat(style.outlineOffset) >= 0 ? candidate.parentElement || candidate : candidate;
                break;
            }
            if (!foreground || foreground[3] <= 0) return { supported: false, reason: 'no_visible_outline_color' };
        } else if (kind === 'icon') {
            const icon = original.matches('svg') ? original : original.querySelector('svg');
            if (!icon) return { supported: false, reason: 'no_icon' };
            const style = getComputedStyle(icon);
            const fill = rgba(style.fill);
            foreground = fill?.[3] > 0 ? fill : rgba(style.stroke);
        } else foreground = rgba(getComputedStyle(original).color);
        if (!foreground) return { supported: false, reason: 'unsupported_color_space' };
        const box = original.getBoundingClientRect();
        const overlaps = (other) => other.left < box.right && other.right > box.left && other.top < box.bottom && other.bottom > box.top;
        let background = [0, 0, 0, 0];
        let branch = element;
        for (let owner = element; owner; branch = owner, owner = owner.parentElement) {
            const style = getComputedStyle(owner);
            if (Number(style.opacity) < 0.99 || style.visibility !== 'visible' || style.display === 'none') return { supported: false, reason: 'faded_or_hidden' };
            if (style.backgroundImage !== 'none' || (style.backdropFilter && style.backdropFilter !== 'none')) return { supported: false, reason: 'gradient_image_or_backdrop' };
            for (const pseudo of ['::before', '::after']) {
                const paint = getComputedStyle(owner, pseudo);
                if (paint.content !== 'none' && paint.content !== 'normal' && paint.backgroundImage !== 'none') return { supported: false, reason: 'painted_pseudo_layer' };
            }
            for (const sibling of owner.children) {
                if (sibling === branch || sibling.contains(element)) continue;
                const siblingStyle = getComputedStyle(sibling);
                if (['absolute', 'fixed'].includes(siblingStyle.position) && siblingStyle.display !== 'none' && Number(siblingStyle.opacity) > 0 && overlaps(sibling.getBoundingClientRect())) return { supported: false, reason: 'overlapping_painted_sibling' };
                const media = sibling.matches('img, video, canvas') ? [sibling] : [...sibling.querySelectorAll('img, video, canvas')];
                if (media.some((item) => overlaps(item.getBoundingClientRect()))) return { supported: false, reason: 'overlapping_media' };
            }
            const layer = rgba(style.backgroundColor);
            if (!layer) return { supported: false, reason: 'unsupported_background_color' };
            background = over(background, layer);
            if (background[3] >= 0.999) return { supported: true, foreground: over(foreground, background), background, text: original.textContent?.trim().slice(0, 120) || '' };
        }
        return { supported: false, reason: 'no_opaque_background' };
    }, kind);
}

async function recordNativeContrast(target, label, records, kind, minimum) {
    const measured = await measureNativePaint(target, kind);
    const result = { label, kind, ...measured };
    if (measured.supported) {
        result.ratio = rgbContrast(measured.foreground, measured.background);
        result.minimum = minimum;
    }
    records.push(result);
    if (measured.supported) expect(result.ratio, `${label} solid-surface ${kind} contrast`).toBeGreaterThanOrEqual(minimum);
    return result;
}

export const recordNativeTextContrast = (target, label, records) => recordNativeContrast(target, label, records, 'text', 4.5);
export const recordNativeOutlineContrast = (target, label, records) => recordNativeContrast(target, label, records, 'outline', 3);
export const recordNativeIconContrast = (target, label, records) => recordNativeContrast(target, label, records, 'icon', 3);

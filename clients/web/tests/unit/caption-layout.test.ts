import { describe, expect, it, vi } from 'vitest';
import { createCaptionLayout, fitCaptionPicture } from '../../src/lib/playback/caption-layout.js';

const stage = { left: 0, top: 0, right: 1280, bottom: 720 };

describe('whole-picture protection for actual burned captions', () => {
    it('fits the original picture without obstructions', () => {
        expect(fitCaptionPicture({ stage, viewport: stage, aspectRatio: 16 / 9 })).toEqual({ left: 0, top: 0, width: 1280, height: 720 });
    });

    it('reserves the visible transport and heading instead of drawing captions below them', () => {
        const picture = fitCaptionPicture({ stage, viewport: stage, aspectRatio: 16 / 9, obstructions: [{ left: 0, top: 608, right: 1280, bottom: 720 }, { left: 16, top: 16, right: 1264, bottom: 60 }] });
        expect(picture.top).toBeGreaterThanOrEqual(72);
        expect(picture.top + picture.height).toBeLessThanOrEqual(596);
        expect(picture.width / picture.height).toBeCloseTo(16 / 9);
    });

    it('optimizes the actual fitted picture rather than selecting the largest rectangle by area', () => {
        const bounds = { left: 0, top: 0, right: 1000, bottom: 800 };
        const picture = fitCaptionPicture({ stage: bounds, viewport: bounds, aspectRatio: 16 / 9, gap: 0, obstructions: [{ left: 500, top: 0, right: 750, bottom: 500 }] });
        expect(picture.width).toBeCloseTo(300 * 16 / 9);
        expect(picture.height).toBeCloseTo(300);
        expect(picture.top).toBe(500);
    });

    it('keeps the complete picture away from a menu and episode-end card', () => {
        const obstacles = [{ left: 0, top: 608, right: 1280, bottom: 720 }, { left: 900, top: 180, right: 1264, bottom: 576 }];
        const picture = fitCaptionPicture({ stage, viewport: stage, aspectRatio: 16 / 9, obstructions: obstacles });
        for (const obstacle of obstacles) expect(picture.left + picture.width <= obstacle.left - 12 || picture.left >= obstacle.right + 12 || picture.top + picture.height <= obstacle.top - 12 || picture.top >= obstacle.bottom + 12).toBe(true);
    });

    it('clips the calculation to a visible scrolled stage below a sticky heading', () => {
        const picture = fitCaptionPicture({ stage: { left: 0, top: -40, right: 640, bottom: 191 }, viewport: { left: 0, top: 0, right: 640, bottom: 420 }, aspectRatio: 16 / 9, obstructions: [{ left: 0, top: 0, right: 640, bottom: 60 }, { left: 280, top: 68, right: 632, bottom: 412 }] });
        expect(picture.left + picture.width).toBeLessThanOrEqual(268);
        expect(picture.top).toBeGreaterThanOrEqual(72);
        expect(picture.top + picture.height).toBeLessThanOrEqual(191);
    });

    it('does not invent a visible area when the stage is offscreen or completely blocked', () => {
        expect(fitCaptionPicture({ stage: { left: 0, top: -200, right: 320, bottom: -20 }, viewport: stage, aspectRatio: 16 / 9 })).toBeNull();
        expect(fitCaptionPicture({ stage, viewport: stage, aspectRatio: 16 / 9, obstructions: [stage] })).toBeNull();
    });

    it('coalesces layout work, observes no changing video dimensions, restores the full stage and rejects a queued frame after disposal', () => {
        vi.stubGlobal('window', Object.assign(new EventTarget(), { innerHeight: 720 }));
        vi.stubGlobal('document', Object.assign(new EventTarget(), { documentElement: { clientWidth: 1280 } }));
        const callbacks: FrameRequestCallback[] = [];
        const requestFrame = vi.fn((callback: FrameRequestCallback) => { callbacks.push(callback); return callbacks.length; });
        const cancelFrame = vi.fn();
        const observer = { observe: vi.fn(), unobserve: vi.fn(), disconnect: vi.fn() };
        const mutations = { observe: vi.fn(), disconnect: vi.fn(), takeRecords: vi.fn(() => []) };
        const element = (box: typeof stage) => Object.assign(new EventTarget(), { isConnected: true, hidden: false, getBoundingClientRect: () => ({ ...box, width: box.right - box.left, height: box.bottom - box.top }) });
        const container = element(stage);
        const videoStage = element(stage);
        const controls = element({ left: 0, top: 608, right: 1280, bottom: 720 });
        const style: Record<string, any> = { removeProperty: vi.fn((name: string) => { delete style[name]; }) };
        const video = Object.assign(new EventTarget(), { videoWidth: 320, videoHeight: 180, style });
        const layout = createCaptionLayout({ container, stage: videoStage, video, readObstructions: () => [controls], requestFrame, cancelFrame, createObserver: () => observer, createMutationObserver: () => mutations, isVisible: () => true });
        layout.update(true);
        layout.refresh();
        layout.refresh();
        expect(requestFrame).toHaveBeenCalledOnce();
        callbacks[0](0);
        expect(parseFloat(style.width)).toBeLessThan(1280);
        expect(observer.observe).not.toHaveBeenCalledWith(video);
        layout.update(false);
        callbacks[1](0);
        expect(style.width).toBeUndefined();
        layout.update(true);
        const queued = callbacks[2];
        layout.dispose();
        expect(cancelFrame).toHaveBeenCalledOnce();
        expect(observer.disconnect).toHaveBeenCalledOnce();
        expect(mutations.disconnect).toHaveBeenCalledOnce();
        queued(0);
        expect(style.width).toBeUndefined();
    });

    it('responds to final popup placement even when only its position changes and removes that listener on disposal', () => {
        vi.stubGlobal('window', Object.assign(new EventTarget(), { innerHeight: 720 }));
        vi.stubGlobal('document', Object.assign(new EventTarget(), { documentElement: { clientWidth: 1280 } }));
        const callbacks: FrameRequestCallback[] = [];
        const requestFrame = vi.fn((callback: FrameRequestCallback) => { callbacks.push(callback); return callbacks.length; });
        const observer = { observe: vi.fn(), unobserve: vi.fn(), disconnect: vi.fn() };
        const mutations = { observe: vi.fn(), disconnect: vi.fn(), takeRecords: vi.fn(() => []) };
        const container = Object.assign(new EventTarget(), { isConnected: true });
        const videoStage = { isConnected: true, getBoundingClientRect: () => stage };
        let popupBounds = { left: 900, top: 100, right: 1264, bottom: 400, width: 364, height: 300 };
        const popup = { isConnected: true, hidden: false, getBoundingClientRect: () => popupBounds };
        const style: Record<string, any> = { removeProperty: vi.fn((name: string) => { delete style[name]; }) };
        const video = Object.assign(new EventTarget(), { videoWidth: 320, videoHeight: 180, style });
        const layout = createCaptionLayout({ container, stage: videoStage, video, readObstructions: () => [popup], requestFrame, cancelFrame: vi.fn(), createObserver: () => observer, createMutationObserver: () => mutations, isVisible: () => true });
        layout.update(true);
        callbacks[0](0);
        const previousLeft = parseFloat(style.left);
        popupBounds = { ...popupBounds, left: 100, right: 464 };
        container.dispatchEvent(new Event('duskcue:player-popover-layout'));
        callbacks[1](0);
        expect(parseFloat(style.left)).toBeGreaterThan(previousLeft);
        expect(parseFloat(style.left)).toBeGreaterThanOrEqual(popupBounds.right + 12);
        layout.dispose();
        container.dispatchEvent(new Event('duskcue:player-popover-layout'));
        expect(requestFrame).toHaveBeenCalledTimes(2);
    });

    it('retains picture protection throughout a control fade and restores it only after the overlay is actually transparent', () => {
        vi.stubGlobal('window', Object.assign(new EventTarget(), { innerHeight: 720 }));
        vi.stubGlobal('document', Object.assign(new EventTarget(), { documentElement: { clientWidth: 1280 } }));
        const callbacks: FrameRequestCallback[] = [];
        const requestFrame = vi.fn((callback: FrameRequestCallback) => { callbacks.push(callback); return callbacks.length; });
        const observer = { observe: vi.fn(), unobserve: vi.fn(), disconnect: vi.fn() };
        const mutations = { observe: vi.fn(), disconnect: vi.fn(), takeRecords: vi.fn(() => []) };
        const container = Object.assign(new EventTarget(), { isConnected: true });
        const videoStage = { isConnected: true, getBoundingClientRect: () => stage };
        const controls = { isConnected: true, hidden: false, getBoundingClientRect: () => ({ left: 0, top: 608, right: 1280, bottom: 720, width: 1280, height: 112 }) };
        const style: Record<string, any> = { removeProperty: vi.fn((name: string) => { delete style[name]; }) };
        const video = Object.assign(new EventTarget(), { videoWidth: 320, videoHeight: 180, style });
        let opacity = 1;
        const layout = createCaptionLayout({ container, stage: videoStage, video, readObstructions: () => [controls], requestFrame, cancelFrame: vi.fn(), createObserver: () => observer, createMutationObserver: () => mutations, isVisible: () => opacity > 0 });
        layout.update(true);
        callbacks[0](0);
        const protectedWidth = style.width;
        opacity = 0.4;
        layout.update(true);
        callbacks[1](0);
        expect(style.width).toBe(protectedWidth);
        opacity = 0;
        container.dispatchEvent(new Event('transitionend'));
        callbacks[2](0);
        expect(style.width).toBeUndefined();
        layout.dispose();
        container.dispatchEvent(new Event('transitionend'));
        expect(requestFrame).toHaveBeenCalledTimes(3);
    });
});

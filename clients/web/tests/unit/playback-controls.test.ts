import { afterEach, describe, expect, it, vi } from 'vitest';
import { createControlsVisibility } from '../../src/lib/playback/visibility.js';
import { createFullscreenController } from '../../src/lib/playback/fullscreen.js';
import { createPlaybackSessionGuard } from '../../src/lib/playback/session.js';

vi.mock('../../src/lib/desktop/tauri.js', () => ({ isTauriDesktop: () => false }));

afterEach(() => { vi.useRealTimers(); });

describe('focus-aware controls', () => {
    it('hides after idle playback and reveals on new activity', () => {
        vi.useFakeTimers();
        const changed = vi.fn((_visible: boolean) => {});
        const visibility = createControlsVisibility({ onChange: changed });
        visibility.setState({ playing: true });
        vi.advanceTimersByTime(2999);
        expect(visibility.getVisible()).toBe(true);
        vi.advanceTimersByTime(1);
        expect(visibility.getVisible()).toBe(false);
        visibility.activity();
        expect(visibility.getVisible()).toBe(true);
        visibility.dispose();
        expect(vi.getTimerCount()).toBe(0);
    });

    it.each(['focusWithin', 'hovered', 'disclosureOpen', 'seeking'])('pins controls while %s and restarts a full delay afterward', (reason) => {
        vi.useFakeTimers();
        const visibility = createControlsVisibility({ onChange: (_visible: boolean) => {} });
        visibility.setState({ playing: true, [reason]: true });
        vi.advanceTimersByTime(10000);
        expect(visibility.getVisible()).toBe(true);
        visibility.setState({ [reason]: false });
        vi.advanceTimersByTime(2999);
        expect(visibility.getVisible()).toBe(true);
        vi.advanceTimersByTime(1);
        expect(visibility.getVisible()).toBe(false);
        visibility.setState({ playing: false });
        vi.advanceTimersByTime(10000);
        expect(visibility.getVisible()).toBe(true);
        visibility.dispose();
    });
});

function browserFullscreen() {
    const document = Object.assign(new EventTarget(), { fullscreenElement: null as unknown, exitFullscreen: vi.fn(async () => {}) });
    const element = { requestFullscreen: vi.fn(async () => { document.fullscreenElement = element; document.dispatchEvent(new Event('fullscreenchange')); }) };
    document.exitFullscreen.mockImplementation(async () => { document.fullscreenElement = null; document.dispatchEvent(new Event('fullscreenchange')); });
    return { document, element };
}

describe('actual fullscreen state', () => {
    it('tracks browser entry, exit, and a browser-initiated exit', async () => {
        const { document, element } = browserFullscreen();
        const fullscreen = createFullscreenController({ document, element, loadNative: async () => null, onChange: (_state: unknown) => {} });
        await fullscreen.ready;
        await fullscreen.toggle();
        expect(fullscreen.getState().fullscreen).toBe(true);
        await document.exitFullscreen();
        await Promise.resolve();
        expect(fullscreen.getState().fullscreen).toBe(false);
        await fullscreen.toggle();
        await fullscreen.exit();
        expect(fullscreen.getState().fullscreen).toBe(false);
        fullscreen.dispose();
    });

    it('reports a rejected request without claiming fullscreen', async () => {
        const { document, element } = browserFullscreen();
        element.requestFullscreen.mockRejectedValueOnce(new Error('Denied'));
        const fullscreen = createFullscreenController({ document, element, loadNative: async () => null, onChange: (_state: unknown) => {} });
        await fullscreen.ready;
        await expect(fullscreen.toggle()).rejects.toThrow('Denied');
        expect(fullscreen.getState()).toMatchObject({ fullscreen: false, pending: false });
        expect(fullscreen.getState().error).toBeTruthy();
        await fullscreen.toggle();
        expect(fullscreen.getState().fullscreen).toBe(true);
        fullscreen.dispose();
    });

    it('uses the native desktop fallback and releases it on disposal', async () => {
        const { document, element } = browserFullscreen();
        element.requestFullscreen.mockRejectedValueOnce(new Error('Webview unavailable'));
        let active = false;
        const stopListening = vi.fn();
        const native = {
            isFullscreen: vi.fn(async () => active),
            setFullscreen: vi.fn(async (value: boolean) => { active = value; }),
            onResized: vi.fn(async (_callback: () => void) => stopListening),
        };
        const fullscreen = createFullscreenController({ document, element, loadNative: async () => native, onChange: (_state: unknown) => {} });
        await fullscreen.ready;
        await fullscreen.toggle();
        expect(fullscreen.getState().fullscreen).toBe(true);
        fullscreen.dispose();
        await vi.waitFor(() => expect(active).toBe(false));
        expect(stopListening).toHaveBeenCalledOnce();
    });
});

describe('playback session ownership', () => {
    it('deduplicates stops with the final current position', async () => {
        const stop = vi.fn(async (_request: unknown) => {});
        const guard = createPlaybackSessionGuard(stop);
        const first = guard.stop('session-1', 12345.8);
        expect(guard.stop('session-1', 0)).toBe(first);
        await first;
        expect(stop).toHaveBeenCalledExactlyOnceWith({ session_id: 'session-1', position_ms: 12345 });
    });

    it('invalidates pending operations without making an older stop current', () => {
        const guard = createPlaybackSessionGuard(async (_request: unknown) => {});
        const pending = guard.begin();
        guard.invalidate();
        expect(guard.isCurrent(pending)).toBe(false);
        const next = guard.begin();
        expect(guard.isCurrent(next)).toBe(true);
        expect(guard.isCurrent(pending)).toBe(false);
    });
});

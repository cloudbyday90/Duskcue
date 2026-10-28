// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import { commandSession } from './transport.js';
import { getServerOrigin } from '../../api/core.js';
import { playbackStartRequest } from '../start-request.js';
import { createCommandController } from './controller.js';
import { PROTOCOL, scope, sameScope } from './schema.js';

export function createCommandAdapter({ update, read, revision, createController = createCommandController } ) {
    let restored = false;
    let selected = false;
    let playing = false;
    let timer = null;
    let disposeSession = null;
    let gate = () => true;
    let session = null;
    const controller = createController({ onChange: (state) => {
        if (!gate() && state.streamUrl) return;
        const ready = state.receipt?.ready;
        const previous = read();
        const newOutput = previous.streamUrl !== state.streamUrl;
        if (previous.streamUrl !== state.streamUrl) revision();
        update((value) => ({ ...value, commandProtocol: selected ? PROTOCOL : value.commandProtocol,
            commandPhase: state.phase, progressUncertain: state.progressUncertain, streamUrl: state.streamUrl,
            commandRetryAfter: state.retryAfter || 0,
            loading: ['preparing', 'changing', 'recovering'].includes(state.phase), seeking: ['changing', 'recovering'].includes(state.phase),
            error: state.error || (['uncertain', 'expired', 'status_required', 'unavailable', 'failed', 'withdrawn'].includes(state.phase) ? new Error('VOD_STATUS_REQUIRED') : null),
            sessionReleased: state.phase === 'released',
            ...(['released', 'idle'].includes(state.phase) ? { sessionId: null, transcodeSessionId: null, directSourceRevision: null } : {}),
            ...(['withdrawing', 'released', 'uncertain', 'expired', 'status_required'].includes(state.phase) ? { isPlaying: false, isBuffering: false } : {}),
            release: { phase: state.record?.phase === 'cleanup' && state.error && state.phase === 'uncertain' ? 'failed' : 'idle', error: state.error },
            ...(ready && state.streamUrl ? { sessionId: ready.session_id, mediaFileId: ready.media_file_id, streamDecision: ready.delivery_mode,
                ...(newOutput ? { isPlaying: playing, isBuffering: false } : {}),
                streamOffsetMs: ready.stream_offset_ms ?? 0, ...(newOutput ? { positionMs: ready.position_ms } : {}),
                directSourceRevision: ready.delivery_mode === 'direct_play' ? ready.output_revision : null,
                transcodeSessionId: null, recovery: ready.recovery } : {}),
        }));
    } });
    async function bind() {
        if (timer !== null || typeof window === 'undefined') return;
        session = await commandSession();
        timer = setInterval(() => { if (controller.active()) void controller.checkpoint().catch(() => {}); }, 15_000);
        window.addEventListener('pagehide', suspend);
        window.addEventListener('pageshow', restoredPage);
        document.addEventListener('visibilitychange', visible);
        disposeSession = session.subscribe(() => {
            const record = controller.getState().record;
            if (!record) return;
            const capture = session.capture();
            const actual = capture.context && scope(getServerOrigin() || window.location.origin, capture.context);
            if (!actual || capture.userId !== record.scope.user || !sameScope(actual, record.scope)) {
                playing = false;
                controller.invalidate(actual);
                if (!controller.active()) selected = false;
            }
        });
    }
    function suspend() { controller.suspend(true); }
    function restoredPage(event) { if (event.persisted) controller.suspend(true); }
    function visible() { if (!document.hidden && controller.active()) controller.resume(); }
    async function restore() {
        if (!restored) { restored = true; if (await controller.restore()) { selected = true; await bind(); update((value) => ({ ...value, commandProtocol: PROTOCOL })); } }
    }
    function result() {
        const state = controller.getState();
        const ready = state.receipt?.ready;
        if (!ready || state.phase !== 'ready' || !state.streamUrl) throw new Error('VOD_STATUS_REQUIRED');
        return { ...ready, stream_decision: ready.delivery_mode };
    }
    async function play(item, file, options, isCurrent) {
        await restore();
        if (controller.active()) {
            if (controller.getState().phase !== 'ready' || controller.getState().cleanupOnly) throw new Error('VOD_STATUS_REQUIRED');
            await controller.close();
        }
        selected = true;
        playing = options.playing !== false;
        await bind();
        if (!isCurrent() || !selected) throw new DOMException('Playback context changed', 'AbortError');
        gate = isCurrent;
        update((value) => ({ ...value, mediaItem: item, mediaFileId: file, durationMs: (item.runtime_seconds || 0) * 1000,
            isPlaying: options.playing !== false, commandProtocol: PROTOCOL }));
        await controller.start(playbackStartRequest(item.id, file, options));
        return result();
    }
    async function reconfigure(item, file, options) {
        const current = read();
        if (item.id !== current.mediaItem?.id || file !== current.mediaFileId) throw new Error('VOD_SOURCE_CHANGED');
        await controller.change({ kind: 'reconfigure', position_ms: Math.floor(current.positionMs), selection: playbackStartRequest(item.id, file, options) });
        return result();
    }
    function observe(position, sourceUrl, transition = false) {
        const state = read();
        const record = controller.getState().record;
        const changed = controller.observe({ output: record?.output, position: Math.floor(position), attached: !!sourceUrl && sourceUrl === state.streamUrl,
            state: state.isBuffering ? 'buffering' : state.isPlaying ? 'playing' : 'paused' });
        if (transition && changed) void controller.flushProgress().catch(() => {});
    }
    return { restore, play, reconfigure, observe, active: () => controller.active(), selected: () => selected,
        seek: async (position) => { await controller.change({ kind: 'seek', position_ms: Math.floor(position) }); return result(); },
        failure: (failure) => controller.recover({ kind: 'decoder', mode: failure.sourceMode, code: failure.mediaErrorCode }),
        stop: async () => { playing = false; const result = await controller.close(); if (result) selected = false; return result; },
        playing: () => playing,
        setPlaying: (value, observed = false) => { if (!observed) playing = value === true; return playing && value === true; },
        check: async () => { await controller.check(); return controller.getState().phase === 'ready' ? controller.describe() : null; }, checkpoint: () => controller.checkpoint(),
        invalidate: () => { playing = false; const capture = session?.capture(); controller.invalidate(capture?.context ? scope(capture.origin, capture.context) : null); if (!controller.active()) selected = false; },
        dispose: () => {
            playing = false;
            if (timer !== null) clearInterval(timer); timer = null;
            disposeSession?.(); disposeSession = null;
            if (typeof window !== 'undefined') {
                window.removeEventListener('pagehide', suspend); window.removeEventListener('pageshow', restoredPage);
                document.removeEventListener('visibilitychange', visible);
            }
            controller.dispose(); restored = false;
        } };
}

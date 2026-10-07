/*
 * Duskcue — Self-hosted media streaming server
 * Copyright (C) 2026-2026 Duskcue Contributors
 * Licensed under AGPL-3.0. See LICENSE for details.
 */

import { resolveNextEpisode } from './next-episode.js';

export const AUTOPLAY_DELAY_MS = 10_000;

const defaultClock = {
    now: () => performance.now(),
    setTimeout: (callback, delay) => setTimeout(callback, delay),
    clearTimeout: (timer) => clearTimeout(timer),
};

function initialState(paused = false) {
    return { phase: 'idle', nextEpisode: null, reason: null, remainingMs: 0, seconds: 0, paused, cancelled: false, error: null, announcement: null };
}

export function createAutoplayController({ resolveNext = resolveNextEpisode, onChange = (_state) => {}, onPlayNext, clock = defaultClock }) {
    if (typeof onPlayNext !== 'function') throw new TypeError('Autoplay requires a playback callback');
    let state = initialState();
    let context = null;
    let generation = 0;
    let controller = null;
    let timer = null;
    let deadline = null;
    let transitioned = false;
    let disposed = false;
    const pauses = { focusWithin: false, disclosureOpen: false, documentHidden: false };

    function getState() {
        return { ...state, nextEpisode: state.nextEpisode ? { ...state.nextEpisode } : null };
    }

    function emit() {
        if (!disposed) onChange(getState());
    }

    function isPaused() {
        return Object.values(pauses).some(Boolean);
    }

    function stopTimer() {
        if (timer !== null) clock.clearTimeout(timer);
        timer = null;
        if (deadline !== null) state.remainingMs = Math.max(0, deadline - clock.now());
        deadline = null;
        state.seconds = Math.ceil(state.remainingMs / 1000);
    }

    function canTime() {
        return context?.preferencesReady === true && context.autoplayEnabled === true && !state.cancelled;
    }

    async function transition(mode) {
        if (disposed || transitioned || !state.nextEpisode || !['countdown', 'ready'].includes(state.phase)) return false;
        if (mode === 'automatic' && (!canTime() || isPaused())) return false;
        const currentGeneration = generation;
        const signal = controller.signal;
        const nextEpisode = state.nextEpisode;
        const { profileId, item } = context;
        transitioned = true;
        stopTimer();
        state.phase = 'transitioning';
        state.remainingMs = 0;
        state.seconds = 0;
        state.announcement = 'next_starting';
        emit();
        if (disposed || signal.aborted || generation !== currentGeneration) return false;
        try {
            await onPlayNext(nextEpisode, { signal, mode, profileId, currentItemId: item.id });
            if (disposed || signal.aborted || generation !== currentGeneration) return false;
            state.phase = 'transitioned';
            state.announcement = 'next_started';
            emit();
            return true;
        } catch (error) {
            if (disposed || signal.aborted || generation !== currentGeneration) return false;
            state.phase = 'error';
            state.error = error;
            state.cancelled = true;
            state.announcement = 'next_failed';
            emit();
            return false;
        }
    }

    function tick(currentGeneration) {
        if (disposed || generation !== currentGeneration) return;
        timer = null;
        if (disposed || state.phase !== 'countdown' || isPaused() || !canTime() || deadline === null) return;
        state.remainingMs = Math.max(0, deadline - clock.now());
        state.seconds = Math.ceil(state.remainingMs / 1000);
        if (state.remainingMs === 0) {
            void transition('automatic');
            return;
        }
        emit();
        if (!disposed && generation === currentGeneration && state.phase === 'countdown' && !isPaused() && canTime() && deadline !== null) {
            timer = clock.setTimeout(() => tick(currentGeneration), Math.min(1000, state.remainingMs));
        }
    }

    function startTimer() {
        if (disposed || state.phase !== 'countdown' || isPaused() || !canTime()) return;
        const currentGeneration = generation;
        deadline = clock.now() + state.remainingMs;
        timer = clock.setTimeout(() => tick(currentGeneration), Math.min(1000, state.remainingMs));
    }

    function settleAvailable() {
        stopTimer();
        if (canTime()) {
            state.phase = 'countdown';
            state.remainingMs = AUTOPLAY_DELAY_MS;
            state.seconds = AUTOPLAY_DELAY_MS / 1000;
            state.announcement = isPaused() ? 'autoplay_paused' : 'autoplay_countdown';
            startTimer();
        } else {
            state.phase = 'ready';
            state.remainingMs = 0;
            state.seconds = 0;
            state.announcement = state.cancelled ? 'autoplay_cancelled' : 'next_ready';
        }
        emit();
    }

    async function discover() {
        const currentGeneration = generation;
        const signal = controller.signal;
        const currentItem = context.item;
        state.phase = 'loading';
        state.nextEpisode = null;
        state.error = null;
        state.reason = null;
        state.announcement = 'next_loading';
        emit();
        if (disposed || signal.aborted || generation !== currentGeneration) return getState();
        try {
            const next = await resolveNext(currentItem, { signal });
            if (disposed || signal.aborted || generation !== currentGeneration) return getState();
            if (!['available', 'unavailable', 'none', 'unordered'].includes(next?.kind) || (['available', 'unavailable'].includes(next.kind) && !next.episode?.id)) {
                throw new Error('Invalid next episode result');
            }
            state.nextEpisode = next.episode;
            state.reason = next.reason;
            if (next.kind === 'available') settleAvailable();
            else {
                state.phase = next.kind;
                state.announcement = `next_${next.kind}`;
                emit();
            }
        } catch (error) {
            if (disposed || signal.aborted || generation !== currentGeneration) return getState();
            state.phase = 'error';
            state.error = error;
            state.announcement = 'next_failed';
            emit();
        }
        return getState();
    }

    function reset() {
        generation += 1;
        controller?.abort();
        controller = null;
        stopTimer();
        transitioned = false;
        context = null;
        state = initialState(isPaused());
        emit();
    }

    async function ended({ item = undefined, profileId = undefined, autoplayEnabled = false, preferencesReady = false } = {}) {
        if (disposed) return getState();
        if (context?.item.id === item?.id && context?.profileId === profileId) return getState();
        reset();
        if (!item?.id || typeof profileId !== 'string' || !profileId) {
            state.phase = 'error';
            state.error = new Error('Autoplay requires current media and profile identity');
            state.announcement = 'next_failed';
            emit();
            return getState();
        }
        context = { item: { ...item }, profileId, autoplayEnabled: autoplayEnabled === true, preferencesReady: preferencesReady === true };
        controller = new AbortController();
        return discover();
    }

    function setPreference({ autoplayEnabled = false, preferencesReady = false } = {}) {
        if (disposed || !context) return;
        context.autoplayEnabled = autoplayEnabled === true;
        context.preferencesReady = preferencesReady === true;
        if (state.phase === 'countdown' && !canTime()) settleAvailable();
        else if (state.phase === 'ready' && canTime()) settleAvailable();
    }

    function setPaused(reasons = {}) {
        if (disposed) return;
        const wasPaused = isPaused();
        for (const reason of Object.keys(pauses)) {
            if (Object.hasOwn(reasons, reason)) pauses[reason] = reasons[reason] === true;
        }
        state.paused = isPaused();
        if (state.phase === 'countdown' && wasPaused !== state.paused) {
            if (state.paused) stopTimer();
            else startTimer();
            state.announcement = state.paused ? 'autoplay_paused' : 'autoplay_countdown';
        }
        emit();
    }

    function cancel() {
        if (disposed || !context || transitioned) return;
        stopTimer();
        state.cancelled = true;
        state.remainingMs = 0;
        state.seconds = 0;
        if (state.phase === 'countdown' || state.phase === 'ready') {
            state.phase = 'ready';
            state.announcement = 'autoplay_cancelled';
        }
        emit();
    }

    async function retry() {
        if (disposed || !context || !['unavailable', 'error'].includes(state.phase)) return getState();
        stopTimer();
        controller?.abort();
        controller = new AbortController();
        generation += 1;
        transitioned = false;
        state.cancelled = true;
        return discover();
    }

    function dispose() {
        if (disposed) return;
        disposed = true;
        reset();
    }

    return { ended, setPreference, setPaused, cancel, playNext: () => transition('manual'), retry, reset, dispose, getState };
}

// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import { mount, tick, unmount } from 'svelte';
import { get } from 'svelte/store';
import Player from '../../src/lib/components/Player.svelte';
import { player } from '../../src/lib/stores/player.js';
import { loadPlaybackEntry } from '../../src/lib/playback/entry.js';
import { createDirectLiveObserver } from './direct-live-observer.mjs';
import '../../src/app.css';

const target = document.querySelector('[data-direct-player-root]') || document.body.appendChild(document.createElement('main'));
const observer = createDirectLiveObserver({ player, root: target });
let component = null;
let activeSelection = null;
let closed = false;

function awaitSession() {
    let unsubscribe = () => {};
    let timer;
    const promise = new Promise((resolve, reject) => {
        timer = setTimeout(() => { unsubscribe(); reject(new Error('The actual Player did not start within fifteen seconds.')); }, 15000);
        unsubscribe = player.subscribe((state) => {
            if (state.sessionId && !state.loading && !state.sessionReleased) {
                clearTimeout(timer); unsubscribe(); resolve({ sessionId: state.sessionId, decision: state.streamDecision });
            } else if (state.error) { clearTimeout(timer); unsubscribe(); reject(new Error('The actual Player reported a start failure.')); }
        });
    });
    return { promise, cancel() { clearTimeout(timer); unsubscribe(); } };
}

async function stop() {
    if (!component) return;
    if (!await component.requestClose()) throw new Error('The actual Player could not release its owned session.');
    await unmount(component);
    component = null;
    observer.detach();
}

document.querySelector('button').addEventListener('click', (event) => { void observer.enableAudio(); event.currentTarget.hidden = true; });
window.directFixture = {
    async start(fixture, candidate) {
        await stop();
        observer.reset();
        activeSelection = null;
        closed = false;
        const entry = await loadPlaybackEntry(candidate.item, { fileId: candidate.file });
        const started = awaitSession();
        try {
            component = mount(Player, { target, props: {
                entry, profileId: fixture.profileId,
                preferenceContext: { serverOrigin: location.origin, userId: fixture.userId },
                onactivechange: (state) => { activeSelection = state.selection; },
                onstop: () => { closed = true; },
            } });
            await tick();
            const video = target.querySelector('video');
            if (!video) throw new Error('The actual Player did not mount its video element.');
            observer.attach(video);
            return await started.promise;
        } finally { started.cancel(); }
    },
    snapshot() {
        const state = get(player);
        return { ...observer.snapshot(), sessionId: state.sessionId, decision: state.streamDecision,
            recovery: state.recovery, transcodeId: state.transcodeSessionId, directRevision: state.directSourceRevision,
            positionMs: state.positionMs, offsetMs: state.streamOffsetMs, seeking: state.seeking,
            selectedAudioIndex: activeSelection?.audio?.index ?? null, audioSelectionMode: activeSelection?.audioSelectionMode ?? null,
            recoveryRequested: state.recoveryRequested, sessionReleased: state.sessionReleased, closed };
    },
    stop,
};

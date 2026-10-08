// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026 Duskcue Contributors
//
// This program is free software: licensed under AGPL-3.0
// See LICENSE file for details.

export function countdownSeconds(snapshot) {
    const seconds = /\b(\d+)s\b/.exec(snapshot.text)?.[1];
    if (seconds === undefined) throw new Error('The actual native countdown is unavailable.');
    return Number(seconds);
}

export function assertBackgroundEvidence(background, later, startsBefore, startsAfter, minimized) {
    if (minimized?.action !== 'minimize' || minimized.iconic !== true || !/^[1-9][0-9]*$/.test(minimized.handle || '')) throw new Error('The exact owned Win32 window must actually be minimized.');
    for (const snapshot of [background, later]) {
        if (snapshot.nativeMinimized !== true || typeof snapshot.nativeVisible !== 'boolean' || snapshot.cardFocused || snapshot.menuOpen) throw new Error('Actual native minimization must be the observed countdown pause condition.');
        if (!['visible', 'hidden'].includes(snapshot.visibility) || snapshot.hidden !== (snapshot.visibility === 'hidden')) throw new Error('Document visibility must remain the actual observed browser state.');
        if (snapshot.hidden && !snapshot.events.some((event) => event.visibility === 'hidden' && event.trusted === true)) throw new Error('A claimed browser-hidden transition must have actual trusted visibility evidence.');
    }
    if (!background.text.includes('paused') || later.text !== background.text || startsAfter !== startsBefore || later.path !== background.path) throw new Error('The actual background countdown advanced or initiated another playback.');
}

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

export function assertBackgroundEvidence(hidden, later, startsBefore, startsAfter) {
    for (const snapshot of [hidden, later]) {
        if (snapshot.visibility !== 'hidden' || snapshot.hidden !== true || snapshot.cardFocused || snapshot.menuOpen) throw new Error('Real document hiding must be the sole observed countdown pause condition.');
        if (!snapshot.events.some((event) => event.visibility === 'hidden' && event.trusted === true)) throw new Error('A trusted actual hidden visibility transition was not observed.');
    }
    if (!hidden.text.includes('paused') || later.text !== hidden.text || startsAfter !== startsBefore || later.path !== hidden.path) throw new Error('The actual hidden countdown advanced or initiated another playback.');
}

/*
 * Duskcue — Self-hosted media streaming server
 * Copyright (C) 2026-2026 Duskcue Contributors
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU Affero General Public License as published by
 * the Free Software Foundation, either version 3 of the License, or
 * (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
 * GNU Affero General Public License for more details.
 *
 * You should have received a copy of the GNU Affero General Public License
 * along with this program. If not, see <https://www.gnu.org/licenses/>.
 */

const TIMECODE_RE = /(?:(\d{2,}):)?(\d{2}):(\d{2})[.](\d{3})/;
const XYWH_RE = /#xywh=(\d+),(\d+),(\d+),(\d+)/;

export function parseTimecodeToMs(tc) {
    const m = tc.match(TIMECODE_RE);
    if (!m) return null;
    const h = parseInt(m[1] || '0', 10);
    const min = parseInt(m[2], 10);
    const s = parseInt(m[3], 10);
    const ms = parseInt(m[4], 10);
    return ((h * 60 + min) * 60 + s) * 1000 + ms;
}

export function parseStoryboardVtt(vttText, baseUrl) {
    const cues = [];
    const blocks = vttText.replace(/\r\n/g, '\n').split('\n\n');

    for (const block of blocks) {
        const lines = block.split('\n').filter((l) => l.trim() !== '');
        if (lines.length === 0) continue;
        if (lines[0].startsWith('WEBVTT')) continue;
        if (lines[0].startsWith('NOTE')) continue;

        let timeLineIdx = lines.findIndex((l) => l.includes('-->'));
        if (timeLineIdx === -1) continue;

        const timeParts = lines[timeLineIdx].split('-->');
        if (timeParts.length !== 2) continue;

        const startMs = parseTimecodeToMs(timeParts[0].trim());
        const endMs = parseTimecodeToMs(timeParts[1].split(/\s/)[0].trim());
        if (startMs == null || endMs == null) continue;

        const payloadLines = lines.slice(timeLineIdx + 1);
        const payload = payloadLines.join('\n').trim();
        if (!payload) continue;

        const xywh = payload.match(XYWH_RE);
        if (!xywh) continue;

        const spriteRef = payload.split('#')[0];
        if (!spriteRef) continue;

        const spriteUrl = resolveUrl(spriteRef, baseUrl);

        cues.push({
            startMs,
            endMs,
            spriteUrl,
            x: parseInt(xywh[1], 10),
            y: parseInt(xywh[2], 10),
            w: parseInt(xywh[3], 10),
            h: parseInt(xywh[4], 10),
        });
    }

    cues.sort((a, b) => a.startMs - b.startMs);
    return cues;
}

export function findCueForTime(cues, timeMs) {
    if (cues.length === 0) return null;

    let lo = 0;
    let hi = cues.length - 1;

    if (timeMs < cues[0].startMs) return cues[0];
    if (timeMs > cues[hi].endMs) return cues[hi];

    while (lo <= hi) {
        const mid = (lo + hi) >> 1;
        const cue = cues[mid];

        if (timeMs < cue.startMs) {
            hi = mid - 1;
        } else if (timeMs >= cue.endMs) {
            lo = mid + 1;
        } else {
            return cue;
        }
    }

    if (lo < cues.length) return cues[lo];
    return cues[cues.length - 1];
}

function resolveUrl(ref, baseUrl) {
    if (!baseUrl) return ref;
    try {
        return new URL(ref, baseUrl).href;
    } catch {
        return ref;
    }
}

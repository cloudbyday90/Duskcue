// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

const uuid = /^[a-f0-9]{8}(?:-[a-f0-9]{4}){3}-[a-f0-9]{12}$/i;
const masters = ['hevc-sdr-8', 'hevc-sdr-10', 'av1-sdr-8', 'av1-sdr-10', 'hevc-pq-10', 'hevc-hlg-10'];
const sourceIds = masters.flatMap((master) => (master.startsWith('av1') ? ['mp4', 'mkv', 'webm'] : ['mp4', 'mkv']).map((container) => `${master}-${container}`));

export function assertFormatCases(cases) {
    if (!Array.isArray(cases) || cases.length !== 14 || new Set(cases.map((entry) => entry.id)).size !== 14) throw new Error('The exact fourteen format cases are required.');
    let sdr = 0;
    for (const [index, entry] of cases.entries()) {
        if (entry.index !== index || !sourceIds.includes(entry.id) || entry.id !== `${entry.master}-${entry.container}`
            || !uuid.test(entry.item || '') || !uuid.test(entry.file || '') || !['hevc', 'av1'].includes(entry.codec)
            || ![8, 10].includes(entry.bitDepth) || !['sdr', 'hdr10', 'hlg'].includes(entry.dynamicRange)
            || !entry.master.startsWith(`${entry.codec}-`) || entry.bitDepth !== (entry.master.endsWith('-8') ? 8 : 10)
            || entry.dynamicRange !== (entry.master.includes('-pq-') ? 'hdr10' : entry.master.includes('-hlg-') ? 'hlg' : 'sdr')
            || entry.videoIndex !== 0 || entry.audioIndex !== 2 || entry.audioHz !== 880
            || !/^[a-f0-9]{64}$/.test(entry.sha256 || '') || entry.capability?.type !== 'file') throw new Error('A format case differs from its owned source envelope.');
        if (entry.dynamicRange === 'sdr') sdr += 1;
    }
    if (sdr !== 10) throw new Error('The format corpus must retain ten SDR and four HDR cases.');
    return cases;
}

export function assertFormatStart(record, candidate, client) {
    if (record?.method !== 'POST' || record.path !== '/api/v1/playback/start' || record.bearer !== (client === 'native') || record.cookie !== (client === 'browser')
        || record.request?.media_item_id !== candidate.item || record.request.media_file_id !== candidate.file
        || record.request.device_profile !== null || record.request.force_transcode !== false
        || record.request.quality_mode !== 'manual' || record.request.max_streaming_bitrate !== 1500000
        || record.request.audio_stream_index !== null || record.request.subtitle_stream_index !== null
        || client === 'native' && record.request.force_hls !== true) throw new Error('Format playback did not use its actual conservative client contract.');
    if (candidate.dynamicRange === 'sdr') {
        if (record.status !== 200 || record.reply?.stream_decision !== 'transcode' || !uuid.test(record.reply.session_id || '')
            || !uuid.test(record.reply.transcode_session_id || '')) throw new Error('SDR format playback did not start its real fallback.');
    } else if (candidate.expectedStatus !== 422 || candidate.expectedCode !== 'PLAY_020' || record.status !== candidate.expectedStatus
        || record.error?.code !== candidate.expectedCode || !/HDR/.test(record.error.detail || '')) throw new Error('HDR conversion did not produce its explicit current refusal.');
    return record;
}

export function assertFormatMedia(records, generation, client) {
    const media = records.filter((record) => record.method === 'GET' && record.path.includes(`/transcode/${generation}/`));
    for (const extension of ['.m3u8', '.mp4', '.m4s']) if (!media.some((record) => record.path.endsWith(extension) && record.status === 200)) throw new Error('Authenticated format manifest, initialization or segment delivery is missing.');
    if (media.some((record) => record.bearer !== (client === 'native') || record.cookie !== (client === 'browser'))) throw new Error('Format media crossed its authentication transport.');
}

export function assertFormatPresentation(value, generation) {
    if (value?.generation !== generation || value.currentSource !== value.source || !value.source?.startsWith('blob:')
        || !Number.isSafeInteger(value.frames) || value.frames < 5 || value.ready < 2 || value.width !== 320 || value.height !== 180
        || !Number.isFinite(value.firstMediaTime) || !Number.isFinite(value.lastMediaTime) || value.lastMediaTime <= value.firstMediaTime + 0.4) throw new Error('Fresh format generation presentation was not established.');
    return value;
}

export function assertFormatPaused(value) {
    if (!Number.isInteger(value?.ready) || value.ready < 2 || value.paused !== true || value.ended !== false || value.playVisible !== true || value.pauseVisible !== false) throw new Error('Format pause intent is not established on ready media.');
    return value;
}

export function formatPauseReady(value, replayEnded = false) {
    if (!Number.isInteger(value?.ready) || value.ready < 2 || typeof value.paused !== 'boolean' || typeof value.ended !== 'boolean') return false;
    if (value.ended && !replayEnded) return false;
    return value.paused ? value.playVisible === true && value.pauseVisible === false : !value.ended && value.pauseVisible === true && value.playVisible === false;
}

export function formatSeeks(records) {
    return records.filter((record) => record.method === 'POST' && record.path === '/api/v1/playback/seek' && record.status === 200);
}

export function assertFormatSeek(record, { sessionId, previousTranscodeId, positionMs, client }) {
    if (record?.method !== 'POST' || record.path !== '/api/v1/playback/seek' || record.status !== 200
        || record.bearer !== (client === 'native') || record.cookie !== (client === 'browser')
        || record.request?.session_id !== sessionId || record.request.position_ms !== positionMs
        || record.reply?.session_id !== sessionId || record.reply.position_ms !== positionMs
        || !uuid.test(record.reply.transcode_session_id || '') || record.reply.transcode_session_id === previousTranscodeId) throw new Error('The actual format seek lost its canonical session, position or generation.');
    return record.reply.transcode_session_id;
}

export function initialFormatGeneration(records, start, candidate, client) {
    const watch = records.find((record) => record.method === 'GET' && record.path === `/api/v1/items/${candidate.item}/watch-data` && record.status === 200);
    if (!watch?.watch || watch.bearer !== (client === 'native') || watch.cookie !== (client === 'browser')) throw new Error('The actual format entry watch position was not observed.');
    const rawPosition = Number(watch.watch.resume_position_ms);
    const resumePositionMs = watch.watch.is_watched || !Number.isFinite(rawPosition) ? 0 : Math.max(0, rawPosition);
    if (!Number.isSafeInteger(resumePositionMs) || resumePositionMs >= 8000) throw new Error('The initial format resume position exceeds its source.');
    const seeks = formatSeeks(records);
    if (seeks.length !== (resumePositionMs > 0 ? 1 : 0)) throw new Error('Unexpected initial format generation rotations were observed.');
    const generation = resumePositionMs > 0 ? assertFormatSeek(seeks[0], { sessionId: start.reply.session_id,
        previousTranscodeId: start.reply.transcode_session_id, positionMs: resumePositionMs, client }) : start.reply.transcode_session_id;
    return { resumePositionMs, generation, automaticSeek: seeks[0] || null, seekCount: seeks.length };
}

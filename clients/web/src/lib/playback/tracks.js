import { DEFAULT_VIEWING_PREFERENCES, canonicalLanguage } from '../preferences/model.js';

const TEXT_SUBTITLE_CODECS = new Set(['subrip', 'srt', 'ass', 'ssa', 'webvtt', 'vtt', 'mov_text', 'text']);

function languageCode(value) {
    try { return canonicalLanguage(value); } catch { return null; }
}

function flag(value) {
    if (value === true || value === 1) return true;
    if (value === false || value === 0) return false;
    return null;
}

function disposition(row, key, flatKey = null) {
    if (row.disposition && Object.hasOwn(row.disposition, key)) return flag(row.disposition[key]);
    return flatKey ? flag(row[flatKey]) : null;
}

function validIndex(index) {
    return Number.isInteger(index) && index >= 0 && index <= 2147483647;
}

function normalizeCategory(value, kind) {
    if (!Array.isArray(value)) return [];
    const counts = new Map();
    for (const row of value) {
        if (row && validIndex(row.index)) counts.set(row.index, (counts.get(row.index) || 0) + 1);
    }
    return value.filter((row) => row && validIndex(row.index) && counts.get(row.index) === 1)
        .map((row) => {
            const codecValue = row.codec ?? row.codec_name;
            const codec = typeof codecValue === 'string' ? codecValue.trim().toLowerCase() : null;
            const isTextDescription = disposition(row, 'descriptions');
            return {
                index: row.index,
                language: languageCode(row.language ?? row.tags?.language),
                title: typeof row.title === 'string' ? row.title : null,
                codec,
                channels: Number.isInteger(row.channels) && row.channels > 0 ? row.channels : null,
                isDefault: disposition(row, 'default', 'is_default'),
                isAudioDescription: kind === 'audio' ? disposition(row, 'visual_impaired', 'is_audio_description') : null,
                isHearingImpaired: kind === 'subtitle' ? disposition(row, 'hearing_impaired') : null,
                isForced: kind === 'subtitle' ? disposition(row, 'forced') : null,
                isTextDescription,
                selectable: kind === 'audio' || (TEXT_SUBTITLE_CODECS.has(codec) && isTextDescription !== true),
            };
        }).sort((first, second) => first.index - second.index);
}

export function normalizeTracks(source) {
    const streams = source?.additional_streams ?? source ?? {};
    const audio = normalizeCategory(streams.audio, 'audio');
    const subtitles = normalizeCategory(streams.subtitles, 'subtitle');
    const audioIndices = new Set(audio.map((track) => track.index));
    const conflicts = new Set(subtitles.filter((track) => audioIndices.has(track.index)).map((track) => track.index));
    return {
        fileId: typeof source?.id === 'string' && source.id ? source.id : null,
        audio: audio.filter((track) => !conflicts.has(track.index)),
        subtitles: subtitles.filter((track) => !conflicts.has(track.index)),
    };
}

function sourceDefault(rows) {
    return [...rows].sort((first, second) => Number(second.isDefault === true) - Number(first.isDefault === true)
        || first.index - second.index)[0] || null;
}

function accessibilityRank(value, preferred) {
    if (preferred === null) return 0;
    if (preferred === true) return value === true ? 0 : 1;
    return value === false ? 0 : value === null ? 1 : 2;
}

function matchingTrack(rows, language, accessibility, preferred) {
    return rows.filter((row) => row.language === language)
        .sort((first, second) => accessibilityRank(first[accessibility], preferred) - accessibilityRank(second[accessibility], preferred)
            || Number(second.isDefault === true) - Number(first.isDefault === true) || first.index - second.index)[0] || null;
}

function exactOverride(rows, tracks, override) {
    if (!tracks.fileId || override?.fileId !== tracks.fileId || !validIndex(override.index)) return null;
    return rows.find((row) => row.index === override.index) || null;
}

export function createTrackOverride(kind, track, fileId = null) {
    if (!['audio', 'subtitle'].includes(kind)) throw new Error('Invalid track kind');
    if (!track) return { mode: kind === 'audio' ? 'source_default' : 'off' };
    if (!validIndex(track.index) || track.selectable === false) throw new Error('Track is unavailable');
    return kind === 'audio'
        ? { mode: 'preferred', fileId, index: track.index, language: languageCode(track.language), preferAudioDescription: flag(track.isAudioDescription) }
        : { mode: 'preferred', fileId, index: track.index, language: languageCode(track.language), preferSdh: flag(track.isHearingImpaired) };
}

export function resolveTrackSelection(tracks, preferences, overrides = null) {
    const requestedOverrides = overrides || {};
    const prefs = { ...DEFAULT_VIEWING_PREFERENCES, ...preferences };
    const audioRows = tracks.audio.filter((row) => row.selectable);
    const subtitleRows = tracks.subtitles.filter((row) => row.selectable);
    const sourceAudio = sourceDefault(audioRows);
    let audio = sourceAudio;
    let audioSelectionMode = 'source_default';
    let audioFallback = false;
    let audioLanguage = languageCode(prefs.audio_language);
    let preferDescription = prefs.prefer_audio_description === true;
    const audioOverride = requestedOverrides.audio;
    const exactAudio = audioOverride?.mode === 'preferred' ? exactOverride(audioRows, tracks, audioOverride) : null;
    if (exactAudio) {
        audio = exactAudio;
        audioSelectionMode = 'override';
    } else {
        if (audioOverride?.mode === 'source_default') {
            audioLanguage = null;
            preferDescription = false;
        } else if (audioOverride?.mode === 'preferred' && languageCode(audioOverride.language)) {
            audioLanguage = languageCode(audioOverride.language);
            preferDescription = flag(audioOverride.preferAudioDescription);
        }
        if (audioLanguage) {
            const matched = matchingTrack(audioRows, audioLanguage, 'isAudioDescription', preferDescription);
            audio = matched || sourceAudio;
            audioSelectionMode = matched ? 'preferred' : 'fallback';
            audioFallback = !matched || (preferDescription === true && matched.isAudioDescription !== true);
        } else if (preferDescription === true) {
            const described = audioRows.filter((row) => row.isAudioDescription === true
                && (!sourceAudio?.language || row.language === sourceAudio.language));
            audio = sourceDefault(described) || sourceAudio;
            audioSelectionMode = described.length ? 'preferred' : 'fallback';
            audioFallback = !described.length;
        }
    }

    let subtitleMode = prefs.subtitle_mode === 'always' ? 'always' : 'none';
    let subtitleLanguage = languageCode(prefs.subtitle_language);
    let preferSdh = prefs.prefer_sdh === true;
    const subtitleOverride = requestedOverrides.subtitle;
    const exactSubtitle = subtitleOverride?.mode === 'preferred' ? exactOverride(subtitleRows, tracks, subtitleOverride) : null;
    let subtitle = null;
    let subtitleSelectionMode = 'off';
    let subtitleFallback = false;
    if (subtitleOverride?.mode === 'off') subtitleMode = 'none';
    else if (exactSubtitle) subtitleMode = 'always';
    else if (subtitleOverride?.mode === 'preferred' && languageCode(subtitleOverride.language)) {
        subtitleMode = 'always';
        subtitleLanguage = languageCode(subtitleOverride.language);
        preferSdh = flag(subtitleOverride.preferSdh);
    }
    if (subtitleMode === 'always') {
        const matched = subtitleLanguage ? matchingTrack(subtitleRows, subtitleLanguage, 'isHearingImpaired', preferSdh) : null;
        subtitle = exactSubtitle || matched || sourceDefault(subtitleRows);
        subtitleSelectionMode = exactSubtitle ? 'override' : matched ? 'preferred' : subtitle ? 'fallback' : 'unavailable';
        subtitleFallback = !exactSubtitle && (!matched || (preferSdh === true && matched.isHearingImpaired !== true));
    }

    return {
        audio,
        subtitle,
        sourceAudio,
        audio_stream_index: audio?.index ?? null,
        subtitle_stream_index: subtitle?.index ?? null,
        audioFallback,
        subtitleFallback,
        audioSelectionMode,
        subtitleSelectionMode,
        needsTranscode: !!subtitle || (audio !== null && ['preferred', 'override'].includes(audioSelectionMode)),
    };
}

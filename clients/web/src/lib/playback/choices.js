/*
 * Duskcue — Self-hosted media streaming server
 * Copyright (C) 2026-2026 Duskcue Contributors
 * Licensed under AGPL-3.0. See LICENSE for details.
 */

export function playbackTrackChoices(tracks, selection, { locale, source, off, unknown, description, sdh, forced, unsupported, messageLocales = null }) {
    let languages;
    try { languages = new Intl.DisplayNames([locale], { type: 'language' }); } catch {}
    const label = (track, kind) => {
        let language = unknown;
        if (track.language) {
            try { language = languages?.of(track.language) || track.language; } catch { language = track.language; }
        }
        const parts = [
            { text: language, lang: track.language ? locale : messageLocales?.unknown ?? locale },
            { text: track.title, lang: locale },
            { text: kind === 'audio' && track.isAudioDescription === true ? description : '', lang: messageLocales?.description ?? locale },
            { text: kind === 'subtitle' && track.isHearingImpaired === true ? sdh : '', lang: messageLocales?.sdh ?? locale },
            { text: kind === 'subtitle' && track.isForced === true ? forced : '', lang: messageLocales?.forced ?? locale },
            { text: track.selectable ? '' : unsupported, lang: messageLocales?.unsupported ?? locale },
        ].filter((part) => part.text);
        return { label: parts.map((part) => part.text).join(' · '), parts, locale: parts[0]?.lang };
    };
    return {
        audio: tracks.audio.length ? [
            { value: 'source', label: source, locale: messageLocales?.source ?? locale, selected: ['source_default', 'fallback'].includes(selection?.audioSelectionMode) },
            ...tracks.audio.map((track) => ({ value: String(track.index), ...label(track, 'audio'), selected: ['preferred', 'override'].includes(selection?.audioSelectionMode) && selection?.audio?.index === track.index, disabled: !track.selectable })),
        ] : [],
        subtitles: tracks.subtitles.length ? [
            { value: 'off', label: off, locale: messageLocales?.off ?? locale, selected: !selection?.subtitle },
            ...tracks.subtitles.map((track) => ({ value: String(track.index), ...label(track, 'subtitle'), selected: selection?.subtitle?.index === track.index, disabled: !track.selectable })),
        ] : [],
    };
}

export function playbackQualityChoices(quality, { auto, maximum, bitrate, messageLocales = null }) {
    const limits = new Set([1_500_000, 3_000_000, 6_000_000, 12_000_000, 20_000_000, 40_000_000]);
    if (quality?.quality_mode === 'manual' && quality.max_streaming_bitrate) limits.add(quality.max_streaming_bitrate);
    return [
        { value: 'auto', label: auto, locale: messageLocales?.auto, selected: quality?.quality_mode === 'auto' },
        { value: 'maximum', label: maximum, locale: messageLocales?.maximum, selected: quality?.quality_mode === 'maximum' },
        ...[...limits].sort((a, b) => a - b).map((value) => ({ value: String(value), label: bitrate(value / 1_000_000), locale: messageLocales?.bitrate, selected: quality?.quality_mode === 'manual' && quality.max_streaming_bitrate === value })),
    ];
}

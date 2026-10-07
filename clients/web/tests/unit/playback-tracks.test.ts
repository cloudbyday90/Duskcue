import { describe, expect, it } from 'vitest';
import { createTrackOverride, normalizeTracks, resolveTrackSelection } from '../../src/lib/playback/tracks.js';
import { DEFAULT_VIEWING_PREFERENCES } from '../../src/lib/preferences/model.js';

function file(id = 'first-file') {
    return {
        id,
        additional_streams: {
            audio: [
                { index: 1, codec: 'aac', channels: 2, language: 'eng', disposition: { default: 1, visual_impaired: 0 } },
                { index: 4, codec: 'aac', language: 'en', disposition: { default: 0, visual_impaired: 1 } },
                { index: 7, codec: 'aac', language: ' FRE ', disposition: { default: 0, visual_impaired: 0 } },
                { index: 12, codec: 'aac', language: 'fra', disposition: { default: 0, visual_impaired: 1 } },
            ],
            subtitles: [
                { index: 2, codec: 'subrip', language: 'eng', disposition: { default: 1, forced: 1, hearing_impaired: 0 } },
                { index: 8, codec: 'subrip', language: 'en', disposition: { default: 0, forced: 0, hearing_impaired: 1 } },
                { index: 9, codec: 'ass', language: 'fra', disposition: { default: 0, hearing_impaired: 0 } },
                { index: 10, codec: 'hdmv_pgs_subtitle', language: 'en', disposition: { default: 1, hearing_impaired: 1 } },
                { index: 11, codec: 'webvtt', language: 'en', disposition: { default: 1, descriptions: 1 } },
            ],
        },
    };
}

describe('actual stream metadata normalization', () => {
    it('normalizes scanner aliases and retains valid indices and explicit accessibility flags', () => {
        const tracks = normalizeTracks(file());
        expect(tracks.fileId).toBe('first-file');
        expect(tracks.audio[0]).toMatchObject({ index: 1, language: 'en', codec: 'aac', channels: 2, isDefault: true, isAudioDescription: false });
        expect(tracks.audio[1].isAudioDescription).toBe(true);
        expect(tracks.audio[2].language).toBe('fr');
        expect(tracks.subtitles[0]).toMatchObject({ index: 2, isForced: true, isHearingImpaired: false, selectable: true });
        expect(tracks.subtitles[1].isHearingImpaired).toBe(true);
        expect(tracks.subtitles[3].selectable).toBe(false);
        expect(tracks.subtitles[4]).toMatchObject({ isTextDescription: true, isAudioDescription: null, selectable: false });
    });

    it('never invents accessibility from titles, paths, or ambiguous legacy subtitle flags', () => {
        const tracks = normalizeTracks({
            id: 'AD-SDH-file',
            additional_streams: {
                audio: [{ index: 1, codec: 'aac', language: 'English', title: 'Audio Description' }],
                subtitles: [{ index: 2, codec: 'srt', language: 'en-US', title: 'SDH forced', is_hearing_impaired: true, is_forced: true }],
            },
        });
        expect(tracks.audio[0]).toMatchObject({ language: null, isDefault: null, isAudioDescription: null });
        expect(tracks.subtitles[0]).toMatchObject({ language: null, isDefault: null, isHearingImpaired: null, isForced: null });
    });

    it('accepts additive structured flags but rejects invalid values and respects raw disposition evidence', () => {
        const tracks = normalizeTracks({ audio: [
            { index: 0, is_default: true, is_audio_description: false },
            { index: 1, is_default: 'true', is_audio_description: 2 },
            { index: 2, is_audio_description: true, disposition: { visual_impaired: 0 } },
        ] });
        expect(tracks.audio[0]).toMatchObject({ index: 0, isDefault: true, isAudioDescription: false });
        expect(tracks.audio[1]).toMatchObject({ isDefault: null, isAudioDescription: null });
        expect(tracks.audio[2].isAudioDescription).toBe(false);
    });

    it('drops invalid and conflicting duplicate indices without replacing them with array positions', () => {
        const tracks = normalizeTracks({ audio: [
            null, { index: -1 }, { index: '2' }, { index: 1.5 }, { index: 2147483648 },
            { index: 2, language: 'en' }, { index: 2, language: 'fr' }, { index: 7, language: 'de' }, { index: 0, language: 'en' },
        ], subtitles: {} });
        expect(tracks.audio.map((track) => track.index)).toEqual([0, 7]);
        expect(tracks.subtitles).toEqual([]);
        expect(normalizeTracks({ audio: [{ index: 1 }], subtitles: [{ index: 1, codec: 'subrip' }] }))
            .toEqual({ fileId: null, audio: [], subtitles: [] });
        expect(normalizeTracks(null)).toEqual({ fileId: null, audio: [], subtitles: [] });
    });
});

describe('deterministic profile track defaults', () => {
    it('uses structured source audio and keeps even forced/default subtitles Off without forcing transcode', () => {
        const result = resolveTrackSelection(normalizeTracks(file()), DEFAULT_VIEWING_PREFERENCES);
        expect(result).toMatchObject({ audio_stream_index: 1, subtitle_stream_index: null, needsTranscode: false, audioFallback: false, subtitleFallback: false });
    });

    it('uses declared audio default before stream order and preserves a first-index fallback for legacy metadata', () => {
        const tracks = normalizeTracks({ audio: [{ index: 1, language: 'en' }, { index: 6, language: 'fr', is_default: true }] });
        expect(resolveTrackSelection(tracks, DEFAULT_VIEWING_PREFERENCES).audio_stream_index).toBe(6);
        const legacy = normalizeTracks({ audio: [{ index: 6, language: 'fr' }, { index: 1, language: 'en' }] });
        expect(resolveTrackSelection(legacy, DEFAULT_VIEWING_PREFERENCES).audio_stream_index).toBe(1);
    });

    it('prefers requested audio language and reliable descriptions without claiming an original soundtrack', () => {
        const tracks = normalizeTracks(file());
        const regular = resolveTrackSelection(tracks, { ...DEFAULT_VIEWING_PREFERENCES, audio_language: 'fra' });
        expect(regular).toMatchObject({ audio_stream_index: 7, audioSelectionMode: 'preferred', needsTranscode: true });
        const described = resolveTrackSelection(tracks, { ...DEFAULT_VIEWING_PREFERENCES, audio_language: 'fr', prefer_audio_description: true });
        expect(described).toMatchObject({ audio_stream_index: 12, audioFallback: false, needsTranscode: true });
        const sourceDescribed = resolveTrackSelection(tracks, { ...DEFAULT_VIEWING_PREFERENCES, prefer_audio_description: true });
        expect(sourceDescribed.audio_stream_index).toBe(4);
    });

    it('does not choose a different-language description in place of source-language audio', () => {
        const source = file();
        source.additional_streams.audio = source.additional_streams.audio.filter((row) => row.index !== 4);
        const result = resolveTrackSelection(normalizeTracks(source), { ...DEFAULT_VIEWING_PREFERENCES, prefer_audio_description: true });
        expect(result).toMatchObject({ audio_stream_index: 1, audioFallback: true, needsTranscode: false });
    });

    it('falls back to source audio when language is unavailable and does not force a redundant transcode', () => {
        const result = resolveTrackSelection(normalizeTracks(file()), { ...DEFAULT_VIEWING_PREFERENCES, audio_language: 'es' });
        expect(result).toMatchObject({ audio_stream_index: 1, audioFallback: true, audioSelectionMode: 'fallback', needsTranscode: false });
    });

    it('matches SDH within the preferred subtitle language and avoids bitmap/description streams', () => {
        const result = resolveTrackSelection(normalizeTracks(file()), { ...DEFAULT_VIEWING_PREFERENCES, subtitle_mode: 'always', subtitle_language: 'eng', prefer_sdh: true });
        expect(result).toMatchObject({ subtitle_stream_index: 8, subtitleFallback: false, needsTranscode: true });
        const regular = resolveTrackSelection(normalizeTracks(file()), { ...DEFAULT_VIEWING_PREFERENCES, subtitle_mode: 'always', subtitle_language: 'en' });
        expect(regular.subtitle_stream_index).toBe(2);
    });

    it('uses a supported subtitle fallback when language is unavailable and reports no supported subtitles', () => {
        const preferences = { ...DEFAULT_VIEWING_PREFERENCES, subtitle_mode: 'always', subtitle_language: 'es' };
        const result = resolveTrackSelection(normalizeTracks(file()), preferences);
        expect(result).toMatchObject({ subtitle_stream_index: 2, subtitleFallback: true, subtitleSelectionMode: 'fallback' });
        const source = file();
        source.additional_streams.subtitles = source.additional_streams.subtitles.filter((row) => row.index >= 10);
        const unavailable = resolveTrackSelection(normalizeTracks(source), preferences);
        expect(unavailable).toMatchObject({ subtitle_stream_index: null, subtitleFallback: true, subtitleSelectionMode: 'unavailable', needsTranscode: false });
    });

    it('keeps missing SDH/description flags unknown while still honoring an available language', () => {
        const tracks = normalizeTracks({ audio: [{ index: 1, language: 'en' }], subtitles: [{ index: 2, codec: 'srt', language: 'en', is_hearing_impaired: true }] });
        const result = resolveTrackSelection(tracks, { ...DEFAULT_VIEWING_PREFERENCES, audio_language: 'en', prefer_audio_description: true, subtitle_mode: 'always', subtitle_language: 'en', prefer_sdh: true });
        expect(result.audio.isAudioDescription).toBeNull();
        expect(result.subtitle.isHearingImpaired).toBeNull();
        expect(result).toMatchObject({ audioFallback: true, subtitleFallback: true });
    });
});

describe('title/session track overrides across playback files', () => {
    it('applies an exact current-file audio choice then matches language and AD across reordered next-file indices', () => {
        const current = normalizeTracks(file());
        const override = createTrackOverride('audio', current.audio.find((track) => track.index === 12), current.fileId);
        const preferences = { ...DEFAULT_VIEWING_PREFERENCES, audio_language: 'en' };
        expect(resolveTrackSelection(current, preferences, { audio: override }).audio_stream_index).toBe(12);
        const next = normalizeTracks({ id: 'next-file', additional_streams: { audio: [
            { index: 1, language: 'fr', disposition: { default: 1, visual_impaired: 0 } },
            { index: 6, language: 'fre', disposition: { default: 0, visual_impaired: 1 } },
            { index: 12, language: 'en', disposition: { visual_impaired: 0 } },
        ] } });
        expect(resolveTrackSelection(next, preferences, { audio: override }).audio_stream_index).toBe(6);
    });

    it('matches a selected SDH subtitle semantically on the next episode without reusing its index', () => {
        const current = normalizeTracks(file());
        const override = createTrackOverride('subtitle', current.subtitles.find((track) => track.index === 8), current.fileId);
        expect(resolveTrackSelection(current, DEFAULT_VIEWING_PREFERENCES, { subtitle: override }).subtitle_stream_index).toBe(8);
        const next = normalizeTracks({ id: 'next-file', additional_streams: { subtitles: [
            { index: 4, codec: 'srt', language: 'eng', disposition: { hearing_impaired: 1 } },
            { index: 8, codec: 'srt', language: 'fr', disposition: { default: 1, hearing_impaired: 0 } },
        ] } });
        expect(resolveTrackSelection(next, DEFAULT_VIEWING_PREFERENCES, { subtitle: override }).subtitle_stream_index).toBe(4);
    });

    it('uses unknown-language exact choices only on their original identified file', () => {
        const current = normalizeTracks({ id: 'first-file', additional_streams: { audio: [
            { index: 1, language: 'fr', is_default: true }, { index: 14, language: 'und' },
        ] } });
        const override = createTrackOverride('audio', current.audio[1], current.fileId);
        expect(override.language).toBeNull();
        const preferences = { ...DEFAULT_VIEWING_PREFERENCES, audio_language: 'fr' };
        expect(resolveTrackSelection(current, preferences, { audio: override }).audio_stream_index).toBe(14);
        const next = normalizeTracks({ id: 'next-file', additional_streams: { audio: [
            { index: 3, language: 'fr', is_default: true }, { index: 14, language: 'de' },
        ] } });
        expect(resolveTrackSelection(next, preferences, { audio: override }).audio_stream_index).toBe(3);
    });

    it('does not treat absent file identities as proof that an old index belongs to the current file', () => {
        const current = normalizeTracks({ audio: [{ index: 1, language: 'en' }, { index: 9, language: 'und' }] });
        const override = createTrackOverride('audio', current.audio[1]);
        expect(resolveTrackSelection(current, DEFAULT_VIEWING_PREFERENCES, { audio: override }).audio_stream_index).toBe(1);
    });

    it('makes explicit Source default and Off override saved language/subtitle defaults without saving them', () => {
        const tracks = normalizeTracks(file());
        const preferences = { ...DEFAULT_VIEWING_PREFERENCES, audio_language: 'fr', subtitle_mode: 'always', subtitle_language: 'en' };
        const result = resolveTrackSelection(tracks, preferences, {
            audio: createTrackOverride('audio', null), subtitle: createTrackOverride('subtitle', null),
        });
        expect(result).toMatchObject({ audio_stream_index: 1, subtitle_stream_index: null, needsTranscode: false });
        expect(preferences).toMatchObject({ audio_language: 'fr', subtitle_mode: 'always' });
    });

    it('rejects unavailable track choices and invalid track kinds', () => {
        const tracks = normalizeTracks(file());
        expect(() => createTrackOverride('subtitle', tracks.subtitles[3], tracks.fileId)).toThrow('unavailable');
        expect(() => createTrackOverride('audio', { index: -1 })).toThrow('unavailable');
        expect(() => createTrackOverride('video', null)).toThrow('Invalid track kind');
    });
});

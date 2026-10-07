import { describe, expect, it } from 'vitest';
import { playbackTrackChoices, playbackQualityChoices } from '../../src/lib/playback/choices.js';

describe('playback choice language parts', () => {
    it('keeps the original text and selection while separating Intl names from fallback modifiers', () => {
        const tracks = { audio: [{ index: 4, language: 'en', title: 'Main', selectable: true, isAudioDescription: true }], subtitles: [{ index: 7, language: null, title: null, selectable: false, isHearingImpaired: true, isForced: true }] };
        const choice = playbackTrackChoices(tracks, { audioSelectionMode: 'preferred', audio: { index: 4 }, subtitle: { index: 7 } }, {
            locale: 'fr', source: 'Source default', off: 'Désactivé', unknown: 'Unknown language', description: 'Audio description', sdh: 'SDH', forced: 'Forced', unsupported: 'Unavailable for playback',
            messageLocales: { source: 'en', off: 'fr', unknown: 'en', description: 'en', sdh: 'en', forced: 'en', unsupported: 'en' },
        });
        expect(choice.audio[0]).toMatchObject({ value: 'source', label: 'Source default', locale: 'en', selected: false });
        expect(choice.audio[1]).toMatchObject({ value: '4', label: 'anglais · Main · Audio description', selected: true, disabled: false, locale: 'fr', parts: [{ text: 'anglais', lang: 'fr' }, { text: 'Main', lang: 'fr' }, { text: 'Audio description', lang: 'en' }] });
        expect(choice.subtitles[0]).toMatchObject({ value: 'off', label: 'Désactivé', locale: 'fr', selected: false });
        expect(choice.subtitles[1]).toMatchObject({ value: '7', label: 'Unknown language · SDH · Forced · Unavailable for playback', selected: true, disabled: true, locale: 'en' });
        expect(choice.subtitles[1].parts.every((part) => part.lang === 'en')).toBe(true);
    });

    it('preserves quality values and selection with distinct message languages', () => {
        const choices = playbackQualityChoices({ quality_mode: 'manual', max_streaming_bitrate: 9_000_000 }, { auto: 'Auto', maximum: 'Highest available quality', bitrate: (value) => `Up to ${value} Mbps`, messageLocales: { auto: 'fr', maximum: 'en', bitrate: 'en' } });
        expect(choices[0]).toMatchObject({ value: 'auto', label: 'Auto', selected: false, locale: 'fr' });
        expect(choices.find((choice) => choice.value === '9000000')).toMatchObject({ label: 'Up to 9 Mbps', selected: true, locale: 'en' });
        expect(choices.filter((choice) => choice.selected)).toHaveLength(1);
    });
});

# Playback Track Defaults

## Research and decision (2026-10-03)

The scanner already obtains machine-readable stream indices and language tags with FFprobe JSON `-show_streams`. FFmpeg defines `default`, `forced`, `hearing_impaired`, and `visual_impaired` dispositions independently. Its `descriptions` disposition identifies textual descriptions in a subtitle stream, rather than an audio-description soundtrack. Sources: [FFprobe documentation](https://ffmpeg.org/ffprobe-all.html) and [FFmpeg stream dispositions](https://www.ffmpeg.org/doxygen/trunk/avformat_8h.html).

WAI distinguishes captions containing speech and relevant non-speech audio from descriptions of visual information. A language label or file/track title does not establish either accessibility feature. These are media-selection implementation decisions, not a claim that every file or the entire player satisfies WCAG. Sources: [WAI captions/subtitles](https://www.w3.org/WAI/media/av/captions/) and [WAI description guidance](https://www.w3.org/WAI/media/av/description/).

| Approach | Benefit | Limitation | Decision |
|---|---|---|---|
| Infer accessibility from file or track names | May identify some loosely tagged media. | Can mislabel ordinary tracks; existing subtitle substring heuristics have no reliable provenance. | Do not use for Tonight default matching. |
| Treat the first audio stream as the original soundtrack | Requires little metadata. | Stream order does not establish the original language or soundtrack. | Rejected; the UI uses Source default. |
| Preserve structured dispositions and resolve semantic preferences per file | Uses real stream indices and metadata; retains unknown flags and works across episode track reordering. | Old files need a real reprobe before new metadata is available. | Selected. |

## Scanner contract

`library_scanner/probe.rs` owns FFprobe execution and JSON parsing; the scanner retains its public `ProbeResult` and orchestration. Existing `additional_streams.audio` and `.subtitles` arrays and their fields stay compatible. New rows add nullable `is_default`, nullable audio `is_audio_description`, and a structured `disposition` object containing the original optional integer flags. Only `0` and `1` represent known false/true. Missing or invalid flags remain unknown. Audio description comes from an audio stream's `visual_impaired` disposition; it is never inferred from a file name, language, or title.

Legacy subtitle `is_forced` and `is_hearing_impaired` fields remain for older consumers, including their existing title heuristics. Tonight's resolver reads structured `forced`/`hearing_impaired` dispositions instead of treating these ambiguous legacy fields as reliable evidence. It does not silently backfill disposition values. New/modified files are reprobed normally; the current full scan's mtime diff also skips unchanged files, so starting a full scan does not guarantee metadata backfill.

Example newly probed rows:

```json
{
  "audio": [{"index": 1, "codec": "aac", "channels": 2, "language": "eng", "title": null, "bitrate": 192000, "is_default": true, "is_audio_description": false, "disposition": {"default": 1, "forced": 0, "hearing_impaired": 0, "visual_impaired": 0, "captions": 0, "descriptions": 0}}],
  "subtitles": [{"index": 3, "codec": "subrip", "language": "eng", "title": "English", "is_forced": false, "is_hearing_impaired": true, "is_default": false, "disposition": {"default": 0, "forced": 0, "hearing_impaired": 1, "visual_impaired": 0, "captions": 0, "descriptions": 0}}]
}
```

## Focused client service

`lib/playback/tracks.js` is independent of transport, player state, and the browser. `normalizeTracks(mediaFileOrStreams)` returns `{ fileId, audio, subtitles }` with validated actual integer indices, canonical language codes, display metadata, nullable accessibility/default flags, and subtitle selectability. Unknown language remains null. Duplicate or invalid indices cannot become menu/request choices. Language normalization reuses the profile preference model; unknown/invalid media tags are unavailable for language matching rather than form validation errors.

`resolveTrackSelection(tracks, preferences, overrides)` returns selected rows, request fields `audio_stream_index`/`subtitle_stream_index`, deterministic fallback indicators, and `needsTranscode`. Audio with no requested language uses the structured source/default stream, then the first valid index. A requested language first matches that language, preferring known AD or ordinary audio according to the intent, then source/default audio. With AD requested and no language override, match known AD in the source language where it is known; otherwise use source audio. Missing accessibility flags never become positive claims.

Subtitles Off always resolves to null, including forced/default tracks. Preferred subtitles match the requested language and known SDH intent, then a supported source/default subtitle, then the first supported index. Only supported text-subtitle codecs are automatically selectable through the current server burn-in path. Bitmap subtitles and textual video-description tracks remain visible metadata but are not advertised as supported caption selections. No available supported subtitle resolves to null with an unavailable fallback outcome.

`createTrackOverride('audio' | 'subtitle', track, fileId)` records a title/session choice without saving profile defaults. Exact indices apply only to the same identified file. The next file resolves language and known accessibility intent; unknown-language choices do not carry an index into another episode. A null audio choice means Source default; a null subtitle choice means Off. The caller retains the override object through sequential playback and clears it at the profile/session boundary.

## Playback boundary

The current playback request already accepts nullable audio/subtitle stream indices. Server `build_media_file_info` treats absent/null subtitle index as no selected subtitle, so Off needs no new API mode. A selected index is validated against actual stored streams. Transcoding maps selected audio and burns the selected subtitle; raw DirectPlay serves the complete container and cannot guarantee arbitrary embedded track selection. `needsTranscode` therefore applies when a semantic or same-file explicit audio choice is successfully selected, or when a subtitle is selected. Source-default audio without an applied override does not force transcoding. The player also disables native text tracks for Off.

This service does not claim native subtitle/audio-track support, choose unavailable external subtitle files, or migrate title choices into profile preferences. See [profile viewing preferences](PROFILES_AND_AMBIENT_CHANNELS.md), [media scanning](MEDIA_SCANNING.md), and [streaming](STREAMING.md).

## Player choice presentation (2026-10-03)

The focused `PlayerMenus.svelte` component owns Episodes, Audio & subtitles, and Settings disclosure panels. The player owns transport, selected values, asynchronous errors, and disabled/busy state. The component receives actual episode/track/quality/speed choices and returns `onselect(kind, value)`; it never invents media or tracks. `onopenchange(boolean)` lets the player keep controls visible while a panel is open. Its exported `close(restoreFocus)` and `isOpen()` methods support player lifecycle and Escape coordination.

Native disclosure buttons expose `aria-expanded` and `aria-controls`; panel choices use stable button labels and `aria-pressed` selected state in ordinary Tab order. A custom ARIA application menu or roving keyboard model would add a different interaction contract and is rejected. Native radio auto-selection would commit a playback change during arrow-key traversal; explicit choice buttons make that action deliberate. Each trigger is immediately followed by its panel in DOM order, and only one panel can be open. The panel DOM remains inside the player's fullscreen subtree. Sources: [WAI disclosure pattern](https://www.w3.org/WAI/ARIA/apg/patterns/disclosure/), [WAI disclosure navigation example](https://www.w3.org/WAI/ARIA/apg/patterns/disclosure/examples/disclosure-navigation/), and [WAI button pattern](https://www.w3.org/WAI/ARIA/apg/patterns/button/), checked 2026-10-03.

Selection dismisses the panel and restores trigger focus before invoking the player action. Escape dismisses the open panel first; outside pointer interaction and focus moving outside dismiss without stealing the user's new focus. Trigger and row targets are at least 44 CSS pixels high, labels wrap, panels scroll within viewport limits, and positioning uses logical CSS properties for RTL. No decorative motion is required. Browser qualification after player integration must verify normal Tab order, focus/selected state, Escape priority, outside-click focus preservation, fullscreen containment, mobile reflow, and disabled choices; the component alone does not establish assistive-technology or complete WCAG conformance.

`PlayerPopover.svelte` keeps positioning local to the fullscreen subtree, clamps width and horizontal placement to the player and viewport bounds, and limits height to the space above the controls. It recalculates on opening, player resize, viewport resize, and browser fullscreen change. The two new components produce no errors or warnings in the shared Svelte check; unrelated in-progress player/autoplay diagnostics still prevent the whole check from passing. Browser focus and geometry evidence remains pending integrated player qualification.

## Verification

The focused Rust probe suite passes six cases covering real indices/languages, known and unknown dispositions, preserved primary/video/chapter metadata, legacy subtitle-field compatibility, and malformed JSON rejection:

```text
cargo test -p duskcue --lib workers::library_scanner::probe --locked
```

Eighteen client unit cases cover aliases, invalid/duplicate indices, source/default fallback, AD/SDH unknowns, Off, unsupported subtitles, successful preference selection, and current-file versus next-file overrides. From `clients/web`, run:

```text
node node_modules/vitest/vitest.mjs run tests/unit/playback-tracks.test.ts
```

The owned Rust files pass formatting checks and the pure client service/tests pass TypeScript checking. The verification environment has no FFprobe executable, so the probe suite uses realistic JSON fixtures; actual FFprobe execution and playback/native-track behavior require the player integration and media-capable environment.

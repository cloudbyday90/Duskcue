# Client Accessibility And Input

## Purpose

This document defines the Phase 16d accessibility and input baseline for Duskcue desktop, mobile, TV, and console clients. It is the human-readable companion to the machine-readable fixture pack at [../api/fixtures/accessibility/v1/manifest.json](../api/fixtures/accessibility/v1/manifest.json).

Downstream platform phases must treat accessibility and input as release gates, not polish. A platform can mark a case not applicable only when the platform genuinely lacks the relevant modality.

## Research Summary

Reviewed July 2, 2026.

| Source area | Finding | Duskcue decision |
|---|---|---|
| WCAG 2.2 | Focus order, keyboard operation, contrast, visible focus, target size, captions, reflow, and reduced-motion expectations are stable cross-platform anchors. | Use WCAG 2.2 AA as the shared baseline for web-like surfaces and as the vocabulary for native-client test cases. |
| Android mobile and Android TV | Android accessibility testing emphasizes TalkBack, labels, touch targets, traversal order, contrast, and D-pad navigation on TV. | Android clients must test TalkBack, Android Accessibility Scanner-style checks, touch target sizing, and D-pad reachability for TV. |
| Apple platforms | Apple Human Interface Guidelines emphasize accessible UI, VoiceOver labels, Dynamic Type, Reduce Motion, captions, focus behavior, and platform conventions. | Apple clients must test VoiceOver, Dynamic Type or tvOS text scaling where supported, Reduce Motion, captions/subtitles, and native focus behavior. |
| Windows and Xbox | Microsoft documents Narrator, keyboard/focus behavior, URI/controller input, high contrast, captions, and Xbox accessibility guidelines such as screen narration. | Windows/Xbox clients must test keyboard/controller navigation, Narrator or platform screen narration, high contrast, captions, and media remote behavior. |
| Roku and TV platforms | Roku certification requires accessibility-law compliance and caption setting behavior. Samsung documents TV accessibility guidance; TV apps rely on visible focus and remote navigation. | TV clients must have remote-only navigation, visible focus, caption/subtitle behavior, screen-reader/TTS support where available, and platform-specific accessibility review evidence. |
| Localization and RTL | Existing Duskcue i18n infrastructure already supports direction metadata and review-gated RTL activation. | Non-web clients must keep strings in client catalogs, preserve server-owned strings, mirror directional UI in RTL, and pass RTL smoke cases before activating RTL locales. |

## Baseline Requirements

### Desktop Keyboard

Desktop clients must support:

- keyboard-only navigation through setup, sign-in, home, browse rows, search, media detail, playback controls, settings, account/session controls, and dialogs;
- predictable focus order that follows visual and semantic order;
- visible focus indicators with sufficient contrast and no clipping;
- no keyboard traps in modals, drawers, player overlays, or webview/native shell boundaries;
- standard media shortcuts where implemented, with controls still reachable without shortcuts;
- screen reader names, roles, states, and live updates for toasts/errors.

### Mobile Screen Readers And Touch

Mobile clients must support:

- TalkBack on Android and VoiceOver on iOS for setup, auth, browse, media detail, playback, downloads where applicable, notifications, and settings;
- clear labels, hints, selected state, disabled state, and progress values for controls;
- traversal order that matches visual reading order;
- Dynamic Type or platform text scaling without truncating essential actions;
- reduced-motion settings for nonessential transitions;
- touch targets that meet platform guidance and never rely on tiny icon-only hit areas without labels;
- captions/subtitles and audio/subtitle track selection reachable from playback.

### TV And Console Remote Input

TV and console clients must support:

- remote/controller-only navigation through every visible control;
- one clear focused element at a time;
- predictable D-pad movement within rows, between rows, into player controls, and back out;
- Back/Menu/B/Escape semantics that return to the previous logical view without losing session state;
- focus restoration after dialogs, playback exit, settings panels, search keyboard dismissal, and TV surface refresh;
- caption/subtitle access during playback;
- screen reader, TTS, Voice Guide, Narrator, or platform equivalent where available;
- safe overscan and 10-foot legibility.

### Captions And Subtitles

Playback clients must expose:

- current subtitle/caption state;
- available subtitle and audio track names;
- toggle/select behavior through mouse, touch, keyboard, and remote/controller input as applicable;
- platform global caption preference adoption where the platform exposes it;
- no visual overlap between captions and player controls in normal and reduced-motion states.

### Contrast, Focus, And Motion

Every client must verify:

- text and control contrast meets WCAG AA or the stricter platform requirement when one applies;
- focus indicators are visible against adjacent content and are not hidden by scroll containers, cards, overlays, or player chrome;
- selected/focused/disabled states do not depend on color alone;
- reduced-motion mode disables decorative motion and keeps only motion needed for orientation or playback state;
- long-running loading, scanning, download, and playback states expose status text or screen-reader announcements.

### Localization And RTL

Non-web clients must:

- keep client-owned strings in platform/client catalogs, not hardcoded in code paths;
- preserve server-owned display strings from API fixtures when the server owns the copy;
- keep dates, durations, byte sizes, and counts locale-aware;
- support RTL layout mirroring for Arabic-class locales before activation;
- mirror directional icons and navigation affordances in RTL while keeping media timelines, seek coordinates, and numeric time positions semantically correct;
- document unsupported platform locale APIs as a release note, not silently fall back to broken layouts.

## Fixture Pack

Run:

```bash
node scripts/verify-accessibility-input.mjs
```

The verifier checks that [../api/fixtures/accessibility/v1](../api/fixtures/accessibility/v1) covers every required platform family, baseline category, focus-order case, remote-navigation case, platform review checklist, and localization/RTL case.

## Android TV / Google TV Binding

Phase 17 Task 9 binds the Android TV client to this pack through `TvQualityPolicyTest`. The test asserts the Android TV review checklist and required home/player remote cases; the client applies conservative overscan margins, initial and restored focus targets, visible focus/pressed/disabled states, immediate reduced-motion-safe state changes, TalkBack semantics/live error announcements, and player-time audio/caption selection.

Automated checks cannot prove a specific television's TalkBack speech, overscan, remote firmware, caption preference, or launcher behavior. Those remain recorded emulator/device release evidence in [ANDROID_TV.md](ANDROID_TV.md), not an implied pass from fixture verification.

## Relationship To Other Docs

| Document | Relationship |
|---|---|
| [CLIENT_PLATFORM_READINESS.md](CLIENT_PLATFORM_READINESS.md) | Phase 16d task routing and mandatory gates for Phases 17-23. |
| [TV_PLATFORM_SURFACES.md](TV_PLATFORM_SURFACES.md) | TV remote/focus/deep-link behavior that accessibility tests must cover. |
| [DESKTOP_MOBILE_CLIENTS.md](DESKTOP_MOBILE_CLIENTS.md) | Desktop/mobile client architecture and native platform decisions. |
| [I18N.md](I18N.md) | Locale activation, RTL policy, and reviewed-locale rules. |
| [UI_FOUNDATIONS.md](../branding/UI_FOUNDATIONS.md) | Visual baseline, focus behavior, and UI primitive direction. |

## Research Sources

- W3C WCAG 2.2: https://www.w3.org/TR/WCAG22/
- W3C WCAG 2.2 Quick Reference: https://www.w3.org/WAI/WCAG22/quickref/
- Android accessibility testing: https://developer.android.com/guide/topics/ui/accessibility/testing
- Android TV navigation: https://developer.android.com/training/tv/get-started/navigation
- Android TV focus system: https://developer.android.com/design/ui/tv/guides/styles/focus-system
- Android TV layouts and overscan: https://developer.android.com/design/ui/tv/guides/styles/layouts
- Apple accessibility HIG: https://developer.apple.com/design/human-interface-guidelines/accessibility
- Apple VoiceOver HIG: https://developer.apple.com/design/human-interface-guidelines/voiceover
- Apple keyboards HIG: https://developer.apple.com/design/human-interface-guidelines/keyboards
- Roku certification criteria: https://developer.roku.com/dev/docs/certification
- Roku text to speech: https://developer.roku.com/dev/docs/text-to-speech
- Samsung TV accessibility guide: https://developer.samsung.com/smarttv/develop/guides/fundamentals/accessibility.html
- Microsoft URI activation and keyboard/controller launch context: https://learn.microsoft.com/en-us/windows/apps/develop/launch/handle-uri-activation
- Microsoft Narrator guide: https://support.microsoft.com/en-us/accessibility/windows/narrator/complete-guide-to-narrator
- Xbox Accessibility Guideline 106: https://learn.microsoft.com/en-us/xbox/accessibility/xbox-accessibility-guidelines/106

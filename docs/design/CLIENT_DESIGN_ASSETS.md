# Client Design Assets

## Purpose

This document is the authoritative Phase 16d Task 8 outcome for shared design assets, UI tokens, artwork handling, string ownership, and media-state badges across Duskcue clients.

The goal is visual consistency without a shared UI toolkit. Web, Tauri, Flutter, Android TV, Fire TV, Roku, Samsung Tizen, LG webOS, tvOS, Windows, and Xbox clients should consume the same source assets and fixtures while still using native platform layout, focus, playback, and accessibility patterns.

## Official Source Review

Reviewed July 2, 2026.

| Area | Official sources reviewed | Design impact |
|---|---|---|
| Design tokens | W3C Design Tokens Community Group and Design Tokens Format Module | Store shared decisions as named token values using `$value`, `$type`, and `$description` fields so tools can map them into CSS, Dart, Kotlin, Swift, BrightScript, or platform-specific constants later. |
| Material Design 3 | Material design tokens, typography, spacing, and states | Use semantic tokens instead of hardcoded values, keep type/spacing roles explicit, and define focus/pressed/disabled state behavior as tokens or fixtures. |
| Apple platforms | Human Interface Guidelines, app icons, SF Symbols, focus and selection, tvOS design guidance | Keep source icons simple, recognizable, scalable, and adaptable to Apple focus/tint contexts; preserve platform-native focus behavior rather than forcing a CSS-like ring everywhere. |
| Android | Adaptive icon guidance, Android launcher codelab, Google Play icon specifications, Material 3 Compose theming | Provide app-icon source assets that can be split into adaptive foreground/background/monochrome outputs later, and keep token names mappable to Material color/type/shape roles. |
| Accessibility | WCAG non-text contrast and focus appearance guidance | Focus indicators and media-state badges need visible contrast and cannot rely on color alone. |

## Shared Asset Source

The machine-readable source starts at [../api/fixtures/design/v1/manifest.json](../api/fixtures/design/v1/manifest.json). The checked-in source SVG assets live in [../branding/assets](../branding/assets).

The fixture pack defines:

- shared token groups for color, typography, spacing, radius, shadow, motion, focus, artwork, and badge tones
- app-icon and placeholder artwork source paths
- poster, backdrop, thumbnail, and logo sizing rules
- authenticated and signed artwork loading behavior
- cache-busting, fallback, offline, and unavailable artwork states
- server-owned, client-owned, and shared message-key ownership
- media-state badge names, tones, icon hints, and localization keys
- platform mapping guidance for native token and asset outputs

Run:

```bash
node scripts/verify-design-assets.mjs
```

## Token Strategy

The canonical token fixture uses a DTCG-compatible shape. It is not a generated platform SDK yet. Each platform phase maps these shared names into its native design system:

| Token group | Purpose | Current mapping |
|---|---|---|
| `color` | Surfaces, text, borders, accent, success, warning, error, info | CSS custom properties in `clients/web/src/app.css`; future Material/Swift/native constants |
| `typography` | Body/control/display font families, sizes, line heights, weights | Web `--font-sans` and `--font-display`; native system fonts where appropriate |
| `spacing` | Compact, default, and TV-scale spacing steps | CSS spacing today; native layout constants later |
| `radius` | Small, medium, large component corners | Existing web radius variables |
| `shadow` | Card and elevated surface depth | Existing web shadows where platforms support shadows |
| `motion` | Fast and normal transitions | Existing web transitions; native reduced-motion settings override |
| `focus` | Minimum focus ring/outline behavior | Web `:focus-visible`; native TV/mobile focus systems may use scale, outline, glow, or platform focus effects |
| `artwork` | Aspect ratios, target sizes, and placeholder rules | Artwork delivery endpoint plus placeholder assets |
| `badge` | Media-state tone mapping and label-key ownership | Shared badge fixture for downstream clients |

Tokens are semantic. Platform clients should not expose token names to users or copy raw CSS variable names into UI text.

## App Icon Direction

Duskcue's source app icon is [../branding/assets/app-icon.svg](../branding/assets/app-icon.svg). It is a simple dusk-screen symbol using the existing low-light editorial palette.

Rules:

1. Keep the icon recognizable at TV launcher, phone home-screen, desktop taskbar, and store-listing sizes.
2. Use the SVG as the design source, not as the final platform package asset.
3. Generate platform outputs later from the source: Apple app icon sets, Android adaptive foreground/background/monochrome layers, desktop icons, TV banners, and store listing images.
4. Do not bake shadows into Google Play icons; platform stores and launchers may apply masks or effects.
5. Keep a monochrome-compatible silhouette for Android themed icons and high-contrast launcher contexts.

## Placeholder Artwork

Source placeholders:

| Asset | Path | Usage |
|---|---|---|
| App icon | [../branding/assets/app-icon.svg](../branding/assets/app-icon.svg) | Brand source and future platform icon generation |
| Poster placeholder | [../branding/assets/placeholder-poster.svg](../branding/assets/placeholder-poster.svg) | Missing poster, offline poster unavailable, loading skeleton fallback |
| Backdrop placeholder | [../branding/assets/placeholder-backdrop.svg](../branding/assets/placeholder-backdrop.svg) | Missing hero/backdrop artwork |
| Thumbnail placeholder | [../branding/assets/placeholder-thumbnail.svg](../branding/assets/placeholder-thumbnail.svg) | Episode stills and compact preview imagery |
| Logo placeholder | [../branding/assets/placeholder-logo.svg](../branding/assets/placeholder-logo.svg) | Missing clearlogo/title-art slot |

Placeholder rules:

1. Preserve the target aspect ratio before the image loads so cards and rows do not shift.
2. Do not render private filenames or local paths inside placeholders.
3. Use title text only in the surrounding UI where localization, truncation, and screen-reader behavior are controlled.
4. Include a text or icon state beside color-coded placeholders when the state matters, such as offline, revoked, unavailable, or expired.
5. Keep placeholders visually quiet so real media artwork remains the primary content signal.

## Artwork Sizing

| Artwork type | Aspect ratio | Shared sizes | Default client use |
|---|---:|---|---|
| Poster | `2:3` | `w185`, `w342`, `w500`, `original` | Cards and detail posters |
| Backdrop | `16:9` | `w300`, `w780`, `w1280`, `original` | Hero surfaces, TV rows, detail backgrounds |
| Thumbnail | `16:9` | `w185`, `w300`, `original` | Episode stills, seek/storyboard-adjacent UI |
| Logo | intrinsic transparent wide art | `original` | Detail title-art and hero overlays |

Clients must reserve stable aspect-ratio boxes and choose a size appropriate to rendered CSS/device pixels. TV clients should prefer larger poster/backdrop variants than phone clients because living-room UIs are viewed at a distance and often render on high-DPI displays.

## Artwork Loading Rules

1. Authenticated API-relative artwork URLs use the Duskcue API path and the current session cookie or bearer-token adapter. Web same-origin `<img>` requests rely on cookies; native clients may need authorized fetch-to-file or a platform-specific image loader that can attach headers.
2. Signed URLs are short-lived secrets. They must not be stored in plaintext, logged, embedded in diagnostics, or used as cache keys beyond memory-only active playback/download use.
3. Cache busting uses strong server validators where available: ETag, artwork row UUID, version/revision fields, or package manifest revision. Clients should prefer validator-aware private caches over query-string secrets.
4. Fallback behavior is deterministic: loading skeleton, placeholder source asset, then state badge or recovery action if access/media/artwork is unavailable.
5. Offline packages use protected local copies referenced from the package manifest. They must not fall back to expired remote signed URLs while offline.
6. Unavailable, revoked, expired, deleted, or missing-file states remove stale signed URLs and require server revalidation before playback.

## String Ownership

Server-owned strings:

- media titles, overviews, episode names, collection names, library names, and provider metadata
- notification templates and server-rendered notification text
- RFC 9457 Problem Details `title`, `detail`, and field validation messages
- admin policy labels that are generated from server policy state

Client-owned strings:

- navigation labels, buttons, tabs, headings, empty-state framing, recovery prompts, platform permission explanations, and store-review copy
- platform-specific instructions for sign-in, passkeys, local-network warnings, and permission flows
- accessibility labels for local controls and client-only badges

Shared key reuse:

- Web message keys can be used as naming references for future platform catalogs, but platform clients own generated/native catalogs.
- Clients must not translate server-owned media metadata or server-rendered notification/problem details unless the server provides localized variants.
- Shared badge label keys in the fixture pack are stable contract keys; each client catalog maps them into native localization files.

## Media-State Badges

Media-state badges are small state indicators used on media cards, detail pages, downloads, TV surfaces, and playback recovery surfaces. They must never rely on color alone.

Required states:

- `playable`
- `downloading`
- `offline_ready`
- `unavailable`
- `missing_file`
- `metadata_incomplete`
- `access_revoked`
- `expired`
- `transcode_unavailable`
- `syncing`
- `live`
- `upcoming`

Each badge has a tone token, icon hint, and shared label key in [../api/fixtures/design/v1/media-state-badges.json](../api/fixtures/design/v1/media-state-badges.json). Platform phases can map icon hints to SF Symbols, Material Symbols, Roku/Tizen/webOS local assets, or text fallback labels.

## Platform Consistency Rules

1. Share source tokens, assets, fixtures, and behavior rules.
2. Do not share a UI abstraction layer across every platform.
3. Map tokens into the native platform style system where one exists.
4. Preserve Duskcue nouns, artwork ratios, media-state labels, and auth/artwork safety rules across clients.
5. Let platform-native focus, screen-reader, media-control, and store-artifact requirements win when they conflict with a web-centric implementation detail.

## Implementation Notes

Phase 16d Task 8 adds:

- [../api/fixtures/design/v1](../api/fixtures/design/v1) as the versioned design asset/token fixture pack
- [../../scripts/verify-design-assets.mjs](../../scripts/verify-design-assets.mjs) as the drift gate
- source SVG assets under [../branding/assets](../branding/assets)
- cross-references in `BUILD_ORDER.md`, `PROJECT.md`, `CLIENT_CONTRACTS.md`, `CLIENT_PLATFORM_READINESS.md`, `UI_FOUNDATIONS.md`, and `NAME_BRANDING.md`

## Research Sources

- W3C Design Tokens Community Group: https://www.w3.org/community/design-tokens/
- Design Tokens Format Module: https://www.designtokens.org/tr/drafts/format/
- Material Design 3 design tokens: https://m3.material.io/foundations/design-tokens
- Material Design 3 typography: https://m3.material.io/styles/typography/overview
- Material Design 3 spacing tokens: https://m3.material.io/styles/spacing/tokens
- Material Design 3 states: https://m3.material.io/foundations/interaction/states
- Apple Human Interface Guidelines: https://developer.apple.com/design/human-interface-guidelines
- Apple app icons: https://developer.apple.com/design/human-interface-guidelines/app-icons
- Apple SF Symbols: https://developer.apple.com/sf-symbols/
- Apple focus and selection: https://developer.apple.com/design/human-interface-guidelines/focus-and-selection
- Apple designing for tvOS: https://developer.apple.com/design/human-interface-guidelines/designing-for-tvos
- Android adaptive icons: https://developer.android.com/develop/ui/compose/system/icon_design_adaptive
- Android launcher icon codelab: https://codelabs.developers.google.com/design-android-launcher
- Google Play icon design specifications: https://developer.android.com/distribute/google-play/resources/icon-design-specifications
- WCAG non-text contrast: https://www.w3.org/WAI/WCAG21/Understanding/non-text-contrast.html
- WCAG focus appearance: https://www.w3.org/WAI/WCAG22/Understanding/focus-appearance.html

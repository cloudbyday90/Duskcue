# UI Foundations

## Overview

This document defines the baseline client experience, look and feel, navigation language, and reusable UI surfaces for the product across web, desktop, mobile, and TV. It complements:

- [PROJECT.md](../../PROJECT.md) - top-level product scope, client platform strategy, and project-wide documentation authority
- [NAME_BRANDING.md](NAME_BRANDING.md) - naming criteria, brand tone, and shortlist direction
- [PROJECT_STRUCTURE.md](../design/PROJECT_STRUCTURE.md) - client code structure and route layout
- [AUTH.md](../design/AUTH.md) - setup, invite-code onboarding, and household-user flows that shape the entry experience
- [SECURITY.md](../security/SECURITY.md) - local-first versus exposed posture that shapes trust messaging and admin warnings

The design goal is to create a product UI that feels intentional and modern while still fitting a self-hosted household media platform: content-first, readable from a distance, consistent across client types, and calm enough that settings and administration do not dominate the experience.

## Web and Desktop Redesign — October 2026

The web and shared desktop experience is being redesigned around the **Tonight** concept selected during the October 3, 2026 UI review. This section records the accepted direction for that redesign; the implementation notes below describe the current shipped client.

Production implementation is in progress under [Tonight Implementation Plan](TONIGHT_IMPLEMENTATION_PLAN.md), with T01–T06 complete. Browsing, full Title/Episode galleries, typed profile preferences and the minimal player are implemented. Shared checking/401 units/web build, all twelve actual native groups and both current Linux progressive-output/HTTP lanes pass. The retained `b51369f` client report is 119/120, including captions, progressive playback, locales and Remember/Forget; only actual trusted tab backgrounding remains failed. The narrow `36a99b7` post-navigation test-harness correction awaits its complete browser report. The plan separates accepted requirements, working defaults and exact evidence boundaries; prototype history is not current runtime proof.

Accepted decisions:

- Use Tonight's stable top navigation and cinematic artwork treatment as the web/desktop foundation.
- Use charcoal/plum surfaces with a restrained lavender accent, a readable sans serif for controls, and editorial serif type for featured titles. This updates the earlier brass visual direction for the web/desktop redesign.
- Home starts with **Continue watching**, followed by a smaller featured title, then recently added content. Resuming an existing viewing session takes priority over the feature.
- Movies and TV use the **Poster gallery** layout with larger artwork and more breathing room.
- Catalog posters open a dedicated **Title page** for movies and TV, with playback and episode selection prominent.
- TV title pages use the **Episode gallery**, with landscape thumbnails and a short synopsis for each episode.
- Search submits with Enter (or the search action) and opens a dedicated results page using the poster gallery, media-type filters, viewing-state filters, and sorting.
- Playback uses **Minimal controls**, with dedicated Episodes and Audio & subtitles actions and secondary quality controls in Settings.
- Player selectors use compact, vertical **popovers anchored above their controls**. Choosing an episode, audio/subtitle track, quality, or playback speed applies the choice and dismisses the popover. Outside click and Escape also dismiss it; keyboard selection returns focus to the originating control.
- Playback controls **hide automatically during playback** and return on pointer movement, tapping, or keyboard focus. Keep them visible when paused, while a popover is open, while hovering over controls, or while controls have keyboard focus. The prototype uses a three-second idle delay; that timing can be adjusted during review.
- Include a **fullscreen toggle** in the player controls and an **X in the top-right corner** to close playback and return directly to the same full **Title page**, with its artwork, details, and episode gallery. Retain the selected season, episode, and saved position so the title page can resume that session. Escape exits fullscreen; closing the player also leaves fullscreen. Do not introduce an intermediate playback summary page.
- **Autoplay the next episode** after a ten-second end-of-episode countdown, with **Play now** and **Cancel**. Cancel stops that countdown and leaves a persistent Play next action. Provide a saved **Autoplay next episode: On / Off** preference in player Settings before the countdown is encountered. With autoplay off, wait for an explicit Play next action.

The browsing review compared a Poster gallery with larger artwork and genre labels against a Compact collection with smaller posters, concise metadata, and title-initial browsing. The gallery was selected for its larger artwork and spacing.

The title-detail review selected a dedicated Title page over a Quick view overlay. The title page gives series episodes, synopsis, and secondary actions room to grow and provides a stable navigation destination.

The episode review selected the Episode gallery over compact Episode list rows. Retain a season picker, watched labels, remaining time, explicit Play/Resume actions, and a prominent series Resume action.

The search review selected a dedicated results page over an instant overlay. Typing preserves the current page until the query is submitted. Return from a result's title page to the originating search with its query, filters, and sorting retained.

Implementation defaults established in the execution plan:

- Continue watching cards resume directly; ordinary catalog cards open title details.
- Return from details to the originating collection, preserving its filters and sorting, as with the accepted search behavior.
- Keep episode selection and playback prominent; disclose technical media information on demand.
- Put profile switching, personal Settings, and capability-filtered Administration in the profile area.

These defaults guide the current implementation without claiming a separate design vote for every choice. Profile entry, controls, authenticated artwork and independent loading/empty/error states are integrated with production services. The prototype uses illustrative media and remains separate from application data.

Remaining execution work is the complete corrected browser journey report and final T07/T08 requirement/documentation audit. Backend playback and actual native journeys have source-bound runtime proof; unavailable spoken AT, physical OS and browser-to-live-server qualification remain precisely recorded limits. Preserve the accepted direction and scope rather than restarting the prototype review.

The playback review selected **Minimal controls** over Quick access's persistent audio, subtitles, and quality selectors. The implementation retains pause/resume, seeking, volume, episode changes, actual audio/subtitle choices, auto-hide, fullscreen, full Title return and next-episode autoplay. Preserve existing shortcut behavior; a subtitle appearance editor remains outside this goal. Continue watching resumes directly under the plan's explicit implementation default, while ordinary catalog cards open Title details.

The ten-second autoplay delay is a product choice, not a W3C requirement. [WCAG 2.2 Timing Adjustable](https://www.w3.org/WAI/WCAG22/Understanding/timing-adjustable) requires a way to disable a content-imposed time limit before encountering it, adjust it sufficiently, or extend it under specified conditions. A ten-second Cancel button alone does not establish that requirement; the saved autoplay-off preference supplies the untimed path. End-of-episode controls remain visible, and Cancel never silently restarts the countdown. Keyboard focus inside the next-episode card and an open player popover pause the countdown as additional safeguards; background tabs also pause it.

Announce the upcoming episode, cancellation, and episode change through a polite status region without moving focus or announcing each second, following [WCAG 2.2 Status Messages](https://www.w3.org/WAI/WCAG22/Understanding/status-messages). Use native buttons, visible keyboard focus, and a non-animated countdown; retain the existing reduced-motion support. Carry audio, subtitles, volume, quality, speed, and fullscreen through an episode transition. Stop at the end of the selected season or a movie rather than wrapping to the first episode; cross-season continuation remains a later review item.

The prototype demonstrates the end event by seeking to the end. Production must connect this state to the actual media-ended event, handle unavailable next episodes and playback failures without advancing the countdown, and verify the complete player with assistive technologies. This design review does not establish full WCAG conformance.

Browser review verified the saved autoplay-off preference after closing and reloading, keyboard cancellation, countdown pause with keyboard focus in the next-episode card, automatic advance with fullscreen and playback preferences retained, and completion without wrapping for movies and final season episodes. The end card fits at 320px in both normal and fullscreen layouts. The status region remains inside the fullscreen player and survives episode rendering; the changing seconds use a non-live timer. Actual screen-reader speech still needs assistive-technology testing.

The browser review uses the native [Fullscreen API](https://developer.mozilla.org/en-US/docs/Web/API/Element/requestFullscreen) from a user action and updates its control when fullscreen changes. Fullscreen depends on the embedding host's permission; the standalone review permits it, while an inline preview may not. Production browser fullscreen and supported Tauri fallback are implemented and have actual native journey evidence; [desktop qualification](../ci/TONIGHT_DESKTOP_TESTS.md) records their precise source and fixture scope.

The active standalone review now verifies native fullscreen entry and exit, Escape from a focused player control, and closing with X from fullscreen. X returns to the full title page with the selected season, episode, and playback position retained. The player handles Escape explicitly when no popover is open and clears an earlier fullscreen error after a successful toggle. No browser-wide permission changes were needed.

The player must retain keyboard access, visible focus, readable contrast, and clear control labels, following [W3C's media player guidance](https://www.w3.org/WAI/media/av/player/). The quieter visual treatment is a product preference, not an accessibility requirement.

Current accessibility guidance still requires an identifiable, persistent keyboard focus indicator. The prototypes preserve native controls and add visible product focus treatment, following [W3C's Focus Visible guidance](https://www.w3.org/WAI/WCAG22/Understanding/focus-visible). Layout density remains a product preference, rather than a requirement derived from that guidance.

### Profiles and viewing preferences — in review

The next Tonight review compares **Quick switch menu** with **Full profile picker** for routine profile changes. The recommendation is the compact menu for everyday use: fewer steps and the current page stays visible while choosing. The full picker gives profile identity and device remembering more space, at the cost of leaving browsing and requiring an extra confirmation. Both retain the server-required initial **Who’s watching?** gate; this comparison does not propose bypassing that gate.

The common viewing-preferences proposal uses a quiet page under Tonight's existing navigation, with labeled native controls, explicit **Save changes** and **Discard changes**, and profile scope shown beside playback defaults. Autoplay shares the player's accepted On/Off preference. Audio and subtitle defaults apply when the media provides the selected track; streaming quality is labeled **On this device**. Editing fields does not save or navigate. Leaving an unsaved form offers Keep editing or Discard changes. Profile management is a separate destination from choosing a profile, and Administration is exposed only when the current account/profile capabilities allow it.

The profile popover is a click-opened disclosure with ordinary buttons and a device-remembering checkbox. It exposes expanded state, retains normal Tab order, closes with Escape or outside interaction, and returns focus to its trigger when dismissed with Escape. This follows [WAI's disclosure guidance](https://www.w3.org/WAI/ARIA/apg/patterns/disclosure/examples/disclosure-navigation/); it does not use the ARIA application-menu role for mixed content. Dialogs use native HTML dialog behavior, following [W3C's H102 technique](https://www.w3.org/WAI/WCAG22/Techniques/html/H102). Group related preference controls with fieldsets and legends, following [WAI form grouping](https://www.w3.org/WAI/tutorials/forms/grouping/). Explicit Save is a product choice: [WCAG On Input](https://www.w3.org/WAI/WCAG22/Understanding/on-input) restricts unexpected context changes, rather than requiring every preference to have a Save button. Save and switch results use a persistent polite status region.

Architecture constraints from the three-agent review:

- **Account:** sign-in, role/capabilities, library permissions, and interface locale. The current `/user/preferences` contract accepts only locale. Profile selection does not sign into a different account.
- **Profile:** watch/resume state, favorites, identity and Kids restrictions. Typed production viewing-preference requests/responses now expose profile-owned autoplay, audio-language/description and subtitle defaults, with scope checks and mutation-race protection. Legacy storage is used only under the plan's explicit migration rules; a track selected during playback does not rewrite saved defaults.
- **Session:** the active profile and temporary parent unlock. A profile switch invalidates playback and old content state. Routine switching preserves a validated complete Title context when appropriate; other contexts return Home under the guarded navigation contract.
- **Device:** remembering is an explicit account/device-to-profile mapping. Label it **Remember this profile on this device**, not Remember the last profile. Routine quick switching leaves an existing remembered mapping unchanged unless the user explicitly changes it.

Protected Kids-to-standard switching must prompt for the current Kids profile's parent PIN and wait for server authorization. That unlock does not grant Kids administrative capabilities. Standard-to-standard and standard-to-Kids selection can switch directly. The prototype uses isolated sample profile histories, a curated sample Kids catalog, and a sample parent PIN of `2468` held only in memory; it does not perform authentication, change permissions, or reproduce production lockout policy. Profile names can be edited in the management review; create/delete and parental-policy management remain later review work.

The ownership and layouts above are the execution plan's explicit implementation defaults, distinct from separately accepted visual decisions. Production preference contracts and profile changes use the existing session invalidation, access checks, device remembering and parent-unlock endpoints. Device quality is scoped by server/account/device; volume retains its existing browser-origin/desktop-installation ownership. See [Profiles and Ambient Channels](../design/PROFILES_AND_AMBIENT_CHANNELS.md), [Auth](../design/AUTH.md) and the plan for current source/evidence boundaries. Earlier prototype observations do not establish production endpoint or assistive-technology behavior.

Browser review verified Save and reload persistence, draft preservation and discard, keyboard search warning before leaving an unsaved form, dialog cancellation focus, isolated profile watch history, unchanged audio defaults after a playback-track override, explicit remembered-profile scope, wrong-PIN errors and successful Kids exit, picker confirmation, initial selection gating, and profile-name edits. Preferences, the profile popover, and the picker fit at 320px. The combined comparison has unique live IDs and independent profile state for each variant. Screen-reader speech and production endpoint integration remain to be tested.

### Production integration approach

The implementation uses the existing web semantic CSS variables for Tonight's charcoal/plum surfaces and lavender actions, with a system serif stack for editorial headings. This keeps controls and administration consistent without changing shared Flutter/TV token fixtures or downloading another font. System serif rendering varies by device; readable fallback typography is preferable to a required remote font.

Artwork requests use the API service's selected server and authentication. A shared component reserves the poster or still aspect ratio, lazily requests a profile-scoped blob, and owns its object URL. Component cleanup aborts pending requests and revokes URLs; profile changes destroy the scoped view. This also works for desktop bearer sessions, where a bare remote image cannot attach an Authorization header. The tradeoff is a small request and object-URL lifecycle per visible image, with no persistent cache of private artwork.

Use Svelte's [effect teardown](https://svelte.dev/docs/svelte/$effect#Understanding-lifecycle) for request cleanup and [lifecycle hooks](https://svelte.dev/docs/svelte/lifecycle-hooks) for document listeners. Keep the installed SvelteKit 2 navigation contract. Its [navigation interceptor](https://svelte.dev/docs/kit/$app-navigation#beforeNavigate) supports guarding unsaved preferences across links, submitted search, and browser history; account/profile switches also need an explicit application-level guard before mutating the session. Native disclosures and modal dialogs preserve normal keyboard behavior. These are implementation defaults under the execution plan, rather than additional prototype decisions.

## Goals

1. Define one baseline visual direction for the product before implementation starts.
2. Keep content discovery and playback primary while pushing admin complexity into secondary surfaces.
3. Make the experience coherent across web, desktop, mobile, and TV without forcing identical layouts everywhere.
4. Bake accessibility, keyboard support, and TV focus behavior into the design language from the start.
5. Establish reusable UI primitives and terminology so the first client implementation does not invent them ad hoc.

## Official Research Findings (May 2026)

### Microsoft guidance for design systems and accessibility

- Microsoft recommends following common patterns and metaphors so users can onboard quickly and navigate with less cognitive load.
- Microsoft recommends design systems built from tokens, reusable components, pattern libraries, and usage guidelines.
- Microsoft recommends semantic colors, consistent terminology, and contrast-aware typography to improve usability and accessibility.
- Microsoft recommends keyboard navigation, visible focus, and content structures that remain understandable for assistive technologies.

### Microsoft guidance for UI content

- Microsoft recommends short, scannable, task-focused content instead of feature-centric or decorative copy.
- Microsoft recommends benefit-first messaging, specific verbs, plain language, and consistent terms across the interface.
- Microsoft recommends sentence case and restraint in emphasis rather than all caps or overly branded phrasing.

### Apple guidance for hierarchy, harmony, and consistency

- Apple recommends clear visual hierarchy so people immediately understand what matters most on screen.
- Apple recommends harmony between interface elements and devices rather than visual treatment that fights the platform.
- Apple recommends consistency with platform conventions so the experience adapts across screens without feeling fragmented.

### Android TV guidance for content-first large-screen design

- Android TV guidance states that TV is a 10-foot experience, so text and controls must be larger and simpler than touch-first interfaces.
- Android TV guidance states that D-pad navigation must feel predictable, with a clear path to every focusable element.
- Android TV guidance recommends clear horizontal and vertical axes, content clusters, visible focus indicators, and layouts that avoid cognitive overload.
- Android TV guidance recommends high contrast, large readable type, restrained gradients, and testing against varying TV color and display conditions.
- Android TV guidance notes that TV is commonly a communal device, which supports privacy-aware surface design and household-friendly tone.

## Design Options

| Option | Pros | Cons | Verdict |
|---|---|---|---|
| Utilitarian admin-first interface | Easy to spec for settings screens | Makes the product feel like server software, not a viewing experience | Reject |
| High-gloss streaming-clone interface | Familiar category signals | Derivative, hard to differentiate, too easy to mimic commercial streamer patterns badly | Reject |
| Content-first cinematic utility | Balances warmth, clarity, and operational trust; works across household and admin use | Requires discipline to keep style restrained and accessible | Preferred |
| Completely different visual language per platform | Max platform specialization | Fragments the product identity and multiplies design work too early | Defer |

## Recommended Direction

### Product posture

The baseline UI direction should be **content-first cinematic utility**.

That means:

1. Media, artwork, and playback state are the stars of the interface.
2. Controls should be obvious, calm, and structurally consistent.
3. Administrative power exists, but it should not visually define the product.
4. The interface should feel like a trusted home-cinema tool, not a streaming-service clone and not an enterprise dashboard.

### Earlier cross-client visual baseline

The following baseline describes the earlier shared client direction. Tonight's web/desktop palette above supersedes its brass accent in the actual shared web CSS; existing mobile/TV asset and token contracts retain their own platform scope.

Use a **low-light editorial palette** as the baseline product direction:

- deep charcoal and graphite as foundational surfaces
- warm off-white for primary text
- brass or amber as the primary accent
- muted red only for destructive states and security warnings
- cool green reserved for healthy or successful states

The background should not be flat black. Prefer soft gradients, subtle film-like texture, or tonal surface shifts that add depth without reducing readability.

Avoid these visual traps:

1. bright white app chrome as the default viewing surface
2. over-saturated neon accents
3. heavy glassmorphism that weakens focus states and contrast
4. dashboard density that competes with artwork and playback

### Typography

Use a two-layer type system:

1. **Legibility layer** - a high-readability sans serif for body text, controls, lists, labels, and settings surfaces
2. **Editorial layer** - a restrained display face for hero titles and featured surfaces only

Typography rules:

- favor larger sizes and shorter line lengths on TV and living-room surfaces
- keep body text plain and highly readable
- use sentence case throughout the product UI
- avoid all caps for navigation, buttons, and section headers
- do not use decorative fonts for long-form or control text

### Navigation model

Keep the product nouns consistent across clients even when the layout adapts.

Baseline primary destinations:

1. Home
2. Libraries
3. Search
4. Continue watching
5. Settings

Secondary or context-specific destinations:

1. Downloads
2. Analytics
3. User management
4. Server health

Navigation rules:

1. Put discovery and playback destinations first.
2. Keep administration inside settings or admin-only surfaces rather than in the primary browse path.
3. On TV, use clear vertical and horizontal browsing axes.
4. On web and desktop, prefer a stable left rail or top-level navigation that does not shift between pages.
5. On mobile, keep the same product nouns even if the navigation compresses into tabs and nested views.

### Focus, input, and interaction

Keyboard and D-pad behavior are baseline requirements, not polish.

Interaction rules:

1. Every interactive surface must have a visible focus state.
2. Focus order must be predictable and reachable without traps.
3. Cards, buttons, and chips should differentiate default, focused, pressed, selected, and disabled states.
4. Focus indicators may combine scale, outline, glow, and surface-color change, but they must remain accessible and not feel noisy.
5. The back action should always have a predictable path and should not depend on on-screen back buttons for TV.

### Motion

Motion should communicate focus, state change, and spatial transition, not decorate idle surfaces.

Use motion for:

1. focus transitions on TV and keyboard navigation
2. page and panel transitions that clarify hierarchy
3. playback-control reveal and dismissal
4. staggered appearance of browse rows where it improves orientation

Avoid autoplay animation loops, ornamental parallax, or motion that competes with video artwork.

### UI primitives

The first client implementations should standardize these reusable surfaces early:

1. Featured hero
2. Media card
3. Continue-watching row
4. Detail header with actions
5. Player HUD and transport controls
6. Search field and result grouping
7. Setup and onboarding stepper
8. Admin alert card
9. Empty state
10. Error and recovery surface

### Copy and labeling

Copy should follow a plain, direct, household-friendly style:

1. lead with the benefit or state, then the action
2. use specific verbs such as `Play`, `Resume`, `Scan library`, and `Fix now`
3. keep labels short and stable across the product
4. avoid infrastructure jargon in user-facing areas
5. reserve technical wording for advanced admin surfaces where precision matters

### Privacy and household posture

Because TV is a communal surface, the product should avoid exposing unnecessary personal detail on shared screens.

Baseline implications:

1. profile and account actions should be easy to reach but not constantly foregrounded
2. sensitive admin warnings belong in clearly separated surfaces
3. invite and account-management flows should use calm trust language rather than security theater

## Baseline Screen Set

The first implementation wave should design around these canonical screens:

1. Setup / first run
2. Sign in / invite code entry
3. Home
4. Library browse
5. Media details
6. Playback
7. Search
8. Settings
9. Admin health / alerts

If a proposed component or pattern does not clearly support one of these screens, it is probably not part of the baseline UI system yet.

## Pros vs Cons

### Pros

- Gives the project a distinct product identity without drifting into commercial-streamer imitation.
- Fits both everyday viewing and self-hosted admin work with one coherent design language.
- Keeps TV constraints visible early enough that web-first choices do not break the big-screen experience later.
- Creates a stable foundation for tokens, components, and route-level design decisions.

### Cons

- A cinematic direction can become muddy if contrast and focus states are not tightly controlled.
- Cross-client consistency still requires judgment because layouts cannot be identical everywhere.
- Two-layer typography and richer surfaces add some implementation discipline compared with a purely utilitarian UI.

## Final Recommendation Stack

1. Use a content-first cinematic utility direction as the product baseline.
2. Keep discovery and playback primary, with admin complexity visually secondary.
3. Standardize navigation nouns, focus behavior, and reusable surfaces before detailed page design begins.
4. Use a low-light editorial palette with strong contrast and restrained accent color.
5. Treat keyboard and D-pad navigation as first-class requirements across the client family.

## Three More High-Value Design Areas

1. Define the player-control model in detail for mouse, touch, keyboard, and TV remote input.
2. Define the first-run and remote-access setup UX so self-hosting complexity does not leak into the main product experience.
3. Define the artwork and poster treatment rules for cards, hero surfaces, and details pages.

## Implementation Status

The current Tonight web and shared Tauri palette is implemented as semantic CSS custom properties in `clients/web/src/app.css`. The token names remain stable while their values update the earlier Phase 8 brass direction:

| Token | Value | Maps to |
|---|---|---|
| `--color-bg-deep` | `#111016` | Deep charcoal surface |
| `--color-bg-surface` | `#1b1821` | Plum surface |
| `--color-bg-elevated` | `#26212e` | Elevated plum card/panel |
| `--color-text-primary` | `#f3eef8` | Light primary text |
| `--color-text-secondary` | `#c1b8cd` | Secondary text |
| `--color-accent` | `#c7b8ee` | Restrained lavender actions |
| `--color-success` | `#6abf69` | Cool green (healthy states) |
| `--color-error` | `#f29191` | Readable red error states |

The shared shell, poster/episode galleries, preferences and player components consume these tokens. Minimal controls, vertical disclosures, keyboard/touch reveal, native fullscreen/Escape/X Title return and actual next-episode behavior delegate to focused services with explicit scope and cleanup. Current browser and native evidence is recorded in the plan; real tab backgrounding remains the final open timing qualification. TV/controller input models remain with their respective client phases.

Phase 16d Task 7 adds the cross-client accessibility and input baseline in [CLIENT_ACCESSIBILITY_INPUT.md](../design/CLIENT_ACCESSIBILITY_INPUT.md) plus machine-readable fixtures under [../api/fixtures/accessibility/v1](../api/fixtures/accessibility/v1/manifest.json). Future client phases should use those artifacts to verify focus order, remote/controller navigation, screen reader behavior, captions/subtitles, reduced motion, contrast, touch targets, and localization/RTL behavior instead of interpreting this visual foundation document alone.

Phase 16d Task 8 promotes the visual foundation into a reusable design asset/token contract in [CLIENT_DESIGN_ASSETS.md](../design/CLIENT_DESIGN_ASSETS.md) plus machine-readable fixtures under [../api/fixtures/design/v1](../api/fixtures/design/v1/manifest.json). The pack defines DTCG-compatible token groups, app-icon and placeholder SVG sources under [assets](assets), poster/backdrop/thumbnail/logo sizing rules, focus token expectations, artwork loading/fallback/offline/unavailable behavior, string ownership, media-state badge keys, and platform mapping guidance. Future clients should consume those shared assets and rules while mapping them into native UI systems.

A fifth component, `SkipButton.svelte`, was added in Phase 10 Task 7 (per [SEGMENT_DETECTION.md](../design/SEGMENT_DETECTION.md)). It instantiates the "Player HUD and transport controls" primitive (item 5 of the UI primitives list) — a bottom-right overlay rendered during detected intro/credits/recap/preview/outro windows. It consumes the same semantic tokens, now lavender/plum in web/desktop, implements two-tier prominence (10s timeout for high-confidence segments; 5s for medium-confidence), and respects the focus-visible ring + fly-transition motion rules defined above.

A sixth component, `SeekPreview.svelte`, was added in Phase 10 Task 8 (per [STORYBOARDS.md](../design/STORYBOARDS.md)). It instantiates a seek-preview thumbnail tooltip above the player seek bar — appearing when the user hovers or scrubs the timeline. It consumes the same design tokens (`--color-bg-deep` charcoal for the thumbnail background, `--color-text-primary` for the time label, `--shadow-elevated` for the tooltip shadow, `--radius-sm` for rounded corners), uses the `fade` transition for appearance, and includes a responsive mobile breakpoint at 480px.

### Current Tonight responsive layout

The actual shell in `routes/+layout.svelte` wraps navigation/search at 1100px, gives navigation and search full rows at 600px, and makes the header non-sticky below 420px height so keyboard scrolling can reach short-height controls. Poster and episode galleries reflow with their own semantic controls; the current browser/native suites cover 320 CSS pixels, RTL, reduced motion and genuine native 400% zoom. These checks keep the documented fixture/AT scope.

### Earlier Responsive Layout (Phase 8 Task 6)

The following records the original Phase 8 implementation; its hamburger/drawer behavior and no-observer statement do not describe the current Tonight shell/player.

The web client implements a responsive layout with a two-breakpoint system covering desktop, tablet, and mobile per the navigation model section above ("On mobile, keep the same product nouns even if the navigation compresses into tabs and nested views"):

| Breakpoint | Behavior |
|---|---|
| `≥ 768px` (desktop/tablet) | Full horizontal nav bar with logo, nav links, compact search, user dropdown |
| `< 768px` (mobile) | Hamburger menu button with animated slide-in drawer containing nav links, full-width search, Settings, Sign Out; user avatar only (no name); stacked layouts for detail pages and tables; horizontally scrollable filter bars; 140px card grid minmax |
| `< 480px` (small phone) | Base font size reduced to 15px; auth card padding reduced; main content padding minimized |

All responsive behavior is pure CSS media queries — no JavaScript viewport detection, resize observers, or breakpoint utility libraries. Each Svelte component uses scoped `<style>` media queries, maintaining Svelte's CSS encapsulation. Existing responsive components (`NotificationToast` at 480px, `SearchBar` at 768px, `Player` at 640px) were implemented in Phase 8 Task 4 and left unchanged.

### Admin Settings Refinement (July 12, 2026)

The web client now distinguishes personal Settings from capability-filtered Admin work. `/settings` retains personal language and notification/device controls, while `/admin` contains task-oriented server, library, access, delivery, and migration links. The implementation follows [ADMIN_SETTINGS.md](ADMIN_SETTINGS.md), which defines ownership, accessibility, canonical-route, and future shared-component rules for the full cleanup.

## Official Sources

- Microsoft Learn: Recommendations for following design standards - https://learn.microsoft.com/en-us/power-platform/well-architected/experience-optimization/design-standards
- Microsoft Learn: Recommendations for writing user interface content - https://learn.microsoft.com/en-us/power-platform/well-architected/experience-optimization/user-interface-content
- Apple Human Interface Guidelines - https://developer.apple.com/design/human-interface-guidelines
- Android Developers: Design for TV - https://developer.android.com/design/ui/tv/guides/foundations/design-for-tv
- Android Developers: Navigation on TV - https://developer.android.com/design/ui/tv/guides/foundations/navigation-on-tv
- Android Developers: Focus system - https://developer.android.com/design/ui/tv/guides/styles/focus-system
- Android Developers: Typography - https://developer.android.com/design/ui/tv/guides/styles/typography
- Android Developers: Layouts - https://developer.android.com/design/ui/tv/guides/styles/layouts
- Android Developers: Color on TV - https://developer.android.com/design/ui/tv/guides/foundations/color-on-tv

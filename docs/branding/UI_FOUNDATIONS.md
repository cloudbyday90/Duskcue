# UI Foundations

## Overview

This document defines the baseline client experience, look and feel, navigation language, and reusable UI surfaces for the product across web, desktop, mobile, and TV. It complements:

- [PROJECT.md](../../PROJECT.md) - top-level product scope, client platform strategy, and project-wide documentation authority
- [NAME_BRANDING.md](NAME_BRANDING.md) - naming criteria, brand tone, and shortlist direction
- [PROJECT_STRUCTURE.md](../design/PROJECT_STRUCTURE.md) - client code structure and route layout
- [AUTH.md](../design/AUTH.md) - setup, invite-code onboarding, and household-user flows that shape the entry experience
- [SECURITY.md](../security/SECURITY.md) - local-first versus exposed posture that shapes trust messaging and admin warnings

The design goal is to create a product UI that feels intentional and modern while still fitting a self-hosted household media platform: content-first, readable from a distance, consistent across client types, and calm enough that settings and administration do not dominate the experience.

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

### Visual language

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
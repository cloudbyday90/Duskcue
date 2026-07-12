# Admin Settings Architecture

## Purpose

This document defines the web client's boundary between personal settings and server administration. It replaces the former flat Settings destination with task-oriented, capability-aware administration that keeps routine household preferences separate from server operations.

It complements [UI_FOUNDATIONS.md](UI_FOUNDATIONS.md), [CLIENT_ACCESSIBILITY_INPUT.md](../design/CLIENT_ACCESSIBILITY_INPUT.md), [AUTH.md](../design/AUTH.md), [OFFLINE_DOWNLOADS.md](../design/OFFLINE_DOWNLOADS.md), [NOTIFICATIONS.md](../design/NOTIFICATIONS.md), and [MIGRATION_STRATEGY.md](../design/MIGRATION_STRATEGY.md).

## Research Findings

Reviewed July 12, 2026.

| Source | Finding | Application |
|---|---|---|
| SvelteKit routing | Nested layouts and file-based routes support shared route-level shells without coupling feature logic to a monolithic page. | Reusable Admin UI belongs in `src/lib/components`; route pages retain their domain API adapters and state. |
| Svelte snippets | Snippets and render tags support reusable markup when a full component would add unnecessary API surface. | Prefer small reusable components for stable Admin surfaces; use snippets only for local repeated markup. |
| W3C Tabs Pattern | A tab UI needs `tablist`, `tab`, and `tabpanel` semantics plus the documented keyboard model. | The System configuration selector is navigation, not a tab widget: use native links with URL state instead of incomplete ARIA tabs. |
| W3C Table Pattern | Native HTML tables are preferred for tabular data whenever possible. | Operational data uses semantic tables; card or list layouts are reserved for responsive action-oriented content. |

## Options Considered

| Option | Pros | Cons | Decision |
|---|---|---|---|
| Keep one flat Settings grid | No route changes | Mixes personal preferences, routine admin work, diagnostics, and one-time tools; exposes unavailable destinations | Reject |
| Put all configuration in System | One generic editor | Duplicates specialized forms, hides ownership, and creates a large low-context control surface | Reject |
| Separate Settings and Admin with canonical domain owners | Matches user intent and capabilities; makes advanced work discoverable without overwhelming routine work | Requires a shared navigation layer and deliberate route migration | Adopt |

## Information Architecture

```text
Settings (personal)
├─ Preferences and language
└─ Notification preferences and devices

Admin
├─ Overview and health
├─ Library management
│  ├─ Libraries
│  ├─ Collections
│  └─ Artwork overlays
├─ Access and delivery
│  ├─ Users and invitations
│  ├─ Playback quality and transcoding
│  ├─ Subtitles
│  └─ Download policy and operations
├─ Operations
│  ├─ Backups and recovery
│  ├─ Storage and maintenance
│  └─ Logging and notifications delivery
└─ Advanced
   ├─ Provider integrations
   └─ Migration
```

## Ownership Rules

1. A persisted configuration field has exactly one editable owner.
2. Specialized editors own domain behavior and validation. The generic System editor only owns infrastructure-level configuration that has no dedicated domain surface.
3. Monitoring, inventory, and one-time wizards are operations, not settings.
4. Personal notification preferences and devices remain reachable without server-management capability. Delivery configuration and test dispatch are Admin-only.
5. Deprecated paths redirect to their canonical destination; unavailable roadmap cards do not appear in primary navigation.

## Shared Admin Surface

The web client uses shared Admin primitives for page framing, headers, cards, async states, metrics, forms, and tables. Each primitive preserves native semantics and the application's existing design tokens. Pages retain only domain-specific controls and API orchestration.

## Accessibility Requirements

- Use native links for route changes and native buttons for actions.
- Keep keyboard and screen-reader order aligned with visible order.
- Do not assign tab roles unless the complete W3C keyboard and state model is implemented.
- Keep visible focus states, descriptive labels, and semantic tables for operational data.
- Use localized client strings; do not add English-only Admin copy.

## Implementation Sequence

1. Introduce the personal Settings versus capability-filtered Admin boundary and retire stale placeholder navigation.
2. Establish canonical configuration ownership and shrink the System editor into an advanced infrastructure surface.
3. Extract shared Admin primitives and migrate settings pages onto them.
4. Move operational workflows out of Settings and simplify the densest pages.

## Implementation Record

### July 12, 2026 — Task 1 complete

- Added a capability-filtered `/admin` hub with server health for server administrators and task-oriented links for server, library, user, delivery, and migration work.
- Reduced `/settings` to personal language preference and notification/device access, with an Admin entry only for users holding an administration capability.
- Added shared capability helpers in the auth store so navigation uses the same owner-bypass behavior as existing page access controls.
- Retired the Quality, Security, and Storage placeholder content in favor of permanent redirects to their implemented System configuration groups.
- Made System group selection URL-addressable with `?group=…` and converted the selector from buttons to native navigation links.
- Added complete localized copy for the new Settings and Admin labels in every reviewed locale.

`npm run build` and `npx svelte-check --tsconfig ./jsconfig.json` pass with no errors or warnings.

### July 12, 2026 — Task 2 complete

- Made the dedicated Subtitles surface the sole editor for `server_config.subtitles` and `integrations.subtitle_providers`; the generic System editor no longer renders either duplicate set of controls.
- Redirected legacy `?group=subtitles` System links to the canonical Subtitles page.
- Extracted the data-driven configuration control renderer into `ConfigGroupForm.svelte` and grouped the remaining System navigation by task area.
- Updated capability-gated loads in System, Subtitles, Backups, and Downloads so a capability that arrives after mount still triggers the initial load.
- Corrected the web proxy helper's inferred header-collection type so the full Svelte check again passes.

### July 12, 2026 — Download workflow consolidation

- Made the Downloads page the canonical editor for `server_config.downloads`, pairing policy controls with the operational package inventory that policy governs.
- Removed the Downloads group from System and redirect legacy System deep links to the Downloads page.
- Extracted shared configuration hydration, serialization, dirty-state, and nested-field helpers into `configForms.js`, used by both System and Downloads.
- Kept inventory as a semantic table and made policy saves explicit, preserving one owner for the persisted policy without hiding operational visibility.

### July 12, 2026 — Backup workflow simplification

- Kept readiness and recovery actions in the default Backup view.
- Moved scheduled tasks, historical run evidence, and recovery-drill evidence behind native disclosure controls so routine administration starts with health and action rather than raw operational detail.

### July 12, 2026 — Notification intent split

- Kept notification feed, user preferences, and registered devices in personal Settings.
- Moved server notification test dispatch to an Admin-only route and linked it from the Admin hub.
- Replaced the incomplete ARIA tab semantics in personal Notifications with native pressed-state controls, avoiding a partial tab-widget keyboard contract.

### July 12, 2026 — Authoring and migration route ownership

- Moved the canonical Collections, Overlays, and Migration routes to `/admin/collections`, `/admin/overlays`, and `/admin/migration`.
- Preserved permanent redirects from each legacy Settings URL so existing bookmarks and inbound links remain valid.

### July 12, 2026 — Shared primitive boundary

- Extracted the repeated configuration-group form and its ownership/serialization helpers because those patterns were genuinely duplicated across System and domain configuration pages.
- Deferred generic page, card, metric, async-state, and table wrappers: the reviewed operational pages have materially different state, actions, and evidence layouts, so a generic layer would currently obscure behavior instead of simplifying it.
- Updated the Admin hub and page back actions to keep authoring and one-time migration flows within the Admin hierarchy.

## Sources

- Svelte: [{#snippet ...}](https://svelte.dev/docs/svelte/snippet)
- SvelteKit: [Routing](https://svelte.dev/docs/kit/routing)
- W3C WAI: [Tabs Pattern](https://www.w3.org/WAI/ARIA/apg/patterns/tabs/)
- W3C WAI: [Table Pattern](https://www.w3.org/WAI/ARIA/apg/patterns/table/)

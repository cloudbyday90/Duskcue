# Implementation Debt Schedule

## Overview

This document catalogs the implementation debt created by the 8 strategic design decisions (HTTP_CACHING, REAL_TIME_PUSH, IMAGE_FORMATS, I18N, SEARCH, MULTI_INSTANCE, REVERSE_PROXY, MOBILE_PUSH) and defines when each spec-only item gets built. It is the authoritative schedule for closing the gap between "documented design" and "shipped reality."

Per Fowler's Technical Debt Quadrant, all 8 decisions are **Deliberate + Prudent** debt: we knowingly deferred implementation because the design decision was more important to get right than to implement quickly, and each deferral has a documented exit plan. This is the most desirable quadrant — the risk is letting it drift toward Reckless by never following through. This document prevents that drift.

## Decision — Hybrid Scheduling: Fold Into Consuming Phases + Pre-v1.0 Hardening

**Implementation debt is paid down via a hybrid approach: critical items (those that block or directly enable feature work) are folded into the phase that naturally consumes them; non-blocking quality items are scheduled for a pre-v1.0 hardening pass.**

No artificial "Phase 8.5" or dedicated "refactoring sprint" is created. The research is clear: one-shot refactoring phases feel like detours, disrupt feature momentum, and often don't produce testable results because there's no consumer exercising the infrastructure. Building infrastructure alongside its first consumer means it's tested immediately and integrated naturally.

### Rejected Alternatives

| Approach | Why rejected |
|---|---|
| **Dedicated "Phase 8.5: Strategic Implementation"** between Phase 9 and Phase 10 | Creates a non-feature phase that delays Phase 10 with no testable consumer. Building SSE without a real event producer (storyboard progress) means building blind. Delays the build order for infrastructure that has natural homes in Phase 10+. |
| **One massive pre-v1.0 "hardening sprint"** before Docker/deployment | Concentrates all debt paydown into one risky burst. 20-30 days of implementation without intermediate verification. Some items (SSE, artwork endpoint) block features that should ship earlier. |
| **Interleave everything into every phase evenly (10-15% capacity)** | Spreads debt too thin. Some items (SSE endpoint) are prerequisites for specific phases; others (ETag headers) are independent quality work. Treating them uniformly loses prioritization signal. |

## Debt Catalog

| # | Debt Item | Source Decision | Estimated Effort | Scheduled Phase | Blocks | Status |
|---|---|---|---|---|---|---|
| 1 | **SSE endpoint + EventBus** (`/api/v1/events` + `DashMap<Uuid, broadcast::Sender>` in AppState) | [REAL_TIME_PUSH.md](REAL_TIME_PUSH.md) | 2-3 days | **Phase 10** (storyboard progress is first consumer) | Phase 7 transcode progress migration; Phase 11 analytics dashboard; Phase 13 notification delivery | ✅ Phase 10 Task 11 |
| 2 | **Image pipeline service** (`services/image_pipeline.rs` — WebP encode, resize, variant generation) | [IMAGE_FORMATS.md](IMAGE_FORMATS.md) | 3-5 days | **Phase 10** (storyboard WebP sprites use same service) | Artwork WebP variants; storyboard sprites; overlay composites (Phase 12) | ✅ Phase 10 Task 9 |
| 3 | **Artwork delivery endpoint** (`GET /api/v1/items/{id}/artwork/{type}?size={size}`) | [IMAGE_FORMATS.md](IMAGE_FORMATS.md) | 2-3 days | **Phase 10** (shares `image_pipeline.rs`) | Web client poster rendering (currently gradient placeholders) | ✅ Phase 10 Task 10 |
| 4 | **Web client `events.js` store** (Svelte store managing `EventSource` lifecycle) | [REAL_TIME_PUSH.md](REAL_TIME_PUSH.md) | 1-2 days | **Phase 10** (storyboard progress feed in player) | Player transcode progress; notification center; scan progress | ✅ Phase 10 Task 12 |
| 5 | **Fluent server-side setup** (`fluent-templates` crate, `server/locales/en/` directory, notification template migration) | [I18N.md](I18N.md) | 2-3 days | **Phase 13b** (notification dispatch is forcing function) | Phase 13 notification templates; error message localization (future) | ✅ Phase 13b Task 1 |
| 6 | **Multi-channel dispatch pipeline** (fan-out to in-app + SSE + webhook + mobile push) | [MOBILE_PUSH.md](MOBILE_PUSH.md) | 3-5 days | **Phase 13** (notification system is forcing function) | Phase 13 notification delivery; Phase 16 mobile push | ✅ Phase 13b Task 2 (in-app + SSE + webhook + push stub; FCM/APNs client deferred to Phase 16a) |
| 7 | **`user_push_devices` table + registration API** | [MOBILE_PUSH.md](MOBILE_PUSH.md) | 1-2 days | **Phase 13** (push device management in notifications UI) | Phase 13 push device UI; Phase 16 Flutter registration | ✅ Phase 13b Task 5 |
| 8 | **Webhook dispatch** (HTTP POST to operator-configured URL; ntfy/Gotify/Discord/Slack/generic formats) | [MOBILE_PUSH.md](MOBILE_PUSH.md) | 2-3 days | **Phase 13** (default push channel; no mobile SDK needed) | Phase 13 notification delivery | ✅ Phase 13b Task 4 |
| 9 | **Cache-Control + ETag response headers** | [HTTP_CACHING.md](HTTP_CACHING.md) | 1-2 days | **Pre-v1.0 hardening** | Nothing blocks; quality optimization | ✅ Pre-v1.0 Task 1 |
| 10 | **Paraglide JS adoption** (web client string extraction + initial `en.json`) | [I18N.md](I18N.md) | 3-5 days | **Pre-v1.0 hardening** | Community translation onboarding | ✅ Pre-v1.0 Task 2 |
| 11 | **Faceted search UI** (genre/year/rating filter pills on search results page) | [SEARCH.md](SEARCH.md) | 2-3 days | **Pre-v1.0 hardening** | Nothing blocks; quality feature | Not started |
| 12 | **Prometheus metrics for new infrastructure** (SSE connections, image pipeline, search latency, push delivery) | Cross-cutting (all 8 decisions) | 2-3 days | **Pre-v1.0 hardening** | SEARCH.md trigger threshold is unmeasurable without it | Not started |
| 13 | **Database partition management executor** (scheduled task that creates future partitions for `play_sessions`, `play_events`, `audit_log`) | Phase 2 operational requirement | 1 day | **Immediate** (before current partitions age out) | Data insertion for analytics tables | Not started |

**Total estimated effort: ~25-40 days** (range depends on integration complexity and testing depth).

## Phase-by-Phase Schedule

### Phase 10 — Segment Detection & Storyboards (+ 4 debt items absorbed)

Phase 10 naturally absorbs SSE, image pipeline, artwork endpoint, and web client events store because storyboards are the first real consumer of all four:

| New Task | Debt Item | Rationale |
|---|---|---|
| Implement `server/src/services/image_pipeline.rs` — WebP encode, resize, variant generation | #2 | Storyboard sprite sheets are WebP images; `image_pipeline.rs` is the same service that generates artwork variants. Build once, use for both. |
| Implement artwork delivery endpoint `GET /api/v1/items/{id}/artwork/{type}?size={size}` | #3 | Shares `image_pipeline.rs` with storyboards. Web client currently renders gradient placeholders — delivering real artwork is a visible quality improvement that ships with storyboards. |
| Implement SSE endpoint `GET /api/v1/events` + `EventBus` in AppState | #1 | Storyboard generation is a background task; SSE delivers "generation in progress" → "ready" events to the player. First real SSE consumer. |
| Implement `clients/web/src/lib/stores/events.js` — Svelte store for `EventSource` | #4 | Player subscribes to storyboard-ready events via the events store. First real consumer of the events store. |

**Phase 10 revised task list (existing 8 tasks + 4 new = 12 tasks):**

1-2. Segments domain + service (existing)
3-4. Storyboards domain + service (existing)
5-6. Segment detector + storyboard generator workers (existing)
7-8. SkipButton.svelte + SeekPreview.svelte (existing)
**9. Implement `services/image_pipeline.rs`** (NEW — debt item #2)
**10. Implement artwork delivery endpoint** (NEW — debt item #3)
**11. Implement SSE endpoint + EventBus** (NEW — debt item #1)
**12. Implement `events.js` store** (NEW — debt item #4)

### Phase 13 — System Operations (+ 4 debt items absorbed)

Phase 13 was already the convergence point for i18n, push, and notifications. The 4 additional tasks make the convergence explicit rather than implicit:

| New Task | Debt Item | Rationale |
|---|---|---|
| Set up Fluent server-side i18n (`fluent-templates` crate, `server/locales/en/notifications.ftl`, migrate `notification_types.in_app_template` from English strings to Fluent message IDs) | #5 | Notification templates must be Fluent message IDs before dispatch ships. Forcing function per [I18N.md](I18N.md). ✅ Done in Phase 13b Task 1. |
| Implement multi-channel dispatch pipeline (in-app + SSE + webhook + mobile push fan-out) | #6 | Notification dispatch must be multi-channel from day one. Forcing function per [MOBILE_PUSH.md](MOBILE_PUSH.md). |
| Implement `user_push_devices` table + `POST /api/v1/user/push-devices` API | #7 | Push device registration for FCM/APNs/UnifiedPush tokens. Needed for push delivery. ✅ Done in Phase 13b Task 5. |
| Implement webhook dispatch (ntfy/Gotify/Discord/Slack/generic formats) | #8 | Webhook is the recommended default push channel. No mobile SDK dependency. ✅ Done in Phase 13b Task 4 (5 formats + exponential backoff with full jitter + retryable/non-retryable status classification + `Retry-After` honored). |

**Phase 13 revised task list (existing 11 tasks + 4 new = 15 tasks):**

1-3. System domain, server_config API, scheduled task management (existing)
4. Notification system + **multi-channel dispatch + Fluent templates + webhook** (existing, expanded with debt items #5, #6, #7, #8)
5-9. Backup domain, coordination, 3 workers (existing)
10-11. Admin settings UI + notifications UI (existing)

**Phase 13 scope warning:** With 15 tasks, Phase 13 is the most task-dense phase. Consider splitting into **13a** (system config + scheduled tasks + backup + workers — tasks 1-3, 5-9) and **13b** (notifications + i18n + push — tasks 4, 10-11 + debt items #5-#8). This prevents notification/i18n/push from blocking backup and maintenance features. See "Phase 13 Split Contingency" below.

### Immediate — Database Partition Management (debt item #13)

**Not phase-scheduled; must be done before partitions age out.** Phase 2 created `play_sessions`, `play_events`, and `audit_log` partitions for June/July 2026. Without the `partition_management` executor (registered as a task type in the seed migration but not implemented), these tables will silently fail INSERTs when no partition exists for the current month.

**Implementation:** ~1 day. Register a `partition_management` executor on the scheduler that:
1. Queries `information_schema.tables` for partitioned tables
2. For each, creates the next N months of partitions (default: 3 months ahead)
3. Runs as a scheduled task at `0 0 1 * *` (monthly at midnight on the 1st)

**Priority: immediate** — do this before starting Phase 10.

### Pre-v1.0 Hardening (debt items #9-#12)

After Phase 14 (migration) and before Phase 15 (Docker/deployment), a hardening pass handles quality items that don't block features:

| Task | Debt Item | Effort |
|---|---|---|
| Cache-Control + ETag response headers on metadata/artwork endpoints | #9 | 1-2 days |
| Paraglide JS adoption — extract existing web client strings to `en.json`, configure Paraglide Vite plugin | #10 | ✅ Done in Pre-v1.0 Task 2 |
| Faceted search UI — genre/year/rating filter pills on search results page | #11 | 2-3 days |
| Prometheus metrics for SSE connections, image pipeline, search latency, push delivery | #12 | 2-3 days |

**Total: ~8-13 days.** This is the "polish before shipping" pass. Items here don't block features; they improve quality, performance observability, and i18n readiness.

## Phase 13 Split — Decision (Not Contingency)

**Phase 13 is formally split into Phase 13a (System Operations Core) and Phase 13b (Notification System).** Dependency analysis (see [PHASE_13_SPLIT.md](PHASE_13_SPLIT.md)) confirms a clean boundary: the notification system has zero dependencies on backup/maintenance workers, and Phase 14 (Migration) does not depend on the notification system. The split unblocks Phase 14 to proceed after Phase 13a without waiting for the notification/i18n/push work in Phase 13b.

See [PHASE_13_SPLIT.md](PHASE_13_SPLIT.md) for the full dependency analysis, split rationale, task assignment, and edge cases.

## Debt Tracking

Each strategic design doc includes an "Implementation Status" table. This document is the cross-cutting rollup:

| Strategic Decision | Implementation Status | First Implementation Phase |
|---|---|---|
| [HTTP_CACHING.md](HTTP_CACHING.md) — Cache-Control + ETag | ✅ Pre-v1.0 Task 1 | Pre-v1.0 |
| [REAL_TIME_PUSH.md](REAL_TIME_PUSH.md) — SSE + EventBus | Spec only → **Phase 10** | Phase 10 |
| [IMAGE_FORMATS.md](IMAGE_FORMATS.md) — WebP pipeline + artwork endpoint | Spec only → **Phase 10** | Phase 10 |
| [I18N.md](I18N.md) — Paraglide + Fluent | ✅ Fluent implemented (Phase 13b Task 1); ✅ Paraglide web integration (Pre-v1.0 Task 2); 7-locale translations + RTL review + activation remain | Phase 13b + Pre-v1.0 |
| [SEARCH.md](SEARCH.md) — PG FTS (default) + Meilisearch (post-v1.0) | ✅ PG FTS implemented (Phase 2) | Done |
| [MULTI_INSTANCE.md](MULTI_INSTANCE.md) — Single-instance via lockfile | ✅ Implemented (Phase 3) | Done |
| [REVERSE_PROXY.md](REVERSE_PROXY.md) — Built-in TLS + trusted proxies | ✅ Implemented (Phase 3) | Done |
| [MOBILE_PUSH.md](MOBILE_PUSH.md) — Multi-channel dispatch + push | Spec only → **Phase 13** | Phase 13 |
| **Partition management** — Future partition creation | Not started → **Immediate** | Before Phase 10 |

## Key Decisions

1. **Hybrid scheduling: fold critical debt into consuming phases, defer non-blocking to pre-v1.0 hardening** — Critical items (SSE, image pipeline, artwork endpoint) naturally belong in Phase 10 because storyboards are their first consumer. Building infrastructure alongside its consumer means it's tested immediately and integrated naturally.
2. **No dedicated "refactoring phase"** — One-shot infrastructure phases feel like detours, produce untestable code (no consumer), and disrupt feature momentum. Infrastructure lands where it's consumed.
3. **Phase 10 absorbs SSE + image pipeline + artwork endpoint + events store (4 items, ~10-13 days)** — Storyboards are WebP images (needs `image_pipeline.rs`), their generation is a background task (needs SSE for progress), and the player needs real-time updates (needs `events.js`). Natural fit.
4. **Phase 13 absorbs Fluent + dispatch pipeline + push devices + webhook (4 items, ~8-13 days)** — Already the convergence point for i18n + push + notifications. Makes the convergence explicit rather than implicit.
5. **Pre-v1.0 hardening handles quality items (4 items, ~8-13 days)** — ETag/Cache-Control, Paraglide migration, faceted search UI, Prometheus metrics. None block features; all improve v1.0 quality.
6. **Database partition management is immediate** — Operational time bomb: Phase 2 partitions through June/July 2026; `partition_management` executor not wired up. ~1 day of work; do before Phase 10.
7. **Phase 13 split contingency pre-documented** — If 15 tasks make Phase 13 a bottleneck, split into 13a (system/backup/maintenance) + 13b (notifications/i18n/push). Phase 14+ can proceed after 13a without waiting for 13b.
8. **Debt is Deliberate + Prudent per Fowler's quadrant** — Every deferral was a conscious design decision with a documented exit plan (the strategic MD). This is the most desirable debt category. The risk is letting it drift to Reckless by never implementing — this document prevents that by assigning every item to a specific phase.
9. **Effort estimates are ranges (25-40 days total)** — Reflects uncertainty in integration complexity. Phase 10 items skew higher because SSE + EventBus is new infrastructure; Phase 13 items skew lower because notification dispatch builds on existing patterns (scheduler, worker tasks).
10. **Cross-cutting metrics (#12) is the highest-leverage pre-v1.0 item** — Without it, SEARCH.md's migration trigger threshold ("200ms p95") is unmeasurable. Every operational claim from the 8 strategic decisions needs verification.

## Relationship to Other Documents

| Document | Relationship |
|---|---|
| [BUILD_ORDER.md](../../BUILD_ORDER.md) | Phase 10 task list gains 4 items; Phase 13 task list gains 4 items; pre-v1.0 hardening section added |
| [PROJECT.md](../../PROJECT.md) | Open Questions tracks implementation status of each strategic decision |
| All 8 strategic design docs | Each has an "Implementation Status" table; this document is the cross-cutting rollup that assigns phases |

## Research Sources

- **[Martin Fowler: Technical Debt Quadrant](https://martinfowler.com/bliki/TechnicalDebtQuadrant.html)** — The foundational framework. Duskcue's strategic decisions are all "Deliberate + Prudent" — the most desirable quadrant. Key insight: "prudent-inadvertent" debt is inevitable even for excellent teams; the danger is allowing "deliberate-prudent" to drift to "deliberate-reckless" by never implementing.
- **[Aviator: Technical Debt and the Role of Refactoring](https://www.aviator.co/blog/technical-debt-and-the-role-of-refactoring/)** — Three-pillar framework (Practices, Inventory, Process). Duskcue has Practices (design docs) and Inventory (this document); Process is the phase scheduling.
- **[CodeAnt: The 4 Types of Technical Debt](https://www.codeant.ai/blogs/what-are-the-4-quadrants-of-technical-debt)** — Modern refinement with 2025 data: "teams who actively manage and prioritize technical debt deliver 30-50% faster release cycles." Validates the investment in explicit debt scheduling.

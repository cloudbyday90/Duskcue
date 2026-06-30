# Phase 13 Split Decision

## Overview

This document is the authoritative decision to split Phase 13 (System Operations) into two independent phases: **Phase 13a (System Operations Core)** and **Phase 13b (Notification System)**. The split is based on dependency-graph analysis showing that the notification system has zero cross-dependencies with backup/maintenance workers, and that Phase 14 (Platform Migration) does not depend on the notification system.

This is a build-order structural decision, not a technology decision. It modifies [BUILD_ORDER.md](../../BUILD_ORDER.md) Phase 13 and updates [IMPLEMENTATION_DEBT.md](IMPLEMENTATION_DEBT.md) to upgrade the "Phase 13 Split Contingency" from a contingency to a committed decision.

## Decision — Split Phase 13 into 13a + 13b

**Phase 13 is formally split into two phases with a clean dependency boundary.** Phase 14 (Migration) proceeds after Phase 13a without waiting for Phase 13b. This reduces the convergence risk identified across [I18N.md](I18N.md), [MOBILE_PUSH.md](MOBILE_PUSH.md), and [IMPLEMENTATION_DEBT.md](IMPLEMENTATION_DEBT.md) where three strategic decisions converged on a single 16-task phase.

### Why Split (Not "Run It as One Phase")

| Concern | One 16-task Phase 13 | Split 13a + 13b |
|---|---|---|
| **Critical path to v1.0** | All of Phase 13 must complete before Phase 14 can start | Phase 14 starts after 13a (10 tasks); 13b (6 tasks) runs in parallel or after |
| **Risk concentration** | Fluent + multi-channel dispatch + push + backup + maintenance all in one phase; one delay blocks everything | Notification/i18n/push risk isolated in 13b; backup/maintenance ships independently in 13a |
| **Task density** | 16 tasks (most task-dense phase in build order) | 10 tasks (13a) + 6 tasks (13b); manageable per-phase scope |
| **Verifiability** | Phase 13 verification mixes backup/maintenance/notifications — hard to test incrementally | 13a verifies independently (system config + backup + workers); 13b verifies independently (notifications + push) |
| **Parallelism** | Strictly sequential | 13b can overlap with Phase 14 if developer capacity allows |
| **Fallback option** | If notification system proves harder than estimated, it blocks backup + maintenance too | 13b can ship minimal (in-app + SSE + webhook, no mobile push) without delaying 13a features |

### Dependency Analysis

The split boundary is determined by the inter-task dependency graph. Tasks grouped by cluster:

```
┌─────────────────────────────────────────────────────────────────────┐
│                    Phase 13a — System Operations Core               │
├─────────────────────────────────────────────────────────────────────┤
│                                                                      │
│  Task 1: System domain (five-file pattern)                          │
│     ↓                                                                │
│  Task 2: server_config runtime API                                  │
│  Task 3: Scheduled task management                                  │
│     ↓                                                                │
│  Task 10: Admin settings UI (renders ALL server_config fields)     │
│                                                                      │
│  Task 4: Backup domain (five-file pattern)                          │
│     ↓                                                                │
│  Task 5: Backup coordination (WAL-G, pg_dump)                       │
│     ↓                                                                │
│  Task 6: backup_runner worker                                       │
│                                                                      │
│  Task 7: reindex_maintenance worker (standalone)                    │
│  Task 8: disk_space_check worker (standalone)                       │
│  Task 9: recovery_drill_runner worker (backup restore proof)        │
│                                                                      │
│  Cluster dependencies: Phase 5 (scheduler), Phase 3 (AppState)     │
│  Cross-cluster dependencies: NONE                                   │
│                                                                      │
└─────────────────────────────────────────────────────────────────────┘

         ═══════════════════════════════════════
         ║  CLEAN BOUNDARY — NO CROSS-DEPS  ║
         ═══════════════════════════════════════

┌─────────────────────────────────────────────────────────────────────┐
│                    Phase 13b — Notification System                   │
├─────────────────────────────────────────────────────────────────────┤
│                                                                      │
│  Debt #5: Fluent server-side setup                                  │
│     ↓                                                                │
│  Task 4: Notification system (multi-channel dispatch + templates)   │
│     ←  Debt #6: Multi-channel dispatch pipeline                     │
│     ←  Debt #8: Webhook dispatch                                    │
│     ←  Phase 10: SSE EventBus (already scheduled)                   │
│     ↓                                                                │
│  Task 11: Notifications UI + push device management                 │
│     ←  Debt #7: user_push_devices table + registration API          │
│                                                                      │
│  Cluster dependencies: Phase 10 (EventBus), Phase 13a Task 2        │
│                        (server_config API for push/webhook config)  │
│  Cross-cluster dependencies: server_config API (generic JSONB       │
│                              CRUD — doesn't need notification code) │
│                                                                      │
└─────────────────────────────────────────────────────────────────────┘
```

**Key finding:** The notification system (Cluster B) has exactly ONE dependency on Cluster A: `server_config` API (Task 2) for reading/writing push and webhook configuration in `server_config.integrations` JSONB. But Task 2 is a generic JSONB CRUD endpoint — it handles push/webhook config the same way it handles every other config field. No notification-specific code needed in Task 2.

### Downstream Phase Dependencies

| Phase | Depends on Phase 13a? | Depends on Phase 13b? | Can proceed after 13a alone? |
|---|---|---|---|
| **Phase 14** (Platform Migration) | ❌ No | ❌ No | ✅ Yes — migration imports watch history; needs core media + auth + playback (Phases 2-7), not notifications or backup |
| **Phase 15** (Docker & Deployment) | ✅ Needs system domain + backup in the binary | ✅ Ideally — notification system should be in the Docker image for v1.0 | ✅ Can start packaging after 13a + Phase 14; finalizes after 13b |
| **Phase 16a** (Desktop & Mobile) | ❌ No | ✅ Mobile push + notification center are Phase 16a features | ✅ Can start Tauri wrapper after 13a + Phase 15; Flutter push needs 13b |

### Phase 13a — System Operations Core (10 tasks)

**Goal:** Server config management, backup system, scheduled maintenance workers, admin settings UI. The operational backbone of Duskcue.

**Tasks:**

1. Create `server/src/domains/system/` — five-file pattern (already partially built from Phase 6 Task 12 — provider validation endpoint)
2. Implement `server_config` runtime API — get/update JSONB config fields; generic CRUD for all config groups including push/webhook settings
3. Implement scheduled task management — list, trigger, cancel, view history; admin UI for task management
4. Implement `server/src/domains/backup/` — five-file pattern
5. Implement backup coordination — WAL-G status check, pg_dump trigger, verification
6. Implement `server/src/workers/backup_runner.rs` — scheduled backup execution
7. Implement `server/src/workers/reindex_maintenance.rs` — weekly REINDEX CONCURRENTLY
8. Implement `server/src/workers/disk_space_check.rs` — 30-minute disk monitoring
9. ~~Implement `server/src/workers/recovery_drill_runner.rs` — manual/scheduled restore drills in disposable PostgreSQL; restore latest `pg_dump` or WAL-G backup, run structural checks, and persist evidence in `scheduled_task_runs.stats`~~ **DONE**
10. ~~Build admin settings UI — all `server_config` JSONB fields as toggles, sliders, dropdowns; push/webhook config fields are visible but annotated "Activation requires Phase 13b — notification dispatch"; backup panel shows latest recovery-drill evidence~~ **DONE**

**Phase 13a status:** All 10 tasks complete. Recovery drill ships as `backup_recovery_drill` scheduled task — restores latest pg_dump into disposable PostgreSQL via Docker Compose, runs 3 structural checks (schema migrations applied, core tables present, row count sample), and persists a full evidence bundle in `scheduled_task_runs.stats`. WAL-G physical restore is deferred until the embedded PostgreSQL layout is finalized for packaged deployments (Phase 15).

**Verification:** Admin can configure all settings via UI. Backups run on schedule. Disk space alerts trigger when thresholds are exceeded. Scheduled tasks are visible and triggerable. Recovery drills prove that at least one recent backup can be restored into disposable infrastructure.

### Phase 13b — Notification System (6 tasks)

**Goal:** Notification dispatch with multi-channel delivery (in-app + SSE + webhook), localized templates via Fluent, and push device registration for future mobile push.

**Prerequisites:** Phase 10 (SSE EventBus), Phase 13a Task 2 (server_config API).

**Tasks:**

1. ~~Set up Fluent server-side i18n — `fluent-i18n` crate, `server/locales/en/notifications.ftl`, migrate `notification_types.in_app_template` from English strings to Fluent message IDs (debt item #5 from [IMPLEMENTATION_DEBT.md](IMPLEMENTATION_DEBT.md))~~ **DONE** — crate switched to `fluent-templates` (async-safe explicit per-call locale); see BUILD_ORDER.md Phase 13b Task 1 notes and I18N.md "Crate Selection Rationale"
2. Implement multi-channel dispatch pipeline — notification record always in DB; fan-out to in-app + SSE + webhook simultaneously; mobile push channel included in fan-out. Phase 13b shipped the push lifecycle/API boundary; Phase 16a Task 9 later completed FCM/APNs/UnifiedPush provider delivery.
3. Implement notification CRUD — create, list, mark-as-read, delete; notification types and user preferences from Phase 2 tables
4. Implement webhook dispatch — HTTP POST to operator-configured URL with ntfy/Gotify/Discord/Slack/generic formats; HMAC signing; retry with backoff (debt item #8)
5. ~~Create `user_push_devices` table + `POST /api/v1/user/push-devices` API — device registration for FCM/APNs/UnifiedPush tokens; token lifecycle (heartbeat, auto-invalidation, manual revoke) (debt item #7)~~ **DONE** — see BUILD_ORDER.md Phase 13b Task 5 notes
6. ~~Build notifications UI — notification center, preferences, push device management, per-channel opt-in per notification type~~ **DONE** — see BUILD_ORDER.md Phase 13b Task 6 notes

**Verification:** Admin triggers a test notification. Notification appears in-app (notification center), via SSE (live update if web client is open), and via webhook (operator-configured endpoint). Notification templates render in the user's preferred locale via Fluent. Push devices register and display in user settings.

**Phase 13b status:** All 6 tasks complete (Fluent i18n infrastructure + template migration; multi-channel dispatch pipeline with DB-write-first + SSE fan-out + webhook with HMAC signing + Phase 13b push placeholder later completed by Phase 16a Task 9; in-app notification center CRUD with cursor pagination + preferences + admin test dispatch; webhook format-specific dispatch [generic/ntfy/gotify/discord/slack] + HMAC signing for all formats + exponential-backoff retry with full jitter + retryable/non-retryable status classification + `Retry-After` honored; `user_push_devices` table + registration/heartbeat/revoke API + 30-day stale-device deactivation wired into `notification_cleanup`; notifications UI — navbar bell + dropdown + persistent notification center store with SSE + polling + full-page Feed/Preferences/Push-Devices/Admin-Test hub). 0 svelte-check warnings, 0 build errors. See [MOBILE_PUSH.md](MOBILE_PUSH.md) "Phase 13b Task 5 implementation notes" for the push device registration design and BUILD_ORDER.md Phase 13b Task 6 notes for the notifications UI.

### Minimal Viable Notification System (Fallback)

If Phase 13b takes longer than estimated, the notification system can ship in a minimal form and defer the rest:

| Component | MVP (ship first) | Full (ship later) |
|---|---|---|
| In-app notifications | ✅ | ✅ |
| SSE delivery (foreground) | ✅ | ✅ |
| Webhook delivery | ✅ | ✅ |
| Fluent localized templates | ✅ | ✅ |
| `user_push_devices` table + API | ✅ (schema + API; no push client yet) | ✅ |
| FCM client implementation | ❌ Defer to Phase 16a | ✅ |
| APNs client implementation | ❌ Defer to Phase 16a | ✅ |
| UnifiedPush integration | ❌ Defer to Phase 16a | ✅ |

The Phase 13b MVP delivered all in-app + SSE + webhook notifications plus the push-device schema/API. Phase 16a Task 9 later completed mobile push provider delivery (FCM/APNs/UnifiedPush) where the Flutter mobile app provides the consumer.

## Edge Cases

### Admin Settings UI Spans Both Clusters

Task 10 (admin settings UI, in Phase 13a) renders ALL `server_config` fields, including push/webhook configuration from `server_config.integrations`. These fields are visible in Phase 13a but have no effect until Phase 13b's notification dispatch ships.

**Implemented Task 10 UI boundary:** `/settings/system` renders the JSONB groups through typed controls and saves one group at a time through the generic config API. `/settings/backups` consumes the backup and scheduler APIs for readiness, manual operations, scheduled task triggers, recent evidence, and recovery-drill evidence once Task 9 registers the drill worker. Push/webhook fields are visible with the Phase 13b activation annotation and remain inert until notification dispatch exists.

**Resolution:** The admin UI renders config fields generically (it's a JSONB editor). Push/webhook fields display a subtle "Not yet active — activation requires the notification system" annotation until Phase 13b ships. When Phase 13b lands, the saved config takes effect immediately — no UI rework needed.

### `notification_cleanup` Scheduled Task

The Phase 2 seed migration includes a `notification_cleanup` scheduled task. This task deletes old notifications past their retention period. It should be registered as an executor in Phase 13a (it's a maintenance task) even though the notification dispatch system doesn't exist yet.

**Resolution:** Phase 13a Task 3 (scheduled task management) registers the `notification_cleanup` executor. It queries and deletes from the `notifications` table (which exists from Phase 2), removing expired rows and rows older than `config.max_age_days` (default 90). No dependency on Phase 13b's dispatch pipeline — cleanup is a DB operation, not a dispatch operation.

### Fluent Template Migration Timing

The Fluent template migration (altering `notification_types.in_app_template` from English strings to Fluent message IDs) is a schema migration. It must land in Phase 13b because:
- Before migration: templates are English strings like `'{{title}} was added to {{library}}'`
- After migration: templates are Fluent message IDs like `'new-media-added'`
- The migration is irreversible without data loss (once you replace the string with an ID, you need the Fluent file to resolve it back)

If Phase 13a ships before Phase 13b, the existing English-string templates continue to work (rendered as-is by the notification system's absence). Phase 13b's migration converts them to Fluent IDs.

### Phase 15 Docker Image Completeness

Phase 15 creates the Docker image. Ideally, the image includes all features (including notifications). But if Phase 13b hasn't shipped yet, the Docker image can still be built — the notification system simply isn't compiled in (feature-gated or not yet implemented).

**Resolution:** Phase 15 can start packaging work after Phase 13a + Phase 14. The Dockerfile compiles whatever is in the codebase at that point. If Phase 13b hasn't shipped, the image has system config + backup + maintenance + migration but no notifications. When Phase 13b ships, a new image build includes notifications. This is acceptable for pre-release Docker images; the v1.0 release image must include both 13a and 13b.

### If Phase 13b Proves Unnecessary

If, during Phase 13a development, the team decides the notification system is not needed for v1.0 (e.g., v1.0 ships without push notifications; in-app + SSE is sufficient for the initial release), Phase 13b can be deferred to post-v1.0 entirely. The split makes this decision clean — Phase 13a ships v1.0-critical system operations; Phase 13b is a post-v1.0 enhancement.

This is unlikely (notifications are a core media-server feature), but the split keeps the option open without structural rework.

## Revised Build Order Dependency Graph

```
Phase 12 (Overlays/Collections)
    ↓
Phase 13a (System Operations Core)     ← 10 tasks
    ↓                                   │
    ├── Phase 13b (Notification System) ← 6 tasks (can overlap with Phase 14)
    │       ↓
    ├── Phase 14 (Platform Migration)   ← proceeds after 13a, independent of 13b
    │       ↓
    ├── Pre-v1.0 Hardening              ← proceeds after 13a + 14; ideally after 13b
    │       ↓
    └── Phase 15 (Docker & Deployment)  ← needs 13a + 13b + 14 for complete v1.0 image
            ↓
        Phase 16a (Desktop & Mobile)    ← mobile push needs 13b
```

**Critical path to v1.0 Docker image:** Phase 12 → 13a → 14 → Pre-v1.0 Hardening → 15 (with 13b completing before or during Phase 15 finalization).

**Parallelism opportunity:** Phase 13b can overlap with Phase 14 if developer capacity allows. They have no cross-dependencies.

## Key Decisions

1. **Split Phase 13 into 13a + 13b — committed, not contingent** — Dependency analysis confirms a clean boundary. The notification system has zero cross-dependencies on backup/maintenance workers. Phase 14 doesn't need notifications. Keeping them as one phase unnecessarily serializes independent work.
2. **Phase 13a ships system operations (10 tasks)** — System config, scheduled tasks, backup, recovery drills, maintenance workers, admin settings UI. The operational backbone.
3. **Phase 13b ships notification system (6 tasks)** — Fluent templates, multi-channel dispatch (in-app + SSE + webhook), push device registration, notifications UI. The user-facing notification experience.
4. **Phase 14 proceeds after 13a without waiting for 13b** — Migration imports watch history; needs core media + auth + playback, not notifications or backup. This is the key unblocking benefit of the split.
5. **Admin settings UI (Task 10) in Phase 13a** — Renders ALL config fields generically (JSONB editor). Push/webhook fields visible but annotated "activation requires Phase 13b." No UI rework when 13b ships.
6. **MVP notification system fallback** — If Phase 13b takes longer than estimated, ship in-app + SSE + webhook (no mobile push client implementations). Defer FCM/APNs/UnifiedPush to Phase 16a. The `user_push_devices` table and API still ship to avoid Phase 16a schema migration.
7. **`notification_cleanup` executor in Phase 13a** — The scheduled task cleanup is a DB operation, not a dispatch operation. Registered in Phase 13a's scheduled task management alongside list/get/trigger/cancel/history endpoints.
8. **Phase 15 Docker image must include both 13a + 13b for v1.0** — Pre-release images can ship without 13b; v1.0 release image includes the complete feature set.

## Relationship to Other Documents

| Document | Relationship |
|---|---|
| [BUILD_ORDER.md](../../BUILD_ORDER.md) | Phase 13 formally split into Phase 13a + Phase 13b sections |
| [IMPLEMENTATION_DEBT.md](IMPLEMENTATION_DEBT.md) | Debt items #5-#8 assigned to Phase 13b; "Phase 13 Split Contingency" upgraded to decision |
| [I18N.md](I18N.md) | Fluent setup is Phase 13b Task 1 (forcing function for notification template migration) |
| [MOBILE_PUSH.md](MOBILE_PUSH.md) | Multi-channel dispatch is Phase 13b Task 2; push device API is Task 5; webhook dispatch is Task 4 |
| [REAL_TIME_PUSH.md](REAL_TIME_PUSH.md) | SSE EventBus (Phase 10) is a prerequisite for Phase 13b's SSE notification delivery |

## Research Sources

- **[Atlassian: Project Dependencies](https://www.atlassian.com/agile/project-management/project-management-dependencies)** — Dependency types (finish-to-start, start-to-start, etc.), dependency mapping, critical path analysis. The Phase 13 split is an application of critical-path optimization: breaking the longest dependent chain into shorter independent segments.
- **[Martin Fowler: Technical Debt Quadrant](https://martinfowler.com/bliki/TechnicalDebtQuadrant.html)** — Duskcue's strategic decisions are Deliberate + Prudent debt. The split ensures the debt paydown (Phase 13b) doesn't block unrelated features (backup, maintenance) in Phase 13a.
- **[Galorath: Project Dependencies](https://galorath.com/project/dependencies/)** — Critical path method: the longest sequence of dependent tasks determines the project timeline. Splitting independent clusters shortens the critical path.

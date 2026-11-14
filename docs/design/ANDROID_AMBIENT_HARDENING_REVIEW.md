# Android ambient ownership review — November 2026

## Scope and status

The November 14, 2026 continuation reviews the unqualified `e97833b` implementation recorded by `883cd4c`. The operator explicitly requested an Astra evaluation. Astra performed a read-only source review and official MCP research; the findings below are implementation defects and evidence gaps under the already-confirmed [ambient contract](VOD_ANDROID_AMBIENT_MIGRATION.md#complete-shared-contract--confirmed-at-8c8cb53). They do not authorize a new playback feature, dependency migration, weaker resource limit or production attachment.

The next item is to correct these defects and qualify the current increment before adding more platform work. Current Windows commit pressure still prevents guarded analysis/build/native admission. Source work proceeds under the operator's existing authorization; no completed qualification is claimed.

## Official sources and applicability

Sources were discovered/read using MCP on November 14. Version-specific service behavior is checked against Media3 1.10.1; living guidance is not represented as an archived November snapshot.

| Source | Application |
| --- | --- |
| [Media3 1.10.1 service source](https://github.com/androidx/media/blob/1.10.1/libraries/session/src/main/java/androidx/media3/session/MediaSessionService.java) | The `@CallSuper` start handler owns media-button/notification intent dispatch. Custom Start/Stop handling must preserve it. |
| [Android background playback](https://developer.android.com/media/media3/session/background-playback?authuser=48) | Background service ownership, task dismissal and foreground evidence are distinct from a Flutter screen or sampled position. Preserve no-resumption behavior after termination. |
| [PendingIntent](https://developer.android.com/reference/android/app/PendingIntent), [Service](https://developer.android.com/reference/android/app/Service) | Extras alone do not distinguish pending operations. Keep immutable Stop actions bound through intent identity as well as checked extras, cancel superseded tokens, and use the observed service start ID when stopping an idle service. |
| [Android CompletableFuture](https://developer.android.com/reference/kotlin/java/util/concurrent/CompletableFuture) | Non-async dependent actions may execute on the completing thread. Publication ordering and retained ownership must be explicit; a future becoming observable is not a join of arbitrary later callbacks. |
| [AtomicFile](https://developer.android.com/reference/androidx/core/util/AtomicFile) | Atomic replacement does not supply locking; keep serialized, retained journal I/O and verified read-back. This reference describes the compatibility implementation; production continues using its existing framework `android.util.AtomicFile`. |
| [W3C WCAG 2.2](https://www.w3.org/TR/WCAG22/), [WCAG2ICT](https://www.w3.org/TR/2025/NOTE-wcag2ict-22-20251211/) | Apply status/focus/control outcomes to native software using Flutter semantics. WCAG2ICT is informative guidance for non-web software. Do not claim native TalkBack, target-size or contrast conformance from source wiring or a single label test. |

## Astra findings and corrections

| Finding | Consequence | Correction and required evidence |
| --- | --- | --- |
| Dart coalesces a new scope behind an unresolved bind | Logout/profile/server changes can leave native output and old credentials unfenced while restoration waits | Separate immediate native fencing from retained asynchronous binding/restoration. Keep finite independent ownership, stale-revision refusal and no command replay. Hold the old bind while delivering a newer fence in tests. |
| A lane removes its pending marker before terminal state publication | A scope drain can clear recovery state before output failure/quarantine is recorded | Add an explicit synchronous terminal publisher before marker removal. Drain means ended/published, not successful. Check actual retirement outcomes for all tracked outputs, including outputs created after the first snapshot. Test a blocked publisher, concurrent fresh drain/admission and failed output cleanup. |
| Service omits `super.onStartCommand` | SDK media-button/notification intents are swallowed | Preserve Media3's handler and still return `START_NOT_STICKY`. Actual system/controller qualification remains required. |
| Explicit successful Play retains the old transient error | Current playback can resume but EOF advancement remains disabled | Clear the acknowledged recoverable error only after exact-current output attachment succeeds. Keep quarantine, lease uncertainty and progress evidence independent. |
| Task dismissal consults a one-second sample | An active channel can be withdrawn before its first sample or during buffering | Use current foreground, authorized lease, owner play intent and actual native player evidence. Preparing/paused work gains no automatic resumption. Qualify actual dismissal and buffering behavior. |
| Logout can await an older fence acknowledgement | A newer null-scope fence can still be pending when logout reports success | Wait for the latest coalesced fence under one five-second observer; a stale revision or failed acknowledgement refuses success. Native cleanup remains separately owned. |
| A stale Stop token can alias a newer notification or a restarted process | Old notification actions can cancel unrelated new work or recreate an idle foreground service | Bind immutable pending operations to a process nonce, owner epoch and pending ticket in both intent data and checked extras. Cancel superseded tokens; a rejected action may stop only an idle service without clearing its retained journal/barrier. Durable entity and command identities remain UUIDv7. |
| An old player view can be reused after scope changes | A new session's output can attach to an old view | Detach and forget the view on fencing, bind new views to exact current native identity and require confirmed scope-bound status before rendering the Flutter platform view. |

The existing seven-stage companion is not complete acceptance. It still needs real accepted-response-loss reconciliation through the owner, notification/headset/untrusted-controller cases, failed-cleanup advancement, pending storage/release scope changes, capacity/refusal/revocation and the remaining HTTP/TLS/HLS faults. Preserve the original twelve journeys, worker guard, exact history sentinel and unchanged shared deadlines.

## Alternatives, pros and cons

| Approach | Pros | Cons | Decision |
| --- | --- | --- | --- |
| Correct the modular native owner on the locked stack | Fits the confirmed background lifetime; isolates concrete defects; reuses existing backend authority | Requires explicit native protocol, ownership and adversarial evidence | Recommended |
| Serialize fencing behind restoration | Fewer concurrent bridge operations | Lets old output/credentials survive a newer scope; contradicts the contract | Reject |
| Move one future completion without an explicit publication boundary | Small diff | Fresh drains or inline callbacks can still race; can break journal chaining or self-wait | Reject |
| Introduce a background Flutter engine or new media package | Potential code reuse | Adds another engine/dependency lifetime and does not resolve these native defects | Defer |
| Replace public HTTP with a new raw-socket parser | Potentially stronger transport visibility | Adds HTTP/TLS parsing and cancellation risk outside the accepted guarantee | Defer |

Recommendation stack: existing Rust/PostgreSQL protocol two; locked Kotlin/JDK 17 and Media3 1.10.1; bounded native HTTP/HLS and joined public owners; a small Flutter bridge with separate fencing/restoration; retained atomic non-secret journal; current-source static checks followed by real native/backend fault qualification when the unchanged guard admits it. Keep private SDK housekeeping exclusions, all budgets, default-off production attachment and no release.

## Outcome tracking

Unqualified source checkpoint `562145f5d818d9543ce3ccb6352822480a7caaf3` implements the corrections above within the confirmed contract. Documentation-only successors preserve that attribution. `Operations.kt` now owns terminal publication before releasing a lane marker. Critical command, renewal, checkpoint, restoration and output callbacks use that boundary. Publication cannot wait on its own lane. Scope reconciliation waits for retained work, retires tracked outputs sequentially, rechecks the latest scope epoch and completes its externally chained promise after its cleanup operation has retired. A failed output keeps quarantine; an observer timeout does not erase ownership.

The Flutter bridge separates prompt fencing from slow restoration. A bind after initialization is identity-read-only, and unchanged wrapper recreation preserves background playback. A newer scope coalesces to the latest fence while admission remains closed. Added Dart cases hold old binds and fence acknowledgements, cover account/profile/server/logout changes and check wrapper recreation and latest-fence logout settlement. Qualification-only native probes hold successful and failed terminal publishers, check publication errors and drain/admission ordering, exercise superseded native binds/fences, and compare actual system PendingIntent identities across epochs, tickets and simulated process incarnations. These probes have not compiled or run at the corrected source.

Keep the cold journal boundary precise: a non-secret plaintext receipt that has not yet been authenticated/adopted is unknown recovery evidence. If logout fences an initial restore before adoption, retain that evidence until a later authenticated context can reconcile it or establish mismatch; never send old-credential requests to classify it. An adopted recovery scope still clears on a stronger boundary after owned work settles. Do not erase an unadopted receipt merely because `runtime` is null. A held-restore → logout → later authenticated-bind case remains part of native qualification.

Astra's final targeted source pass found no additional definite blocker. This is read-only review, not a compiler pass, security certification or runtime evidence. Earlier fifteen-test Dart and 47-test host passes belong to the earlier implementation; they do not validate these corrections. Current analysis and focused-test attempts were refused before execution by the unchanged host memory guard. [Qualification](../ci/ANDROID_AMBIENT_QUALIFICATION.md#november-14-astra-corrections--unqualified) records exact receipts and outstanding gates; [memory attribution](../ci/QUALIFICATION_MEMORY_ATTRIBUTION.md#november-14-ambient-review-admission) distinguishes commit headroom from physical RAM.

The only open PR was Dependabot #6. Its existing branch was rebased once and verified without merging; the unchanged, previously locally failed major migration remains unadopted. [PR review](../ci/SVELTEKIT_3_UPGRADE_REVIEW.md#november-14-astra-continuation) records its immutable head and applicability decision. No SDK/dependency, OS, service or resource-policy change accompanies these corrections.

Next: run current-source formatting, focused/full tests, analysis and both APK checks under the unchanged guard, correct any compiler/runtime failures, then complete the real native/backend fault matrix while preserving the original twelve journeys. Keep production attachment/caps off until the migration and later all-client rollout gates are met. Recording this source checkpoint on main is not phase completion or a release.

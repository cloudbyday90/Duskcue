# Android ambient playback qualification

## Status

Confirmed acceptance contract, November 8, 2026. [Design](../design/VOD_ANDROID_AMBIENT_MIGRATION.md#complete-shared-contract--confirmed-at-8c8cb53) records Q1–Q9 and the complete implementation contract confirmed by the operator. [Security](../security/ANDROID_AMBIENT_TRANSPORT.md) owns native transport limits and the accepted public-completion guarantee, including private audio/codec housekeeping exclusions. Implementation is in progress; current-source compilation and runtime qualification remain pending. The operator permits qualification to wait for host memory. Earlier interactive/relay passes do not qualify native ambient.

## Preserved infrastructure

Reuse the locked Flutter/Dart/Kotlin/Media3/JDK/Gradle stack, already-qualified public SDK driver/bootstrap, exact package/artifact checks, private ADB owner and existing real PostgreSQL/HTTP/FFmpeg fixture. Direct HLS imports add only an explicit 1.10.1 declaration for the already-resolved module. No new dependency version, SDK/OS change, default-device operation or service interruption is authorized by this design.

Keep one isolated API 36 emulator, two CPUs and the approved 2,560-MiB guest RAM. Preserve three-minute driver results, 240-second stages, sixteen-MiB stage evidence and the shared thirty-minute native deadline. Heavy work stays serial through the guarded workflow. Preserve current host memory admission/emergency rules and backend SQL/process/I/O/root/pool budgets. Do not extend deadlines, lower reserves or repeat mutations to obtain a pass.

Keep all twelve original Android interactive/scope/relay journeys and their assertions unchanged, plus the standalone worker guard. Add separately attributed ambient companion journeys using fresh fixture identity and exact expected counts. If the complete native suite cannot fit the existing shared deadline, report that as unqualified; do not weaken an assertion or silently split acceptance into a fresh budget.

## Host and isolated native tests

| Area | Required meaningful evidence |
| --- | --- |
| Native command owner | UUIDv7 server-time identity, complete immutable Start selection, no duplicate/revived admission, exact ETags and context, lost Start/seek/renew/Stop observations, conclusive withdrawal, no downgrade/replay |
| Scheduling | Independent renewal during preparation/pause/control/background; finite permits, status/read coalescing, no unbounded executor queue, retention after observer timeout |
| Queue | Successful natural completion releases old native/server work before next; stale selection's bounded definite-refusal path; exhausted quota, absent next, failed cleanup and failed/new channel replacement stop admission truthfully |
| Journal | First use, strict 4-KiB/UTF-8/schema bound, interrupted/failed write and clear, actual cold process restart, corruption refusal, no-backup placement and namespace separation from the interactive encrypted journal |
| Scope | Account/server/session/credential/profile change during journal, selection, Start, seek, media open/read and native release; late results cannot adopt/stop newer work; profile-only cleanup differs from stronger-boundary revocation |
| Public HTTP | Exact-origin/path before bearer dispatch; all redirects refused; valid/invalid/name-mismatched TLS; failed connect/handshake/read/close; identity coding; bounded JSON/errors/media; MIME/range/framing/truncation and deadline limits |
| HLS | Exact generated subset, unsupported/mixed tags, duplicate URI attributes, external/key/resource attacks, byte/reference bounds and atomic epoch/revision allowlist publication |
| Player/worker | Release-timeout and late loader completion retain barriers; public request/stream tasks and owned loader/playback workers settle; private SDK pooling/housekeeping limits remain explicit |
| Controllers | Unknown-controller refusal; app/system allowlist; Play/Pause/Stop/seek enter the same owner; supplied URI/items and direct seek bypass unavailable; stale notification actions fenced |
| Flutter UI | Single-flight bounded status, truthful acceptance/stopping/uncertainty, permitted buttons, navigation without false cleanup claims, semantics/focus, large text/RTL/narrow layout |

Do not claim a complete transport boundary from regex/source-presence assertions. Update legacy native contract verification to distinguish preserved fallback from the new lane and add behavioral tests of actual owners. Native public cleanup tests must not substitute a returned disconnect/release/listener for joined owned work.

## Actual Android companion journeys

1. **First start and background:** install the actual qualification APK; create a real ambient channel/local item; explicitly negotiate protocol two; prove decoded/advancing native output and the foreground media session. Send real Home/lock and controlled task dismissal, then verify renewal/activity with Flutter screen absent. No saved credentials or automatic resumption.
2. **Pause and same-viewer seek:** exercise app and system media controls, verify renewal while paused, exact configuration/output-bound seek, confirmed old-output cleanup and paused intent. Explicit Play reattaches only confirmed current output.
3. **Natural advancement and channel replacement:** author short finite media; observe exact old viewer settlement/local worker drain before next Start, with one ambient viewer at a time. Confirm new channel replacement obeys the same gate; inject failed cleanup and prove no next Start.
4. **Uncertain outcome and restart:** discard complete accepted responses at the fixed-origin gateway; verify one mutation, bounded monitoring and explicit status/withdrawal. Force-stop and observe exact process death, cold launch/authenticate, then reconcile/Stop the same journal without automatic playback. Include a genuine missed lease.
5. **Scope and cancellation:** use real provider wiring for profile-only and stronger account/server/session changes while work is pending. Verify immediate output fence, cleanup-only authority, retained local barriers across recreation and no new old-secret request dispatched after stronger revocation. Already transmitted bytes remain outside that fence's recall guarantee.
6. **Independent concurrency:** run one interactive and one ambient session together. Prove distinct Start/viewer identities, renewal and cleanup; stopping/failing ambient must not implicitly Stop interactive. Audio focus may pause/duck independently of counted slots. Enforce exact one interactive history versus zero ambient personal-history writes.

Bind every journey to verified source/tool/runtime/APK/executable hashes, exact fixture scope, witness events and actual result. Use real generated H.264/AAC stereo SDR output; retain probe/frame evidence without inferring physical-device/speaker proof. Native HTTP and HTTPS need separately attributed actual-Android evidence; a host JVM TLS pass does not establish Android platform trust behavior. Fixture CA/trust/controls remain qualification-only.

## Database and resource assertions

Derive exact expected viewers/operations from each frozen journey rather than accepting aggregate lower bounds. All accepted Starts must settle, no duplicate/revived request may create work, and no server output/FFmpeg/preparation obligation may be silently released. Ambient adds operational play-session diagnostics but zero personal history/resume/play-count/recommendation/Trakt mutations. Seed a personal-history sentinel and prove its full identity/value stays unchanged; the concurrency journey attributes interactive history separately.

Assert permission/channel-revision/membership/source revocation with actual PostgreSQL transactions and current output refusal. Preserve existing cap-only reduction versus true revocation behavior. Capacity/refusal tests distinguish viewer, transcode-viewer, encoder, client-operation and receipt backpressure. Production-wide enforcement remains off.

## Completion, maintenance and limitations

Pass native host tests, Flutter analysis/tests, both verified APK/package checks, relevant backend/tooling regressions, formatting, legal headers, link and privacy checks. Record unrelated baseline diagnostics separately; do not suppress them. No source input changes during a frozen qualification run. An interrupted attempt remains failed and is never relabeled by a later pass.

Observe exact app/driver/log/forward/ADB/emulator/backend/FFmpeg/HTTP/PostgreSQL cleanup. Capture memory attribution without assigning shared/system growth to an actor unsupported by evidence. Use the [repository-cleanup skill](../../.agents/skills/duskcue-repository-cleanup/SKILL.md) to remove newly owned AVD disks after exact process/listener completion, preserving current compiler/SDK/APK checkpoints and compact results. Do not touch private Dispatcharr fixtures, application volumes, unrelated services or the earlier policy-blocked TEMP fixture.

Update this authority with actual results, failed attempts, hashes, cleanup and remaining limitations before marking the increment qualified. Emulator passes do not establish physical-device battery/thermal/audio/TalkBack results. iOS/offline, direct/copied video, broad formats, provider/DVR, track/quality pickers and automatic ambient recovery remain separate. No release or production attachment/shared-cap enablement.

## Implementation and verification checkpoint

Source checkpoint `e97833b` on main records the unqualified draft. Documentation-only successors preserve that implementation attribution. Do not treat the checkpoint commit as a native qualification result or completed migration.

The new `ambient` companion is isolated from the original twelve journeys and their SQL assertions. Seven native stages represent background, controls, advancement/replacement, cold retain, cold restore, scope and concurrency. `android-playback` includes this companion after the existing worker/scope/base/relay groups under the same shared deadline; `android-ambient` is a focused diagnostic, not a substitute for complete acceptance. Expected companion attribution is nine ambient viewers, one independent interactive viewer, ten released reservations/operational sessions, one seek, zero unsettled Starts, one new interactive personal-history row and an unchanged full ambient history sentinel. These counts are frozen draft assertions, not observed evidence.

Qualification-only native probes exercise strict JSON/UTF-8/size rules, UUIDv7 time fencing, atomic HLS publication, retained lane timeout, joined loader termination, atomic journal corruption/write failures, actual platform HTTP framing/redirect/lost-POST/cancellation, injected public-I/O/late-handle/failed-close races and normal platform TLS/name rejection. The public fixture CA is a qualification-flavor resource only; normal TLS validation remains enabled. Dart fixture requests separately trust that same test root without a certificate callback bypass. Production artifact exclusion still requires a fresh verified APK check.

Observed attempts, retained privately in `.cache`:

| Attempt | Actual outcome |
| --- | --- |
| `2026-11-09T00-31-18-896Z-b56fc448-18a9-4057-b7e7-c97635ff6b8e.jsonl` | Preliminary APK compiled before the later ownership/harness changes. Source hash `905cb9d723048f553495c125ad6fbca1706f25407f35f9a7495d7ef361ea5f8e`; this artifact is stale for current inputs. |
| `2026-11-09T00-43-00-588Z-38b1c145-83ea-4380-9428-f4b1c48e5fdf.jsonl` | Eight focused Flutter tests passed at that source checkpoint. Later changes require fresh tests. |
| `2026-11-09T00-52-25-866Z-e3aa3e91-dc9f-4799-b4a4-aff85ba3ae0c.jsonl` | APK rebuild refused before compilation by the unchanged memory admission policy. |
| `2026-11-09T01-03-29-302Z-0f05f1a4-8c6f-4700-9e71-c8f8867a3598.jsonl` | Formatting refused before execution by the unchanged check policy. |
| `2026-11-09T01-03-37-089Z-4ce52390-9675-4c34-b0c2-b4a46bef6687.jsonl` | Full Flutter regression interrupted at 97.259% system commit. Fifty-nine tests had passed; no complete-suite pass is claimed. |
| `2026-11-09T01-07-11-529Z-5a725117-7b9a-47a9-9161-f8d043fa5fed.jsonl` | Forty-seven host tooling tests passed with two existing Linux skips, including real host TLS/channel confinement. Later phase-wiring changes require repetition. |
| `2026-11-09T01-28-55-589Z-ea448a2e-1360-452e-b58a-79ff070a54f1.jsonl` | Fifteen current Dart ambient tests pass: exact binding/control scope, coalesced reads, concurrent Stop, stale bind/result refusal, truthful cleanup, RTL focus, large text/semantics and existing legacy bridge behaviors. This does not compile the new integration/native files. |
| `2026-11-09T01-29-25-434Z-b81dbd13-16d8-4929-8f15-a96d9230917c.jsonl` | Current host tooling repeat passes 47 tests with two existing Linux skips, including phase rejection and real host TLS/channel confinement. |
| `2026-11-09T01-29-37-128Z-a0a9c584-7ab1-43af-bd63-f4cee5a3ae2f.jsonl` | Current formatting remains refused before execution at 75.15/86.84 GiB commit. Guard limits are unchanged. |

No actual ambient emulator journey has run. Current-source formatting, analysis, both APKs, native compilation/probes, complete Flutter regression, real PostgreSQL/native HTTP/TLS/FFmpeg journeys and exact cleanup remain required. Complete the accepted-response-loss, stale-selection/refusal/capacity, revocation, controller/headset/task-dismissal, pending-scope/storage/release and full transport adversarial matrix; the current draft does not yet establish those gates. Do not turn host checks or source assertions into a native pass. See [memory attribution](QUALIFICATION_MEMORY_ATTRIBUTION.md#november-8-ambient-source-work) for the interrupted attempt.

The source checkpoint is deliberately unqualified. Source-wiring verification, staged copyright compliance (1,070 files), changed host JavaScript syntax, whitespace and private-LAN/cache exclusion checks pass. They do not substitute for the pending compiler/formatter/analysis/native gates. [Maintenance](../operations/DEVELOPMENT_STORAGE.md#ambient-source-work-maintenance--november-8) retains current reusable artifacts and compact evidence without creating/deleting a VM or interrupting a service. Commit/push records the work in progress on main; it is not a release or a completed migration.

## November 14 Astra corrections — unqualified

Source checkpoint `562145f5d818d9543ce3ccb6352822480a7caaf3` records these corrections on main. Documentation-only successors do not change the unqualified source or turn earlier checks into current behavioral evidence.

The [requested independent review](../design/ANDROID_AMBIENT_HARDENING_REVIEW.md) finds ownership/publication, slow-bind fencing, service-intent, recoverable-error and sampled task-dismissal defects in the preceding draft. Follow-up source review also identifies latest-fence logout, stale notification identity and old-view adoption gaps. Corrections remain within the confirmed contract and locked stack. Astra's final targeted pass finds no additional definite source blocker; no compiler, native or security pass is inferred from that assessment.

New Flutter regression cases hold an old bind across account/profile/server/logout changes, hold successive fence acknowledgements, preserve unchanged-wrapper background behavior and wait for the latest null-scope logout fence. Qualification-only `AmbientPublicationProbe` holds successful and failed terminal publishers while testing drain, timeout, admission and original-error preservation; it also exercises native read-only binds and superseded scope reconciliation in an isolated no-backup directory. Native notification probes compare actual immutable PendingIntent identities for different epochs, tickets and process incarnations, without sending a service action. They do not qualify actual notification/headset, task-dismissal or hostile-controller delivery. Integration assertions require the added probe flags without changing the original twelve journeys or shared deadlines.

Current-source attempts were all refused before execution:

| Receipt retained privately in `.cache/testing-memory` | Actual outcome |
| --- | --- |
| `2026-11-14T21-23-47-659Z-ff259f10-ab20-4ef9-b60a-291e8bc67c0f.jsonl` | Analysis preflight refused on commit headroom and percentage. No analyzer execution. |
| `2026-11-14T21-52-07-464Z-90bdbd0a-ebf8-4347-b928-75340a258072.jsonl` | Focused Flutter preflight refused at the unit commit ceiling. No tests executed. Subsequent source corrections are also untested. |
| `2026-11-14T22-07-24-793Z-34526be9-aa0f-4025-8971-7f46d3897797.jsonl` | Final corrected-source focused preflight refused at **97.005%** commit: **71.73/73.95 GiB**, about **2.21 GiB** commit headroom despite **20.13 GiB** available physical RAM. No tests executed. |

No APK, emulator, PostgreSQL fixture or native probe is run during this continuation. Prior source/check receipts remain historical evidence, not current-source passes. Current formatting, analysis, focused/full Flutter tests, both APK/package checks and native probe compilation/execution remain required. Then complete accepted-response-loss reconciliation through the real owner, controller/headset/task dismissal, failed-cleanup advancement, pending storage/release scope changes, capacity/refusal/revocation and the full transport fault matrix against actual PostgreSQL/HTTP/TLS/FFmpeg. Include held cold restoration → logout → later authenticated bind: retain unadopted non-secret evidence until current context establishes ownership/mismatch, while clearing adopted stronger-scope recovery only after owned work settles. Preserve exact viewer/history assertions and original journeys. The phase stays incomplete and production attachment/caps stay off.

The operator permits qualification to wait; no host app, model or service is stopped and no guard, resource/deadline, SDK/dependency or OS setting changes. [Memory attribution](QUALIFICATION_MEMORY_ATTRIBUTION.md#november-14-ambient-review-admission) and [repository maintenance](../operations/DEVELOPMENT_STORAGE.md#ambient-review-maintenance--november-14) record the bounded continuation. Commit/push is an unqualified source checkpoint on main, with no release.

The existing legacy/command-service source-wiring verifier and staged whitespace check pass at this continuation. Copyright compliance checks 1,070 files; a separate narrow check verifies legal headers on all fourteen changed Kotlin files and twenty-one added local Markdown links/anchors. Staged paths exclude private/cache evidence and the added diff contains no private LAN value. These are text/metadata checks, not behavioral tests; none replaces the refused compiler/test gates.

# Android ambient playback qualification

## Status

Proposed acceptance contract, November 8, 2026. [Design](../design/VOD_ANDROID_AMBIENT_MIGRATION.md) records confirmed Q1–Q8 and proposed implementation. [Security](../security/ANDROID_AMBIENT_TRANSPORT.md) owns native transport limits and the accepted public-completion guarantee. Complete-contract confirmation, code and runtime evidence remain pending. Earlier interactive/relay passes do not qualify native ambient.

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

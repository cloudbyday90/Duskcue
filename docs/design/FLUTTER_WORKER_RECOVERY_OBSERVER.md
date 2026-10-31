# Flutter positive worker-evidence observation

## Scope and review state

This increment addresses the genuinely reproduced [Android worker-recovery failure](../ci/ANDROID_WORKER_RECOVERY.md#current-native-result-and-next-design-decision), following repository maintenance at `501dd4e`/`a577e66`. Work stays on main, with no release. The operator confirmed both behavior choices and the complete implementation/qualification contract below. The scoped observer is implemented and qualified, including the genuine eight-stage native repeat and cleanup recorded below. Remaining Android scope/I/O/relay gates keep the overall migration open.

Keep normal production attachment/caps, native ambient, iOS, offline changes, decoder downgrade, broader formats and the remaining Android scope/I/O/relay matrix separate. Existing Flutter 3.47.8/Dart 3.13.7, locked dependencies, backend watchdog, recovery eligibility/budget, lease, control, native, relay, VM and root limits stay unchanged. Reuse the approved standard legal headers without explanatory code comments.

## Evidence and official research — October 31, 2026

URLs were discovered and read through MCP search. Existing source and the real native result establish one generic-error callback per native generation, one immediate configuration read and no later eligibility observation. The run confirms a 503-ms native checkpoint, genuine Stalled evidence and physical encoder exit, but ends `media_unavailable`. Exact ordering of the negative client read and server failure was not timestamped; the timing explanation remains an inference to qualify, not a manufactured failure label.

[Media3's HLS tracker](https://developer.android.com/reference/androidx/media3/exoplayer/hls/playlist/DefaultHlsPlaylistTracker?authuser=44) uses a stuck-playlist threshold based on target duration, independently of the server watchdog. A native error can justify checking authoritative server evidence; it cannot establish worker or decoder eligibility itself.

[Dart Future.timeout](https://api.dart.dev/dart-async/Future/timeout.html) ends observation without stopping the source future. Retain actual read completion after cancellation/deadline, and fence every late result. [Dio's cancellation guidance](https://pub.dev/documentation/dio/latest/) explains that a shared token cancels every request using it; use a per-read owner and leave renewal/checkpoints independent. The existing transport-wide cancellation remains reserved for its existing scope/Close/disposal behavior.

[Dart Timer.periodic](https://api.dart.dev/dart-async/Timer/Timer.periodic.html) does not guarantee precise spacing between callbacks. Use the existing injected [monotonic clock](https://api.dart.dev/dart-core/Stopwatch/start.html), a checked deadline and sequential pacing. Never issue catch-up bursts after suspension. [Flutter lifecycle guidance](https://api.flutter.dev/flutter/widgets/State/setState.html) favors cancelling work that could notify disposed UI, alongside ownership checks across asynchronous gaps.

[W3C status-message guidance](https://www.w3.org/WAI/WCAG22/Understanding/status-messages.html?trk=article-ssr-frontend-pulse_little-text-block) supports programmatically identified waiting/result status without stealing focus and warns against overly chatty live updates. Use the existing AppStrings/Flutter live-region surface for phase transitions, not a once-per-second announcement. Widget/semantics evidence does not establish spoken TalkBack or physical-device fidelity.

## Agreed behavior

1. A current native error may initiate one observer for that exact signed-in origin/account/session/credential/profile context, Start UUID and output revision. Capture the command epoch as well as current scope; a matching account or title alone is insufficient. Permit at most one retained observer, with no queue or automatic replacement observer.
2. Observe for at most sixty monotonic seconds from admission, issuing no more than one configuration GET per second. Continue only successful, validated matching-output negatives in `ready`/`unavailable` with no pending operation. Refuse changed output, uncertain/changing/retiring evidence, malformed data, failed/refused reads and loss of current authority. Clamp each read to the remaining observation budget and existing five-second maximum; reject positive results received after the deadline.
3. Cancel on Close, disposal, background/explicit-Play requirement, account/server/session/profile/credential transition, output change, competing control/status work, or uncertain/expired lease. Cancellation fences state publication and requests cancellation only of this observer's read. A timer, abandoned widget or disconnected observer never frees an unconfirmed HTTP permit. Hold the single observer slot until its actual issued read finishes; preserve the existing three regular plus two cleanup permits and 32-KiB JSON bounds.
4. Detach failed native/relay delivery immediately. Observe existing native cleanup within its existing limits and retain/quarantine unconfirmed completion. Never dispatch replacement work or attach another player across unconfirmed native cleanup. The observer does not extend the native-only thirty-second attachment observer or other five-second control/cleanup observations.
5. Show “Checking playback status” while waiting. Keep Pause and Close reachable, disable Play/seek/tracks/quality and competing status actions. Pause changes retained play intent without cancelling evidence observation; a successful replacement remains paused. Close always cancels and uses normal conclusive withdrawal/cleanup. Repeated negative reads do not steal focus or repeatedly announce the same status.
6. Positive current server evidence plus a reconciled output-bound checkpoint permits one fresh conditional Recover attempt through the existing command implementation. Re-read authoritative configuration/ETag, reconcile progress, journal the fresh operation UUID and preserve existing server admission. A positive observation is not guaranteed acceptance. Never replay a refused/uncertain Recover, infer decoder proof, issue another Start or revive a retired UUID. The server's existing replacement budget remains authoritative across outputs.
7. Preserve viewer/history, checkpoint source position, negotiated tracks/quality and explicit play intent. Continue lease renewal independently through the wait and accepted recovery while Dart can run. The observation deadline creates neither a new lease nor server grace. Accepted Recover preparation/status and native attachment retain their existing separate lifecycle limits.
8. If observation ends without recovery, leave failed delivery detached and offer explicit status, settings correction or Close. Require explicit Play after successful reconciliation before reattaching that failed source; a status read does not automatically launch another observer or resume known failed media. Background still requires status plus explicit Play. A late cancellation/failure cannot overwrite a newer viewer or cleanup state.

## Modular implementation

Compose a focused evidence observer with injected time, wait, read and current-context checks through a small command-controller part. Keep private command epoch ownership in the command service, native/background/play-intent ownership in the coordinator and HTTP ownership in transport/retained services. Do not turn the coordinator or screen into a polling singleton.

Expose scoped read cancellation and actual completion from the existing retained-operation/transport implementation, preserving the current `run`/`call` behavior for existing callers. Validate configuration through one shared parser. The observer must not call transport-wide cancellation merely to stop waiting. Wire the visible recovery state through AppStrings and the existing screen semantics/control gate. Preserve independent ambient, legacy and offline lifetimes.

| Approach | Pros | Cons / decision |
| --- | --- | --- |
| Retain one immediate read and require manual correction | Smallest state machine; least traffic | Misses later authoritative worker eligibility; does not meet the chosen automatic behavior |
| Bound positive-evidence observation with exact ownership | Bridges independent native/server timing; preserves authority and current command path | Adds cancellation, pacing, retained-completion and UI-state obligations; selected |
| Retry native playback or issue Recover from generic error text | May appear to resume sooner | Lacks authoritative eligibility, risks wrong-source work/replay and invents decoder proof; refused |
| Add a new retry/network package | Could supply generic scheduling helpers | Does not solve application ownership/proof and adds dependency surface; unnecessary |

Recommendation stack: locked Flutter/Dart/video_player/Dio; small injected monotonic evidence observer; existing retained transport, checkpoint and conditional command services; existing bearer-free loopback relay; real Android/PostgreSQL/HTTP/FFmpeg qualification. Add no dependency or production route and relax no trust/resource limit.

## Qualification and completion criteria

Meaningful deterministic/loopback tests must cover successful false-to-true eligibility with one mutation; maximum read spacing/count; deadline clamping and a late positive result; malformed/terminal/changed-output evidence; read refusal/network uncertainty; cancellation during wait and in-flight I/O; retained completion/permit refusal; independent renewal; Close, new viewer, scope/profile/credential/background transitions and competing work; checkpoint failure; uncertain conditional mutation without replay; native cleanup failure/late completion; Pause intent and ready/failed UI/control/semantics states. Use existing injected clocks, loopback CommandFixture and delayed native player seams rather than production fault routes.

Run the full Flutter unit suite, analysis, format and relevant host checks under the existing managed serial workflow. Build/verify both APKs. Rebuild removed Rust fixtures under the unchanged memory guard when needed; the repository cleanup invalidated availability of historical server executables without erasing their earlier test evidence. Reuse cached dependencies without trusting absent or changed artifacts.

The required real eight-stage run must first pass the independent Windows owned-encoder guard, then all established native stages and the genuine worker journey. Require at least one validated negative eligibility read followed by positive real-backend evidence, an actual native checkpoint before watchdog retirement, real Stalled/physical exit, one completed automatic Recover, fresh output revision, the same viewer/history/configuration and confirmed app/native/relay/server/VM/database cleanup. Preserve existing five-viewer/history SQL attribution and zero unsettled Starts. Unit/widget tests additionally qualify Pause during evidence observation; the real worker journey retains its existing unpaused automatic-recovery proof. No shorter watchdog, fabricated native error/position/proof, bypass route or disabled resource guard may make the test pass.

After the owning run drains, apply the [repository-cleanup skill](../../.agents/skills/duskcue-repository-cleanup/SKILL.md) to the new stopped per-run VM disks and stale task artifacts, retaining source/tool hashes, logs/results and useful current compiler/APK inputs. Do not repeat the earlier session's service-outage approval. Record exact passing/failing/held attempts and before/after storage separately; no native gate is complete until genuine qualification passes.

## PR and next boundary

GitHub MCP identified only Dependabot PR [#6](https://github.com/cloudbyday90/Duskcue/pull/6). Direct Git comparison found it ten commits behind main; the connector lacks branch-update capability, so the authorized CLI rebase completed. Verified head `65f513c08610c1ae480d3a8d6e36798e833da4ea` is zero behind / one ahead of `a577e66`, and its dependency files are unchanged from the documented [local configuration failure](../ci/SVELTEKIT_3_UPGRADE_REVIEW.md#october-30-retrial). Relevant web/desktop inputs remain unchanged. Reuse that exact applicability evidence instead of reinstalling the same incompatible graph. The PR remains open and unmerged; its dependency-only patch is not adopted. A coordinated SvelteKit migration remains separate.

After this gate, complete the pending Android scope/control/read cancellation and remaining relay adversarial/backpressure/output-format evidence before native ambient migration. Normal production advertisement/attachment/caps remain disabled.

## Implementation and verification

The implementation composes a bounded `WorkerEvidenceObserver`, command-controller recovery part, retained per-read transport handle and shared configuration parser. The coordinator binds native failures to their exact output, detaches delivery, preserves Pause intent and fences background/Close/new-context completion. A refused or uncertain Recover also requires explicit Play after reconciliation, including checkpoint refusal before any mutation. The earlier unused immediate-read recovery helper is removed. Screen status uses the existing live region; Pause and Close remain reachable while competing controls are disabled. No dependency, backend rule, production advertisement or resource budget changed.

| Check | Outcome / ignored receipt |
| --- | --- |
| Final Flutter unit/HTTP/native-seam/widget suite | 94 pass; `2026-10-31T23-00-55-848Z-18141521-d7cb-40d6-b6b1-a5464bcb4248.jsonl` |
| Flutter analysis | No issues; `2026-10-31T23-02-37-223Z-d69a98b3-35fb-4807-95e9-d229ec240505.jsonl` |
| Final changed Dart source/test format | Nineteen files, zero changes; `2026-10-31T23-30-13-133Z-8802af3f-8a93-4c93-88e6-ee17f6393bf2.jsonl` |
| Host tooling | 31 pass, two Linux-only skips; `2026-10-31T22-57-13-976Z-35abc2f1-66e4-4ea1-8636-deea04ae96cf.jsonl` |
| Current qualification APK | Verified package/source/tool identity; `2026-10-31T23-02-46-867Z-a3e6ca53-9bff-4dec-8836-75304151375b.jsonl` |
| Current normal APK | Verified package/source/tool identity; `2026-10-31T23-03-39-966Z-f115887c-a45c-451e-bf09-3387704d8a49.jsonl` |

Tests cover paced negatives/positive proof, sixty reads and final timeout clamping, late positives, retained cancelled completion/capacity, independent in-flight renewal, malformed/oversized/refused/changed-output evidence, missing checkpoint, uncertain Recover without replay, credential/profile-context/background/Close fencing, late old-viewer completion after a fresh Start, real loopback HTTP, delayed native cleanup, paused replacement and large-text narrow RTL status/controls. Widget/native seams do not establish actual emulator decoding or spoken TalkBack.

The first extended focused run had 26 passes and one failed test assertion: credential invalidation correctly removed the record and entered `scope_changed`, whereas the test expected an error object. That assertion was corrected; no invalidation rule was weakened. Initial analysis found three test brace diagnostics and one unused test import, corrected before the clean final analysis. Early format/analysis attempts were held by the existing Windows commit policy, despite available physical RAM. No guard was relaxed and no unrelated process/service was stopped. Genuine APK/native qualification and post-run storage outcomes are recorded below when completed.

### Interrupted first native attempt

Resource `a31e5a71-54e3-474c-a322-ab4a21dedc63`, receipt `2026-10-31T23-04-44-723Z-21620dfa-2343-46b7-b23b-0794b0b0f7ae.jsonl`: Cargo rebuilt the cleaned test package in 3m42s while reusing dependencies. The independent 47-thread guard passed real Stalled/physical-exit/cleanup proof; all seven established native stages passed. During the final worker stage Windows commit reached 94.0357%, triggering the unchanged emergency guard. Available physical RAM was still 17.34 GiB; global commit, rather than free RAM, stopped the run. No worker-recovery pass or final SQL attribution is claimed.

The wrapper terminated its owned process tree and removed its one PostgreSQL container. App-level graceful cleanup and native final status were unconfirmed because the process was interrupted. A separate post-interruption inspection confirmed zero owned encoder/test-server/emulator/ADB processes and zero owned VM listeners; the original owner record is preserved rather than rewritten as graceful success. The installed manager identified the exact AVD name/path despite a system-image metadata-rendering error, then deleted that named AVD successfully. The 4.294-GiB logical disk and its private app state were removed; logs/results, current APKs, SDK/Gradle inputs and rebuilt compiler checkpoint remain. The observed host free-space increase during that cleanup was 4.234 GiB, separately measured from logical size. No application data, unrelated process/service or shared Docker cache was removed.

After headroom recovered, one fresh unchanged guarded repeat began at resource `6ced6dff-e301-428b-8bfd-392933ff9d17`. The operator also agreed to free unused host memory for that retry. Its passing result below remains separate from this interrupted attempt.

### Passing real native repeat and final boundary

Resource `6ced6dff-e301-428b-8bfd-392933ff9d17`, receipt `2026-10-31T23-19-13-106Z-a34131aa-636f-4f65-80da-7ad3fd4e29e5.jsonl`, passes both real Rust fixtures and all eight Android stages. Cargo validates/reuses the rebuilt current package; neither fixture runtime is skipped. The independent encoder guard again proves 47-thread suspension, Stalled and physical exit. Android records a genuine checkpoint sequence three at 548 ms while the original encoder is alive, then the unchanged watchdog confirms Stalled/physical exit after 39,138 ms of observed ownership. The observer records **17 successful negative reads followed by positive evidence**, exactly one completed automatic Recover, a fresh output revision, the same viewer/history, preserved configuration and play intent, and confirmed cleanup. Final SQL enforces exactly five released viewers/histories, one worker viewer/recovery/attempt, the restarted-stall budget and zero unsettled Starts.

Application force-stop/private-data clearing/uninstall, debug/reverse mapping removal, emulator/ADB exit and listeners, backend/root resources and PostgreSQL container cleanup all complete. The run finishes normally, without memory interruption; peak global commit is 75,815,620,608 bytes, below the unchanged 94% emergency limit. No fabricated checkpoint/error, shortened watchdog, command replay or source/dependency/resource-limit change is involved.

| Attribution | SHA-256 |
| --- | --- |
| Mobile inputs | `fc4a990827e6fb85657b246e190b4e8b45a942859e8f283b84b0f81621cd4228` |
| Backend inventory | `338d0f1c6aab9636cdc68e2f711a7aa08ab932864cbde00de35c00e02a577939` |
| Host tools | `37c919c6f19ede85232b7070b028cc9c6000f5f2d1b6ea738d6843250fe60850` |
| Qualification APK | `901a8a410e934b2975d41a52bd0ad670f84b2f785e2214fbbab878307db2e626` |
| Normal APK | `f081085e4edc92cad190e57558abc1e473a4778b2a8d16c8122a0a4d559c66af` |

Post-run native-manager deletion removes the successful run's 5.099-GiB logical AVD, after exact owner/path/reparse/process/listener checks. Observed host recovery during that deletion is 5.040 GiB. Across both attempts, two new AVDs totaling 9.393 GiB logical are retired; compact evidence, current APKs/SDK/Gradle inputs and the useful rebuilt compiler checkpoint remain. Aggregate host-space changes also reflect concurrent host work and are not wholly attributed to these deletions. The [storage authority](../operations/DEVELOPMENT_STORAGE.md) records this task's maintenance boundary.

The selected stack and tradeoff stand: locked Flutter/Dart/Dio/video_player with scoped retained HTTP reads, one monotonic observer and the existing authoritative checkpoint/conditional-Recover path. This adds explicit lifecycle state and bounded metadata traffic while qualifying the timing gap without granting native errors recovery authority. Next: Android scope/control/read cancellation, then remaining relay adversarial/backpressure/output-format qualification before native ambient migration. Physical-device/TalkBack fidelity, iOS, broader formats and normal production attachment/caps remain separate.

# Root-owned SSE lifecycle

## Status and scope

Research began October 25, 2026, against `cd24f154073079132a4ee9211fb836faf7622d7d` on `main`. The preceding [manual-refresh authority](IPTV_REFRESH.md), implementation `3b060e2a76ea5030ec2aa8cb874390049a6faafd`, qualifies actual HTTP/import draining and identifies connected SSE as the next lifecycle gap. This document records current-source findings, MCP-discovered official sources, alternatives and a concrete bounded implementation contract. The user confirmed both behavior choices after research checkpoint `86d9b99`. The user confirmed the complete contract after checkpoint `e1e6275`. Implementation and scoped qualification are complete; final verification results and boundaries are recorded below. No release or PR merge is made.

[Real-time push](REAL_TIME_PUSH.md) continues to own event taxonomy, authentication, filtering, replay and client behavior. [Memory ownership](MEMORY.md) owns the shared shutdown budget. [API security](../security/API_SECURITY.md) owns safe responses and current authority boundaries. This increment concerns the existing SSE transport, not IPTV catalog delivery or playback.

## Findings at the research baseline

- `services/events_handler.rs` starts an untracked task for each response. That task owns the per-user connection guard, replay and broadcast receiver. Idle client disconnect is handled through `Sender::closed()`; application-root shutdown is not.
- Initial retry, replay and live-event sends await bounded-channel capacity without root cancellation. An idle-only cancellation branch would leave full-channel sends blocked.
- The response uses a 32-entry receiver wrapped in Axum KeepAlive. Ending the producer alone allows already queued entries to drain; it does not express immediate application-queue discard.
- `runtime::shutdown` joins actual HTTP, root-tracked tasks and imports under one shared thirty-second deadline, then closes the pool with at most sixty additional seconds. SSE needs its own admission/completion owner participating in that existing drain.
- Both AppState constructors are infallible. A shared per-AppState SSE lifecycle can be constructed there without a process-global singleton, database schema, secret store or new dependency.
- Browser EventSource and the existing bearer-fetch transport both retry successful stream EOF. Their HTTP-error retry behavior differs: native EventSource can fail terminally on non-200, while the custom transport retries selected transient statuses. A shutdown 503 must not be described as identical recovery behavior for both clients.
- The current SSE response defaults to `Cache-Control: no-cache`, rather than the private API's `no-store` policy. Axum supplies this header itself, so an if-not-present header layer would not override it.
- `REAL_TIME_PUSH.md` incorrectly describes RFC 8895 as the generic SSE wire specification. It defines an ALTO application using SSE. WHATWG HTML section 9.2 is the relevant wire/reconnection authority.

## Confirmed behavior choices

1. Confirmed: on root stop, cancel forwarding and discard each connection's remaining application queue. Do not wait to deliver it before closing. Leave the existing process-local replay ring untouched. Bytes already handed to HTTP or the operating system cannot be retracted; no event-delivery acknowledgment is introduced.
2. Confirmed: end established streams with ordinary SSE EOF, preserve existing clients and reject late authenticated admission with a fixed 503/Retry-After response. Add no shutdown event or HTTP 204. Native HTTP-error recovery remains distinct from bearer-fetch recovery; broader client changes would be a subsequent design decision.

The user accepted both recommendations. The complete concrete contract received shared-understanding confirmation through [the repository design review](../../CONTEXT.md#design-review-with-grill-me) before implementation.

## Official-source research

The following primary URLs were discovered and fetched through MCP on October 25, 2026. Installed locked source supplements version-specific documentation. Platform budgets and product queue policy remain Duskcue decisions, not external mandates.

| Primary source | Finding and application |
|---|---|
| [WHATWG HTML section 9.2](https://html.spec.whatwg.org/multipage/server-sent-events.html) | Successful SSE EOF reestablishes a connection; incomplete trailing events are discarded. HTTP 204 explicitly stops reconnection, and non-200/wrong content-type responses fail native EventSource. Preserve wire behavior deliberately rather than promising all clients retry every shutdown response. |
| [RFC 8895 official record](https://www.rfc-editor.org/info/rfc8895/) | ALTO incremental updates are an application of SSE. Its generic SSE explanation is non-normative; ALTO control/shutdown messages do not define Duskcue's transport. |
| [Locked Axum 0.8.9 SSE source](https://docs.rs/axum/0.8.9/src/axum/response/sse.rs.html) | Inner stream termination becomes body EOF. KeepAlive propagates that termination, but queued receiver entries remain readable after sender drop. Inference: cancellation belongs on the output stream as well as the producer when immediate queue discard is chosen. |
| [Locked Axum server source](https://docs.rs/axum/0.8.9/src/axum/serve/mod.rs.html) | Graceful shutdown stops acceptance and waits for connections; it does not terminate an infinite SSE response. Ending the application stream cannot prove a stalled peer received EOF. |
| [Locked CancellationToken 0.7.18](https://docs.rs/tokio-util/0.7.18/tokio_util/sync/struct.CancellationToken.html), [Tokio select semantics](https://docs.rs/tokio/latest/tokio/macro.select.html) | Cancellation is cooperative. Child cancellation does not stop the parent. A cancellation-first biased selection is appropriate when shutdown must win against ready replay/live work. Already produced bytes cannot be recalled. |
| [Locked TaskTracker 0.7.18](https://docs.rs/tokio-util/0.7.18/tokio_util/task/struct.TaskTracker.html) | Closing permits wait completion but still allows spawning; dropping does not abort tasks. Inference: synchronize admission and task registration separately, and retain explicit failure outcomes. |
| [Locked Tokio 1.52.3 Sender](https://docs.rs/tokio/1.52.3/tokio/sync/mpsc/struct.Sender.html) | Every send can wait for capacity; a selected cancellation branch drops an unsent value. Successful send does not acknowledge client delivery. Monitor cancellation during initial retry, replay, live sends and idle waiting. |
| [OWASP REST security](https://cheatsheetseries.owasp.org/cheatsheets/REST_Security_Cheat_Sheet.html) | Private API responses should use no-store and generic safe errors. Apply a streaming-preserving policy to the exact SSE route and refusals, preserving authenticated per-user access and existing credential transport. |
| [W3C ARIA22 status technique](https://www.w3.org/WAI/WCAG21/Techniques/aria/ARIA22) | Visible connection-status updates should be programmatically announced without moving focus. Backend lifecycle changes add no UI and cannot claim accessibility qualification; this guidance applies if client messaging is later changed. |

Current docs identify Tokio 1.54.1 and tokio-util 0.7.20, while the repository locks 1.52.3/0.7.18. The required ownership APIs exist in those locked versions. The later [tokio-util changelog](https://docs.rs/crate/tokio-util/latest/source/CHANGELOG.md) and [upstream change 8556](https://github.com/tokio-rs/tokio/pull/8556) describe a cancellation deadlock involving a custom descendant waker synchronously re-entering an ancestor token. That is not evidence of this repository triggering it with ordinary Tokio wakers. Avoid a custom reentrant waker and do not silently bundle an unrelated dependency upgrade.

## Alternatives, pros and cons

| Choice | Pros | Cons | Recommendation |
|---|---|---|---|
| Root-owned admission, cancellation and retained completion | Closes the registration race; joins forwarders before dependency closure; supports deterministic failures. | Adds a small lifecycle service and completion state. | Prefer. |
| Add one idle cancellation branch to the current task | Very small patch. | Full sends, queued output, late registration and unobserved task failure remain. | Insufficient. |
| Cancel and discard application-queued output | Finite application cleanup even for a slow reader; no delivery promise during teardown. | Some transient hints are lost; process-local replay does not survive restart. | Confirmed. |
| Drain queued events before ending | May deliver more pending hints. | Delivery is not acknowledged and a stalled client can consume the entire root budget. | Not selected. |
| Normal EOF with existing clients | Preserves current event/retry protocol and avoids a terminal 204 contract. | HTTP-error recovery remains client-specific. | Confirmed. |
| New shutdown event/204/client recovery protocol | Can define richer recovery behavior explicitly. | Expands client scope; 204 stops reconnection; an unsent shutdown event cannot be guaranteed. | Deferred. |
| Replace SSE with WebSocket or add a broker | Different future delivery capabilities. | Does not solve this HTTP/root ownership gap by itself and expands platform scope. | No need for this increment. |

## Recommended stack and implementation contract

Retain Rust/Axum, the existing EventBus and bounded Tokio channels. Compose a small per-AppState event-stream lifecycle with locked CancellationToken/TaskTracker APIs. Preserve existing cookie/bearer authentication, per-user isolation, UUIDv7 event identities, five-connection limit, 100-event replay, 32-entry forwarding channel, filtering, retry 5000 and fifteen-second heartbeat. Add no database schema, dependency version, broker or global singleton.

Separate lifecycle/admission/completion, forwarding, output cancellation and wire encoding into focused `services/event_streams/` files; keep `events_handler.rs` a thin authenticated HTTP adapter. This is shared transport infrastructure under the existing service convention, not a new domain. Both AppState constructors share the same lifecycle model.

Implementation properties:

- Atomically close SSE admission against task registration. Keep the synchronized section short; do not hold it over async waits or replay/encoding work. TaskTracker closure alone is not a gate.
- Retain each producer through all preparation, initial retry, replay, live delivery and destructor completion. Observe both root cancellation and response disappearance. Treat expected cancellation as normal completion; retain unexpected task-exit/cleanup failure as a sticky failure rather than a successful drain.
- Use cancellation on the output stream before polling queued events. Once ended, the output stays ended and KeepAlive cannot revive it. A response drop cancels only its own producer, not other users or the root.
- Retain authentication before shutdown refusal and use a safe fixed problem response with bounded Retry-After. Do not add a 204/shutdown event, credentials in URLs or a promise that native and custom clients retry identically.
- Apply no-store to successful SSE and exact-route extractor/admission/rate-limit errors without collecting or buffering response bodies. Preserve current CORS origins/credentials, content type and anti-buffering behavior.
- The root closes SSE admission and signals producers/output before HTTP drain. Join SSE completion alongside HTTP, existing root tasks and imports under the same thirty-second budget. Close the pool only after all relevant joins succeed; preserve the sixty-second pool-close limit, nonzero failure paths and continuing second-signal behavior.
- Existing per-user replay/filter semantics remain scoped as before. This work does not qualify exactly-once delivery, a replay/subscription transaction or replay across restart.

### Service registration and synchronized admission

Construct one shared SSE lifecycle in each AppState constructor; cloning AppState shares that owner. It is infallible and holds no database connection, encryption key or mutable global registry. Keep the existing EventBus separate from the transport lifecycle. Initialize the owner before HTTP readiness through normal state construction.

Use one short synchronized admission boundary for checking open/healthy state, reserving the existing per-user connection slot and registering retained producer work. Closing the owner uses the same boundary, making later registration impossible; never infer this from TaskTracker closure alone. Move replay preparation and encoding into owned work, outside the admission critical section. Avoid waiting, database access or replay cloning while holding that lock. Release the lock before notifying cancellation. An admitted operation that races closure receives an already-cancelled or subsequently cancelled child token and must finish without reopening admission.

Retain the current handshake authentication and per-user limit ordering: unauthenticated/invalid-session requests keep their existing authentication refusal; an authenticated request after owner closure/failure gets the fixed temporary-unavailable response, and an open owner still enforces the existing five-connection limit. This endpoint does not acquire household-management proof or introduce new permissions.

### Producers, response bodies and retained failures

A retained task owns replay preparation, broadcast receiver, 32-entry sender and connection guard. Every initial retry, replay and live send uses cancellation-aware waiting, and the idle subscription wait observes both cancellation and receiver closure. Prioritize locally observed cancellation over ready work. Normal producer cancellation or client disconnect releases the per-user slot and is successful cleanup.

The response adapter observes its connection cancellation independently of producer scheduling and before yielding queued entries. On observed cancellation it drops the receiver/remaining application queue and permanently returns EOF; subsequent polls cannot revive it. Response disappearance cancels only that connection. Axum continues to own SSE framing and KeepAlive; heartbeat generation stops when the underlying stream ends. Do not implement a custom reentrant waker or introduce a dependency solely for stream adaptation.

Registration must retain destructor completion, not merely a detached join handle. An unexpected producer exit/panic or poisoned lifecycle state sets a sticky failure, closes further SSE admission and signals existing SSE work. A later root drain reports failure even if all counts subsequently reach zero. There is no automatic owner reset or task replay. This does not newly shut down unrelated application services immediately on an SSE producer failure; the SSE service becomes unavailable, and the existing executable failure path applies when root cleanup cannot be certified.

### HTTP and privacy behavior

Keep `/api/v1/events`, existing successful status 200, `text/event-stream`, `X-Accel-Buffering: no`, retry 5000, fifteen-second heartbeat, UUIDv7 event IDs and existing event taxonomy/filter/replay behavior. Existing streams end with ordinary EOF; do not add shutdown events, HTTP 204, durable delivery, cross-restart replay or exactly-once claims.

Use a fixed 503 problem response, `SSE_UNAVAILABLE`, and `Retry-After: 5` for authenticated admission after closing or a retained lifecycle failure. Keep existing connection-limit 429 behavior while open. No error includes an event payload, credential, raw task panic or implementation detail. Broader native/bearer client recovery remains outside scope: the existing native client may stop on 503 while the custom bearer transport retries it.

Force `Cache-Control: no-store` on the exact SSE resource, including success, malformed-query/extractor/authentication, limit, closing/failure and outer rate-limit responses. Use a shared streaming-preserving HTTP response policy, retaining all existing private-route behavior. Override Axum's existing no-cache value; an if-not-present layer is insufficient. Do not read, collect or transform event bodies merely to set headers. Preserve current CORS origin/credential rules and unrelated cacheable routes.

### Root shutdown and remaining boundaries

On root shutdown, close SSE admission and signal producer/output cancellation before starting HTTP drain. Join retained SSE completion concurrently with actual HTTP, existing root tasks and imports under the existing single thirty-second budget. Only successful relevant joins allow watcher stop and up to sixty further seconds of pool closure. Keep explicit nonzero failure handling and the continuing second-signal listener. Do not add a sequential SSE cleanup timeout or close dependencies underneath unjoined work.

Already produced HTTP/OS bytes can still reach a client. A permanently stalled transport may consume the retained HTTP deadline even after all application producers stop. Preserve that failure classification instead of claiming guaranteed remote EOF delivery. No scheduler ownership, active-session revocation redesign, replay/subscription transaction, channel/EPG endpoint, client UI, Kids policy or playback capability is added.

### Concrete qualification and regression plan

- Deterministic idle, initial-retry, replay-send and live-send cancellation under full channels; queued-output discard, cancellation precedence and permanent EOF; response drop/readmission and user isolation; synchronized close/admission race; unexpected task-exit/sticky failure and actual destructor/connection-guard completion.
- Owned PostgreSQL and actual production routes/TCP for cookie and bearer auth, filtering/replay/event framing, connection caps, current refusal statuses and exact no-store behavior including outer rate limiting. A normal connected reader and a temporarily paused reader must not require new events to release application ownership.
- Run the normal executable with connected SSE and accepted manual refresh, deliver an owned signal and verify normal completion, stream termination, stopped upstream work, bounded safe logs and joined process cleanup. Retain the normal no-SSE case as appropriate.
- Replace the obsolete SSE-held-open deadline expectation with connected-stream success. Preserve the production coordinator's actual thirty-second HTTP deadline using an owned finite test router/blocked request; assert failure leaves the pool open, then release and explicitly join fixture cleanup. This is not a production test route or an automatic retry policy.
- Retain actual executable second-signal coverage with an observed blocked production request under a separate owned database coordinator connection. Its ordinary lock timeout is sufficient to keep shutdown in progress for the second signal; release/reap everything. Keep the real sixty-second pool-close failure check and unexpected HTTP-task-exit qualification. Do not add a production bypass switch or silently delete failure tests because SSE now ends.
- Execute heavy checks serially through the existing resource guard: focused SSE lifecycle/TCP/process checks and relevant manual-refresh regression, feature-enabled library tests and default-feature compilation, existing client events-store/transport tests, workflow cases, formatting/copyright/scripts/local document targets and strict/all-target lint accounting. Existing reserves, Cargo jobs, Tokio/test threads, fixture CPU/PID limits and deadlines remain unchanged. No release, deployment or PR merge.

## Qualification outcomes and baseline

All heavy checks run serially through `scripts/testing-memory/run.mjs` with the existing reserves and limits. Receipts below are retained under `.cache/testing-memory/`; they are local evidence rather than committed artifacts. The final source identity is recorded after the verification record is complete.

Final results:

- Feature-enabled library suite: **1,000 passed, seven existing ignores**, 117.48 seconds. This includes nine lifecycle tests covering idle/full-send cancellation, readiness, permanent output EOF, disconnect/readmission, close/admission races, producer panic and poisoned admission. Receipt `2026-10-25T21-52-56-328Z-8e86c245-2990-407a-8342-f4f539ad8560.jsonl`.
- Final owned PostgreSQL/TCP contract: **one passed, zero ignored**, 16.36 seconds; resource `f7e2c0d7-c2fc-473c-915e-d413ec1a2d62`, receipt `2026-10-25T21-57-26-228Z-36fd063b-109a-445e-a573-b972eb916a04.jsonl`. Both authentication transports, exact-route privacy, framing/replay/filtering, isolation/caps, disconnect/readmission and connected/paused-reader EOF pass.
- Full root/manual-refresh regression passes all three contracts in **292.98 seconds**, zero ignored cases and zero proxy-trap connections; resource `1c9ac51e-d6ff-4b05-8641-4a3b38b14374`, receipt `2026-10-25T21-39-19-967Z-dd846403-841b-449b-9b6a-a41409eeb3eb.jsonl`. Actual connected SSE now completes its HTTP response and the executable exits zero. Owned forced second-signal, thirty-second HTTP deadline, sixty-second pool deadline, accepted import/commit/revocation tests and cleanup ordering pass. Wrapper exit is zero without memory stop/interruption; minimum available host memory 6,683,680,768 bytes, peak owned private bytes 7,576,399,872 includes compilation and process qualification rather than an application RSS guarantee.
- Default-feature all-target compilation passes in 107 seconds; receipt `2026-10-25T21-58-25-203Z-6abbc75a-05d7-43b7-bcda-a939d0ea3f79.jsonl`.
- Owner-security production-route/PostgreSQL regression: **one passed, zero ignored**, 151.62 seconds; resource `a419e2eb-32d1-4ed2-8755-33b2545e7e06`, 57 safe events, receipt `2026-10-25T22-00-16-405Z-8af515e9-6441-4c73-9d22-0f4259f84f19.jsonl`. Signed WebAuthn, password proof, rotated cookie/bearer sessions, replay/recovery and revocation race remain qualified.

- Assignment production-route/PostgreSQL regression: **one passed, zero ignored**, 132.42 seconds; resource `69d8138f-c7ff-4d25-b3df-30d136e88379`, 17 safe events, receipt `2026-10-25T22-03-35-365Z-bb9ceec4-d8c1-45c1-9435-61f6c1981b03.jsonl`. Conditional writes, current proof/permission, delegation/capability races, encrypted storage, rotation/replacement, Kids refusal and outer privacy remain qualified.
- Existing client event-store/transport regressions: **21 passed across two files**, 2.92 seconds; receipt `2026-10-25T22-06-32-834Z-75e2e1c5-271b-49a0-b9db-444afcda45ca.jsonl`. The client command also passes its 37 existing Node fixture/pretest cases. No client source is changed.
- Workflow/resource-policy regressions: **16 passed**, zero failures/skips; receipt `2026-10-25T22-06-47-603Z-0e50ad34-c4c9-4707-98c9-a425c0295c03.jsonl`. Worker/heap budgets and ownership refusal remain intact.
- Final all-target warning diagnostic completes in 4.91 seconds; receipt `2026-10-25T22-06-50-844Z-bd91247a-b5da-443f-a7a9-58e4e80c7aae.jsonl`. It retains **28 library / 32 library-test baseline warnings plus two existing Tonight integration casts**. Every diagnostic location is outside this change. No introduced warning remains. The existing `proc-macro-error2` future-incompatibility warning remains.


- Strict Clippy exits **101** on the same 28 library / 32 library-test findings; receipt `2026-10-25T22-06-58-382Z-6ff4e29f-4a2f-4db9-a773-f67d17e17525.jsonl`. This is not a strict-lint pass. The diagnostic pass above is used only to account for existing versus introduced warnings.
- Workspace and isolated compression-fixture formatting, staged whitespace, copyright (**800 tracked source files**) and **750 local link targets across thirteen changed documents** pass. All seven containers registered to the initial/final TCP, root, owner and assignment qualification runs are confirmed absent after cleanup. Cargo.lock, manifests and client sources are unchanged. Rust/Cargo remain **1.99.1**, rustup **1.29.2**; no additional SDK/dependency update is required for this lifecycle.

All final functional wrappers exit zero without memory stop/interruption; strict lint records the expected compiler exit 101 without a memory stop/interruption. Final library/TCP/owner/assignment checks preserve at least 11,175,518,208 available host bytes. The separate full root regression above includes compilation and executable qualification and has its own lower minimum and explicit resource accounting. Seven library ignores are existing environment-dependent cases, not unexecuted SSE acceptance tests.

Reproduction commands, run serially with the existing resource guard (Windows executable signal qualification requires the existing attached console/ConPTY):

```powershell
node scripts/testing-memory/run.mjs cargo-test --features playback-qualification --lib
node scripts/testing-memory/run.mjs cargo-test --features playback-qualification --test sse_lifecycle -- --ignored --nocapture
node scripts/testing-memory/run.mjs iptv-refresh
node scripts/testing-memory/run.mjs cargo-check --all-targets
node scripts/testing-memory/run.mjs cargo-test --features playback-qualification --test household_owner_security -- --ignored --nocapture
node scripts/testing-memory/run.mjs cargo-test --features playback-qualification --test live_tv_assignment -- --ignored --nocapture
node scripts/testing-memory/run.mjs web-unit events-store.test.ts events-transport.test.ts
node scripts/testing-memory/run.mjs iptv-transport-unit scripts/testing-memory/presets.test.mjs scripts/testing-memory/policy.test.mjs
node scripts/testing-memory/run.mjs cargo-iptv-clippy --cap-lints warn
node scripts/testing-memory/run.mjs cargo-iptv-clippy
```

Corrected and preceding attempts:

- Initial seven lifecycle tests pass in 0.01 seconds; receipt `2026-10-25T21-23-33-215Z-6bc12ff5-4ec3-45e2-b68c-d1838e1d7715.jsonl`. Five unused received-Event warnings in the fixture were corrected rather than suppressed.
- Readiness-aware seven-test extension passes without new compiler warnings in 0.08 seconds; receipt `2026-10-25T21-30-16-555Z-0bae9fb3-338d-4c18-8ca5-bb14457f7ce5.jsonl`.
- Real owned PostgreSQL/TCP SSE contract passes in 15.46 seconds with zero ignored cases; resource `08208b71-386a-4ad3-9314-c018579ff37e`, receipt `2026-10-25T21-32-28-715Z-add99e74-5186-46c3-a88c-93c73aadd3d2.jsonl`. Cookie/bearer, replay/filter/isolation, disconnect/readmission/cap, ordinary EOF/paused readers, safe late refusal, malformed query and outer-rate-limit privacy pass.
- Initial all-target diagnostic completes in 118 seconds with one introduced nested-condition warning in the process fixture, receipt `2026-10-25T21-50-06-243Z-9f2af84f-e863-412c-99e1-17df0597eb5e.jsonl`. The fixture condition was corrected without suppressing the diagnostic. The final diagnostic above confirms its correction, with no introduced warning.


Before implementation, the unchanged focused SSE suite passed five tests with zero failures/ignores and 998 filtered cases:

`node scripts/testing-memory/run.mjs cargo-test --features playback-qualification --lib services::events_handler::tests`

Receipt: `.cache/testing-memory/2026-10-25T19-44-23-587Z-c15ebdce-d9bf-4f1c-9315-3160a61c5f51.jsonl`. This proves the existing idle-disconnect and encoding baseline, not new root shutdown behavior. The preceding manual-refresh authority retains the actual connected-SSE deadline failure evidence. The baseline wrapper exits zero without memory stop/interruption, preserving at least 26,155,548,672 available host bytes; peak owned private bytes are 263,061,504. That research-only documentation checkpoint passed whitespace checks and 618 local link targets across five Markdown documents. Its baseline result does not imply a new runtime implementation result.

## Random open PR applicability

Fresh MCP GitHub search returned one open candidate on October 25. Uniform selection from that one-item set selected [PR 5](https://github.com/cloudbyday90/Duskcue/pull/5), head `c018d77b7f09543e5960a571dc43e2f061cedf0a`, open and unmerged. MCP metadata, the complete two-file path list and both exact file patches were inspected. They request PostCSS 8.5.23 and Nano ID 3.3.16. Both current lockfiles already contain 8.5.30/3.3.20. The PR is superseded: no downgrade, local patch, merge, comment or closure applies.

## Qualification boundaries

The owned TCP cases exercise HTTP/1.1, not a new browser/mobile/TV end-to-end journey or reverse-proxy/HTTP2 matrix. Actual executable signals use the existing Windows console-break helper; Unix signal implementation is unchanged and is not newly process-qualified here. Connected and temporarily paused readers are qualified; already handed-off bytes and permanently stalled transports remain subject to the retained deadline/failure boundary. Existing client unit regressions do not establish identical native and bearer HTTP-error recovery. No schema, dependency version, catalog/playback grant, new scheduler or release is introduced.

## Next item

Design standard-profile-scoped, read-only channel/EPG delivery under current household assignment, source generation and profile permissions. Return safe bounded metadata without revealing upstream stream/logo credentials, and preserve explicit missing/stale/ambiguous guide evidence. Resolve that API contract before implementation. Scheduling, owner unlock UI, client recovery, Kids programme policy and shared playback admission remain separate.

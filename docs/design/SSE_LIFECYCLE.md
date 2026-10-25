# Root-owned SSE lifecycle

## Status and scope

Research began October 25, 2026, against `cd24f154073079132a4ee9211fb836faf7622d7d` on `main`. The preceding [manual-refresh authority](IPTV_REFRESH.md), implementation `3b060e2a76ea5030ec2aa8cb874390049a6faafd`, qualifies actual HTTP/import draining and identifies connected SSE as the next lifecycle gap. This document records current-source findings, MCP-discovered official sources, alternatives and a concrete bounded implementation contract. The user confirmed both behavior choices after research checkpoint `86d9b99`. The complete contract below is ready for the required final shared-understanding confirmation. No SSE source change, new lifecycle qualification, release or PR merge is claimed by this checkpoint.

[Real-time push](REAL_TIME_PUSH.md) continues to own event taxonomy, authentication, filtering, replay and client behavior. [Memory ownership](MEMORY.md) owns the shared shutdown budget. [API security](../security/API_SECURITY.md) owns safe responses and current authority boundaries. This increment concerns the existing SSE transport, not IPTV catalog delivery or playback.

## Current-source findings

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

The user accepted both recommendations. The complete concrete contract must receive shared-understanding confirmation through [the repository design review](../../CONTEXT.md#design-review-with-grill-me) before implementation.

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

## Baseline and checkpoint outcomes

The unchanged focused SSE suite passes five tests with zero failures/ignores and 998 filtered cases:

`node scripts/testing-memory/run.mjs cargo-test --features playback-qualification --lib services::events_handler::tests`

Receipt: `.cache/testing-memory/2026-10-25T19-44-23-587Z-c15ebdce-d9bf-4f1c-9315-3160a61c5f51.jsonl`. This proves the existing idle-disconnect and encoding baseline, not new root shutdown behavior. The preceding manual-refresh authority retains the actual connected-SSE deadline failure evidence. The baseline wrapper exits zero without memory stop/interruption, preserving at least 26,155,548,672 available host bytes; peak owned private bytes are 263,061,504. This documentation checkpoint also passes whitespace checks and 618 local link targets across five Markdown documents. No new source build/lint or runtime implementation result is implied.

## Random open PR applicability

Fresh MCP GitHub search returned one open candidate on October 25. Uniform selection from that one-item set selected [PR 5](https://github.com/cloudbyday90/Duskcue/pull/5), head `c018d77b7f09543e5960a571dc43e2f061cedf0a`, open and unmerged. MCP metadata, the complete two-file path list and both exact file patches were inspected. They request PostCSS 8.5.23 and Nano ID 3.3.16. Both current lockfiles already contain 8.5.30/3.3.20. The PR is superseded: no downgrade, local patch, merge, comment or closure applies.

## Next item

Both behavior choices are confirmed. Confirm shared understanding of the complete contract above, then implement and qualify it on main without a release. After connected-client root shutdown is qualified, proceed to profile-scoped channel/EPG reads under current assignment/source authority. Scheduling, owner unlock UI, Kids programme policy and playback remain separate.

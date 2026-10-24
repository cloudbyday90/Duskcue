# IPTV transport qualification

## Scope and design review

This document owns the first supervised transport slice within [IPTV milestone 2](IPTV.md). The user confirmed the scope, failure/rolling-buffer contract and legal-header exception on October 24 under the repository [grill-me workflow](../../.agents/skills/grill-me/SKILL.md). Existing household, owner-proof, Kids, cap and client decisions remain authoritative. The controlled Linux transport qualification is complete behind the existing default-off `playback-qualification` feature. Production Live TV remains a later milestone.

Qualify a controlled HTTP MPEG-TS source, restricted managed FFmpeg pipe input, rolling fMP4 HLS, production cookie/bearer authentication in a qualification-only router, decoded video/audio and confirmed resource release. Do not enable a product Live TV start route or invent catalog/play-session records. Real Dispatcharr and complete web/Tauri journeys remain later evidence; this slice targets their agreed HTTP MPEG-TS contract.

## Official research — October 24, 2026

URLs were discovered through MCP web search. Installed dependency source was inspected where current documentation describes a newer release. Search/fetch failures are not treated as evidence of an API guarantee.

| Source | Finding and decision | Tradeoff |
|---|---|---|
| [Dispatcharr system/output profiles](https://dispatcharr.github.io/Dispatcharr-Docs/system/) | Consume proxied MPEG-TS as agreed; Dispatcharr owns provider failover. | Duskcue still needs its own connection and worker lifecycle. No provider-management adapter. |
| [OWASP SSRF prevention](https://cheatsheetseries.owasp.org/cheatsheets/Server_Side_Request_Forgery_Prevention_Cheat_Sheet.html) | Bind each operation to an explicitly authorized destination; reject redirects and forbidden addresses. The initial qualifier accepts canonical literal IPs, including explicitly selected LAN/loopback destinations, and performs no DNS. | Hostname support is deliberately unavailable in this internal constructor until a separately owned resolver is qualified. Syntax validation continues accepting hostnames without granting network permission. |
| [Reqwest ClientBuilder](https://docs.rs/reqwest/latest/reqwest/struct.ClientBuilder.html) and installed 0.12.28 source | Operation-scoped client; no ambient proxies, automatic retries or redirects; bounded connect/header/read waits; incremental body chunks with backpressure. | Deliberate retry requires a new authorized attempt. A full-response timeout is inappropriate for an indefinite live body. No dependency upgrade is needed. |
| [FFmpeg protocols](https://ffmpeg.org/ffmpeg-protocols.html) | Rust owns HTTP and supplies only MPEG-TS bytes to `pipe:0`; FFmpeg retains network-denying isolation and `-nostdin`. | Requires explicit stdin transfer before process waiting. Upstream URLs/secrets never appear in FFmpeg arguments. |
| [FFmpeg HLS formats](https://www.ffmpeg.org/ffmpeg-formats.html) | Separate live arguments: finite playlist, deleted old segments, atomic temporary publication, regular keyframes and H.264/AAC output. | A segment window is not a hard byte quota. Independent file/byte accounting and a hard bounded fixture filesystem must also be qualified. The documentation endpoint was intermittently unavailable; option behavior must be tested against the actual managed build. |
| [Tokio shutdown](https://tokio.rs/tokio/topics/shutdown), [Child lifecycle](https://docs.rs/tokio/latest/tokio/process/struct.Child.html) | Cancellation requests cleanup; completion means the process has exited and owned tasks/readers/cache have been released. Retain a task owner after an observing caller disappears. | Drop alone cannot promise asynchronous cleanup completion. Failed cleanup must remain explicit and retain ownership/capacity. |
| [W3C Media Source Extensions](https://www.w3.org/TR/media-source-1/) | Live media has a moving seekable range; do not reuse movie resume/completion assumptions. | No client Live TV interaction is enabled here. Later client work must qualify its own live-edge and expired-buffer behavior. |
| [WCAG 2.2](https://www.w3.org/TR/wcag/) and [focus not obscured](https://www.w3.org/WAI/WCAG22/Understanding/focus-not-obscured-minimum) | Preserve keyboard access, focus and accessible status/error messages when the client milestone exposes controls. | Backend/decoder tests establish no UI accessibility conformance. |

## Repository evidence

- The current HLS builder uses an unlimited EVENT playlist. Its readiness parser requires VOD/EVENT. Keep both unchanged for catalog playback and create a live-specific output/readiness policy.
- `tokio-process-tools` 0.11.2 replaces command stdin with a pipe, then `wait_for_completion` closes the handle's stdin. Transfer `Stdin::Open` to the input pump before waiting; retain the process-group wrapper.
- Reuse `sandbox::launch::PreparedLaunch` with an explicit pipe-input preparation path. Preserve existing file-input preparation, filesystem permissions and network-denying syscall policy.
- Current movie authorization depends on catalog/play-session records. A qualification-only router can reuse production authentication/capability extractors and enforce exact fixture account/profile ownership without creating fake movie records.
- Tokio hostname lookup uses blocking resolver work whose cancellation cannot be proved by dropping a timeout future. The first transport constructor therefore rejects hostnames instead of claiming a joined DNS task.
- Raw reqwest errors can contain URLs; application error display is logged. Use fixed transport error variants, secret-free source types and no raw stderr/URL forwarding. Connector diagnostic logs require an explicit redaction check.

## Confirmed contract and recommendation stack

Use existing Rust/Tokio/Reqwest 0.12.28, an operation-scoped literal-IP HTTP source, the managed FFmpeg sandbox and process-group owner, a backpressured stdin pump, separate live HLS/cache/readiness modules and a tracked supervisor. No new singleton or database schema is needed.

The first fixture requires video and audio and uses known SDR H.264/AAC input. It does not establish HDR, arbitrary provider codecs, subtitles or upstream nested playlist support. Encode a bounded compatible H.264/AAC rendition rather than declaring all MPEG-TS content copy-safe.

Initial engineering envelope: two-second segments, fifteen retained entries (approximately thirty seconds), two extra deleted-segment grace entries, 32-KiB manifest reads, 4-MiB fragment reads, a 32-MiB/24-file cache monitor and a hard 64-MiB qualification tmpfs. The monitor is not represented as a hard production filesystem quota. Qualify multiple complete rotations and decode output after old segments disappear. This is a rolling transport buffer, not persistent DVR/catch-up or a settled pause UX.

Use a fifteen-second total startup budget, five-second connect/header and body-idle budgets, and a ten-second observed normal-stop budget. Cancel during connect, response headers, startup, input backpressure and ready playback. EOF/stall/malformed input fails and cleans up; no automatic reconnect is selected. Dispatcharr may still perform its own provider failover while the HTTP stream remains healthy.

The owner must stop admissions before shutdown, signal all sessions and await tracked cleanup. Dropping a session requests cancellation. Successful Stop/shutdown is reported only after process exit, readers/pump completion and cache removal. A failed cleanup remains a failure and retains the affected resource ownership; timeouts must not be reported as successful release.

Alternatives: direct FFmpeg HTTP is simpler but loses the current network boundary and central destination policy; copying VOD output/readiness retains the wrong storage/lifecycle semantics; an unbounded reconnect loop obscures outages and complicates revocation. None is selected for this slice.

## Acceptance and implementation outcome

The confirmed controlled scope passed its actual managed Linux contract. Evidence covers forbidden/encoded address refusal, redirects/ambient proxy refusal, safe errors, actual video/audio decoding, authenticated cookie/bearer reads and ownership refusals, cache rotation and ceilings, cancellation, exact child disappearance, joined tasks/readers and cleanup. This completes the scoped transport experiment, not the later production or real-client milestones.

The implementation uses focused `live_tv/transport` source, pump, worker, owner, cache and playlist modules. It reuses the existing process-group library, transfers stdin before terminal waiting and sends FFmpeg diagnostics directly to the OS null device, allocating no log-reader tasks or replay buffer. The pump is explicitly awaited. A small shared structural fMP4 inspection wrapper preserves existing VOD readiness behavior; pipe-specific sandbox preparation adds no new network or filesystem permission.

The qualification test has separate authentication, upstream, media/process and diagnostic-capture helpers. A dedicated guarded driver reuses source snapshots, exact container ownership, compiler memory telemetry and Linux reserve monitoring. It does not redefine a VOD qualifier or use the inherited historical server executable as current-code evidence.

### Passing runtime evidence

Run `9cd945d0-b676-433a-beae-d3ae9916b7f5` passed on October 24, 2026. Its exact integration test passed once with zero ignored cases in 117.09 seconds; the guarded workflow finished in 142.008 seconds, including cached compilation/preparation and owned cleanup. Evidence is `.cache/testing-memory/iptv-transport-9cd945d0-b676-433a-beae-d3ae9916b7f5/result.json` and adjacent compiler, runtime, source-manifest and Linux-memory receipts.

| Evidence | Observed outcome |
|---|---|
| Source/image binding | Source SHA-256 `5cbf3efba57f489dfdb5519557a78fb9c427c77bcf56ede8112b9ac867bf6e07`; qualification image `sha256:83bffbec2e592926b15d7ec6ae36c412ab467bb7051ad35aa626163a52194a47`; immutable managed runtime base `sha256:3bb308dc35122dd3217436864a683d1975437b945fe3fcc6b37c202a6d81f4ab`. The newly compiled exact test binary executes current source; the inherited historical server binary is not used as current-code evidence. |
| Rolling output | Media sequence 30 establishes two complete fifteen-entry rotations; an early segment is unavailable. Observed peak 6,843,484 bytes / 19 files remains below the 32-MiB / 24-file monitor. Independent sparse-byte and file-count faults each stop the worker with the fixed cache error. |
| Actual media | Both cookie and bearer paths return authenticated init/fragments; ffprobe reports H.264 640×360/yuv420p and AAC stereo/48 kHz. Independent decoding yields 691,200 RGB bytes with nonuniform pixels and nonzero PCM measured at 996 Hz against the 997-Hz source. |
| Authorization | Anonymous, member, Kids, other account and same-account alternate-profile reads are refused; expired credentials are refused. Manifest/output responses are no-store. No catalog `play_sessions` row is created. |
| Failures | Redirect trap receives zero requests. HTML, invalid TS, EOF and stalled input return the selected fixed errors. Impossible ambient proxy settings do not intercept the approved source. Bounded connector/auth diagnostic capture contains neither the source secret nor actor tokens. |
| Lifecycle | Normal Stop takes 12 ms. Immediate startup cancellation, abandoned sessions, a verified SIGSTOP/backpressured input pipe, dropped owner and explicit shutdown all confirm process/task/input/cache release. Shutdown closes admissions. Owned containers and temporary image tags are removed; the compiler probe is removed. |
| Resource envelope | Runtime and PostgreSQL each have a 1-GiB memory/no-extra-swap ceiling, two CPUs and 128 PIDs. Nonroot read-only runtime uses a hard 64-MiB fixture tmpfs; the database has verified 16-MiB buffers / 16 connections. No host ports, bind mounts or Docker socket. Runtime exits zero without OOM. All 67 Linux samples preserve at least 5,503,209,472 available bytes; admission/reserve limits are unchanged. |

### Checks and corrected attempts

- The feature-enabled Rust library run passes **975 cases**, with seven existing environment-dependent ignores (982 total). Six focused transport cases cover canonical destinations, playlist/argument boundaries, HTTP-header and real TLS-handshake cancellation with peer EOF, and refusal of unsupported single-threaded runtimes before allocation. Receipt: `.cache/testing-memory/2026-10-24T16-10-43-552Z-5299e827-2548-4847-bcdb-381d09aa6baa.jsonl`.
- The Windows integration target compiles. Separate default-feature `cargo check` passes in 34.74 seconds; receipt `.cache/testing-memory/2026-10-24T16-43-15-008Z-085c3b10-5c7b-4b30-9ded-fca9f323ce20.jsonl`. Four serial JavaScript proof/ownership cases pass; malformed, duplicate, wrong-owner and incomplete receipts fail. Lightweight receipt tests are wired into existing Linux CI; hosted execution is separate evidence. Rust formatting, Node syntax, diff whitespace and copyright checks pass.
- Strict feature/all-target Clippy exits 101 on unchanged code: 28 library diagnostics / 32 library-test reports including duplicates, plus two pre-existing integration-test casts. The final all-target warning diagnostic exits zero with 34 reports at 27 unique existing locations and none on introduced/changed lines. A new fixture `collapsible_if` diagnostic found during implementation was fixed. No allow or lint-policy change hides findings. Receipts: `.cache/iptv-transport-checks/clippy-strict.log`, `clippy-final-diagnostic.log` and `.cache/testing-memory/2026-10-24T16-24-15-229Z-9dc6e1b9-4b31-4996-8804-d73e3fcd770c.jsonl`. The existing `proc-macro-error2` future-incompatibility warning remains.
- The first focused cancellation test exposed incorrect explicit-stop error normalization; it was corrected and repeated successfully. The first Linux attempt `928e3abb-3d6b-43f8-bf4d-ef458c05ed33` was held before compilation at 5,667,057,664 available bytes against the unchanged 5,905,580,032-byte admission threshold. The admitted fresh dependency build `f020dde8-8632-4a94-8356-290bcfb32cd7` compiled, then failed because the process library requires a multithreaded Tokio runtime. The integration test now uses two runtime workers, the owner rejects unsupported runtimes before allocation, and a negative test covers that requirement. Cleanup failure in that attempt was not represented as success.
- Corrected build `e64b99bd-10e3-4827-9fe4-ce907866d1a5` compiled successfully but was held before runtime when fresh Linux headroom fell below admission. The unchanged-source cached repeat above passed after headroom recovered. No reserve, WSL cap, deadline or unrelated service was changed; the final resource check shows only unrelated user services remaining. Held/failed attempts remain in their own evidence directories.

Local Rust/Cargo are 1.99.1 with rustup 1.29.2, as recorded in [toolchain maintenance](../operations/RUST_TOOLCHAIN.md). The immutable Docker builder actually reports Rust/Cargo 1.96.0; this qualifier does not claim a container compiler upgrade.

### Boundaries and next item

The positive stream is controlled IPv4-loopback HTTP with known SDR H.264/AAC MPEG-TS. Literal IPv6 acceptance and stalled TLS cancellation are tested, but successful IPv6/TLS streaming, hostname resolution, real Dispatcharr, actual web/Tauri playback, broad codec/HDR/subtitle support, pause/live-edge UX and UI accessibility remain unqualified. The monitored cache ceiling is not a hard production quota. VOD EVENT arguments/readiness remain unchanged; this increment does not repeat the complete movie/client qualification matrix.

Proceed to milestone 3: design and qualify fresh owner proof, sensitive authenticator enrollment/recovery, administrator delegation and encrypted household assignment. Keep networking disabled in that management slice until destination authority and separately owned hostname/TLS fetching are qualified. Production live sessions still require common household viewer reservations, Kids/programme/revocation checks and root application shutdown integration. [IPTV.md](IPTV.md), [API security](../security/API_SECURITY.md), [project structure](PROJECT_STRUCTURE.md) and [testing memory](../ci/TESTING_MEMORY.md) cross-reference these limits.

## Random PR applicability

MCP search again returned one open candidate, [PR #5](https://github.com/cloudbyday90/Duskcue/pull/5). Random selection chose #5, immutable head `c018d77b7f09543e5960a571dc43e2f061cedf0a`; it remains open and unmerged. Both current lockfiles contain PostCSS 8.5.30 and Nano ID 3.3.20, superseding the PR's requested versions; Vite remains 8.1.6. No local PR patch or merge applies; no dependency downgrade is made.

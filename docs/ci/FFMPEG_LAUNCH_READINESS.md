# Managed FFmpeg verification readiness

Source-only checkpoint, October 7, 2026. No compiler, container or playback process was launched in this interval. This note supplements [launcher qualification](FFMPEG_LAUNCHER_QUALIFICATION.md); it does not replace runtime evidence or authorize bypassing resource admission. Rust-analyzer and user services remain running.

The managed-launch/readiness source set now has twenty prepared focused units and five ignored Linux runtime cases. None has been executed against this source checkpoint. The corrected production-launch tests separately require finite completed VOD output and active throttled encoding with initialized/segmented output. The active case records the locked process handle's PID, requires `/proc/<pid>` before Stop, and requires its absence after awaited stop, alongside permit/cache cleanup. Both cases attempt cleanup before returning a proof error. The infinite VOD case deliberately does not require a final manifest.

## Windows commands after a fresh normal gate

Run these separately through the existing wrapper, preserving its one-job lock and Cargo's 12 GiB commit reserve:

```powershell
node --max-old-space-size=128 scripts/testing-memory/run.mjs cargo-check
node --max-old-space-size=128 scripts/testing-memory/run.mjs cargo-test --lib services::transcoding::lifecycle::tests -- --include-ignored --nocapture
node --max-old-space-size=128 scripts/testing-memory/run.mjs cargo-test --lib services::transcoding::arguments::tests -- --include-ignored --nocapture
node --max-old-space-size=128 scripts/testing-memory/run.mjs cargo-test --test playback_stop_contract stop_is_idempotent_serializes_heartbeat_seek_and_retains_original_profile -- --exact --ignored --nocapture
node --max-old-space-size=128 scripts/testing-memory/run.mjs cargo-clippy
```

Use the verified FFmpeg 8.1.2 directory from ignored `.cache/tonight-tools/ffmpeg-8.1.2/provenance.json` only in the child PowerShell/workflow PATH. Lifecycle requires that host executable plus Node. Argument-output testing requires the retained immutable focused-font image via `DUSKCUE_TEST_FFMPEG_IMAGE`, then generates its own source fixture. The SQL case requires `DUSKCUE_TRANSCODE_ACCESS_TESTS=disposable` and `DUSKCUE_TEST_PLAYBACK_SOURCE` pointing to the retained eight-second `source.mkv` under workspace `.cache/tonight-server-ffmpeg`. Remove inherited `DUSKCUE_DATABASE_URL` from that child environment to select a fresh registered PostgreSQL 18 container; do not alter the user/global environment. The helper uses 512 MiB RAM/total memory-plus-swap, two CPUs, 128 PIDs and a dynamic loopback port. Source is validated before database creation. The cached runner currently accepts only a verified fresh library-test artifact, so it cannot execute the integration-test binary or repair a stale manifest by refreshing hashes.

Windows checks do not compile `cfg(target_os = "linux")` modules or the C bootstrap. The existing strict Clippy baseline must remain distinguished from introduced failures. A held workflow starts no compiler and is not passing evidence.

## Linux artifact prerequisites and exact inner commands

There is currently no admitted Docker compiler preset. The existing unit-policy caption/policy helpers must not be used to compile Linux artifacts; their two-minute diagnostic timeout is also unsuitable. Prepare a separate normal Cargo/build-policy runner with registered ownership, bounded compiler memory/no-swap/CPU/PIDs, `CARGO_BUILD_JOBS=2` and explicit `-j 2`. Host wrapper environment does not automatically enter Docker/BuildKit. The Dockerfile release Cargo RUN now explicitly limits compilation to two jobs; this source setting has not been exercised in a current image build. Full recipe qualification must also bound and sequence its independent frontend/Rust stages. Preserve original images/tags/services and inherited security settings.

Within that future owned builder, use the current Dockerfile's exact strict C command and protected runtime packaging. Then compile the current Linux crate and test executable:

```sh
cargo check -p duskcue --locked -j 2
cargo test -p duskcue --locked -j 2 --lib --no-run --message-format=json
cc -std=c11 -O2 -Wall -Wextra -Werror -Wl,-z,now,--no-as-needed -o /out/duskcue-ffmpeg-probe native/ffmpeg-bootstrap/probe.c /tmp/duskcue-ffmpeg-bootstrap.so
```

Select the library-test executable from Cargo's actual artifact JSON; record its hash, source/Cargo/native/Docker/font-config hashes, exact C commands, architecture, compiler/package versions and immutable qualification image ID. Production runtime does not contain that executable or the test-only probe. The isolated qualification image must install the fixed root-owned probe and its ordered probe/library manifest described in launcher qualification, retain the exact production bootstrap, and copy the source clip into an owned Linux overlay path. Windows-bind Landlock EACCES remains a separate qualification requirement.

Before executing, list each exact case and refuse zero tests. Run the verified executable with `RUST_TEST_THREADS=1`, `DUSKCUE_TEST_RESOURCE_ID` and the owned source path. The exact five runtime selectors are:

```text
services::transcoding::launcher::tests::actual_production_managed_ffmpeg_writes_a_finite_completed_hls_playlist
services::transcoding::launcher::tests::actual_active_managed_ffmpeg_releases_child_before_cache_and_permit
services::sandbox::owned_output::tests::actual_mandatory_bootstrap_denies_later_exec_network_and_outside_paths
services::sandbox::owned_output::tests::actual_constructor_refuses_missing_corrupt_and_wrong_arch_policy_before_main
services::sandbox::owned_output::tests::abandoned_managed_observer_terminates_after_stdout_eof_before_output_cleanup
```

For each selector, first use `<verified-lib-test> <selector> --exact --list`, then `<verified-lib-test> <selector> --exact --ignored --nocapture` in a registered bounded runtime container. This is a proposed future owned-runner sequence, not an existing executable command/preset on this host. ARM64 requires its own native build/runtime proof. Linux SQL qualification additionally needs an externally owned disposable PostgreSQL connection and the embedded `CARGO_MANIFEST_DIR` fixture/marker layout; do not mount the host Docker socket into the test image merely to reuse the self-starting Windows helper.

## Required progressive playback decision

Exact [FFmpeg 8.1.2 HLS source](https://raw.githubusercontent.com/FFmpeg/FFmpeg/refs/tags/n8.1.2/libavformat/hlsenc.c) suppresses VOD playlist publication until the trailer. Current production arguments use VOD, so the implementation cannot promise playback while a long encoder continues. This is source-confirmed behavior; no current production execution failure was observed in this interval.

Recommend EVENT for append-only progressive publication and retained earlier segments, matching existing seek/restart behavior. Omission with list size zero could also retain segments, but EVENT makes that promise explicit. Production arguments and the final filter remain unchanged pending review and runtime proof. File-protocol non-VOD publication uses temporary files plus rename; the current filter does not allow those rename calls. Determine the actual native syscall under the managed artifact, preserve Landlock session/path scope, and prove atomic playlist readers before allowing the smallest justified operation.

The minimum change also needs first playable playlist/init/complete-segment readiness under the existing pending guard before start/seek success. The client currently treats startup 404 as denied/fatal. Preserve known full title runtime rather than replacing it with the growing EVENT window, start at stream-relative zero rather than a live edge, preserve seek offsets, and require final ENDLIST/natural media completion before completion/autoplay. Canonical authenticated manifest rewriting already preserves EVENT and absent ENDLIST; no auth/profile/Kids ownership relaxation or route/DTO redesign is needed. Real manager/HTTP and actual progressive-HLS browser cases must prove publication growth, start before physical encoder exit, seek/stop cleanup, and end semantics.


## First-playable startup source checkpoint

`TranscodeManager::start_session` and `start_remux_session` now confirm bootstrap and first usable HLS output before returning; seek replacement inherits that confirmation before its metadata/result publication. One 15-second total deadline begins after worker ownership/session registration and covers both phases. No explicit HTTP timeout was found in the current router or browser API core, so this is a documented application startup default rather than a claimed proxy/client budget. It is not an additional 15 seconds after the existing bootstrap timeout. Failed/expired confirmation awaits owned worker Stop and cache cleanup. Observer Drop requests the same owned actor cleanup; it does not detach a process or release a live permit. Physical cleanup remains required even after the observation deadline, including the configured ten-second default grace and force/retry behavior when exit cannot yet be confirmed.

`lifecycle_execution.rs` keeps execution state separate from cleanup completion. Only a successful natural process exit yields Succeeded; unsuccessful exit or timeout-driven termination yields Failed. Cancellation has priority, and a late `progress=end` cannot promote it. Running with a lost observer channel fails closed. These internal watch states add no public DTO/auth/profile contract.

The focused readiness files separate orchestration, cache ownership, playlist references and MP4 extents. Inspection allows a 1 MiB playlist, bounded lines, the known flat initialization/decimal segment names and stable file snapshots. External URLs, traversal, query/fragment tokens, symlinks, alternate resource tags and unimplemented byte-range/master layouts are refused. Asset inspection reads only at most 256 top-level box headers per file, with 16 MiB initialization and 512 MiB fragment metadata limits; payloads are not buffered. Complete extents and required ftyp/moov or moof/mdat structure are checked, including extended headers and truncated/zero-size refusal.

The [current published W3C ISO BMFF byte-stream format](https://www.w3.org/TR/mse-byte-stream-format-isobmff/), reviewed October 7, 2026, provides the initialization/media box distinction. These checks establish a bounded startup structure, not complete codec/sample validation or full MSE conformance; actual decoder output still requires the separate managed-runtime/browser proof. Snapshot consistency alone does not prove an atomic writer. Current direct-written VOD therefore requires final ENDLIST plus successful physical publisher exit before acknowledgment. Synthetic EVENT tests use an actual temporary-file rename and accept referenced closed assets while the producer runs; the real managed EVENT writer/rename/path proof remains required before the production switch.

Ten additional tests are prepared for valid/incomplete/malformed references; external/traversal/resource refusal; transient missing-to-atomic-ready publication; complete versus truncated asset extents; VOD publisher success; terminal failure/cancellation; bounded/unowned/symlink resources; a shared bootstrap/startup deadline; an abandoned observer with awaitable physical cleanup; successful exit with missing referenced assets; and unsuccessful exit despite late progress=end. Four cases use the existing tiny owned Node-child fixture. No Rust compiler, child runtime, Cargo tests or container ran in this source-only checkpoint. Scoped rustfmt and diff checks pass; notices use the project's exact AGPL copyright line.

Production VOD arguments and the final sandbox filter remain unchanged. Long VOD encoding still cannot publish during the encoder run and will hit the startup deadline unless it completes within the window. This source step prevents returning an unavailable URL and tightens ownership; it does not claim progressive playback is solved. The next required EVENT step still includes actual native rename/path enforcement, first-playable HTTP/session publication, known full runtime, relative-zero playback and final ENDLIST/ended semantics described above.


The earlier pushed checkpoint `36c9fd3` Docker CI compiled the C bootstrap successfully, then failed Linux Rust at `sandbox/owned_output.rs` because locked tokio-process-tools has no `GracefulShutdown::default()`. This source fixes that real compile defect with the builder API and explicit Unix SIGTERM grace from `ResourceLimitsConfig::default().ffmpeg_shutdown_grace_secs` (ten seconds). Storyboard generation previously had no propagated custom grace field, so this preserves the shared default without inventing a Default implementation; transcoding still uses its configured grace. The new source, including readiness, still requires fresh compilation. Earlier C compilation is not runtime/mandatory-launch proof.

# Linux build durability

Status: scoped implementation and qualification complete, October 26, 2026, on main without a release. This supplements [VOD output lifecycle](../design/VOD_OUTPUT_LIFECYCLE.md). It concerns Linux Rust compilation and qualification, following the user's clarification of “durable due to the retry.”

## Problem and evidence

The recovery Dockerfile currently compiles the integration executable, optional pacing checks and VOD library executable in one RUN. A later failure prevents that instruction's completed filesystem from becoming a reusable checkpoint. Its mutable Cargo target mount can retain partial work, but that is a separate cache with different correctness and garbage-collection properties.

One controlled attempt reached the existing thirty-minute compiler deadline after completing the integration executable. A subsequent attempt completed both executables in approximately seventeen minutes, then correctly refused runtime admission: the new library runner had not overlaid the runtime image's inherited data/cache volumes with bounded tmpfs. Neither attempt establishes passing Linux runtime qualification. The temporary Classifarr handoff restored the exact original container and image after each stop; Harmoniarr was untouched.

After correcting those overlays within the already approved VOD scope, the final retry reused completed compilation and passed all fresh Linux runtime groups and cleanup. The [VOD verification record](../design/VOD_OUTPUT_LIFECYCLE.md#verification-outcome) owns its source, image and receipts. VOD implementation `28f3f9c` and documentation `b658f6d` were pushed on main before the user confirmed the complete build-durability contract. Its subsequent implementation and evidence follow below.

The existing full-source digest is introduced in the runtime stage, so changing only a run identity or qualification script does not by itself invalidate the earlier compiler COPY layers. The durability improvement must preserve completed compiler checkpoints and validate mutable target reuse; merely adding another source hash is insufficient.

## Official research

Sources were discovered through MCP search and opened on October 26, 2026. Docker excludes modification time from COPY cache checks. Cargo uses both fingerprints and filesystem times for freshness. Consequently, copied or reverted source and an independently mutable target directory need an explicit input fence; copying timestamps is not a correctness proof. [Docker cache invalidation](https://docs.docker.com/build/cache/invalidation/), [Cargo fingerprint implementation documentation](https://doc.rust-lang.org/stable/nightly-rustc/cargo/compiler/fingerprint/index.html).

Cache mounts are optional acceleration: their contents can be absent, overwritten or garbage-collected. Locked sharing serializes cooperating writers; it is not a durability guarantee or protection from a privileged builder administrator. Cargo supports repeated package selectors for targeted cleanup. [Dockerfile cache mounts](https://docs.docker.com/reference/dockerfile/#run---mounttypecache), [cargo clean](https://doc.rust-lang.org/cargo/commands/cargo-clean.html).

Cargo's JSON artifact messages report target, profile, features and executable location. Select from those messages, then validate the pinned recipe's expected release path and copied hash; do not guess an executable by globbing the mutable target directory. Cached compilation is distinct from running a test. BuildKit may reclaim cache according to its storage policies, so checkpoint reuse is conditional on cache availability. [Cargo external tools](https://doc.rust-lang.org/cargo/reference/external-tools.html?highlight=artifact), [BuildKit garbage collection](https://docs.docker.com/build/cache/garbage-collection/).

## Alternatives

| Option | Benefit | Cost |
|---|---|---|
| Increase timeouts or repeat the whole build | Minimal implementation | Repeats completed work; weakens the existing bound if increased. |
| New target cache for every source digest | Simple source isolation | Discards compiled dependency reuse and grows cache storage. |
| Separate binary checkpoints plus guarded, locked target reuse | Retains completed executables and unchanged dependency work | Requires explicit input identity, interrupted-preparation handling and artifact validation. |
| Persist successful runtime results and skip tests on retry | Faster apparent completion | Does not test the new attempt's actual runtime, ownership or cleanup. |

Recommendation: separate completed integration/library compiler checkpoints, with guarded target reuse. Keep runtime execution fresh.

## Confirmed implementation contract

1. Split integration and VOD library compilation into separate completed Docker instructions. Copy each verified executable outside the mutable target mount before completing its instruction. Keep recipe-controlled compilation modes and independent runtime source/image checks.
2. Use a recipe-versioned, platform-scoped target-cache namespace with locked sharing. Before every uncached Cargo instruction, verify the prepared-cache identity against actual copied compiler inputs, recipe/helper bytes, installed toolchain/native package identity, profile and enabled modes. A missing, malformed or mismatched prepared receipt invalidates the owned workspace/path packages before compilation; unrelated dependency work remains reusable within a verified compatible compiler environment. An absent, corrupt or changed environment identity instead clears the owned release cache, because Cargo does not establish native-package compatibility merely from a source receipt. Publish receipts atomically only after successful invalidation. They record preparation, never compilation or test success. Same-input partial work may be resumed. Existing legacy caches are not silently trusted.
3. Validate a unique expected Cargo artifact, release path, executable status and feature parity for both executables. Record hashes alongside completed binaries. Reject ambiguous or mismatched artifacts.
4. Keep the owned Docker telemetry transport, current engine/boot identity checks, five-second read timeout, ten-second sample freshness, two Cargo jobs, one runtime test thread, thirty-minute compiler deadline, bounded logs and existing memory/container limits. Retain explicit failures and cleanup evidence. Do not add automatic qualification replay or change playback retry behavior.
5. Execute fresh restricted runtime containers on every attempt, including inherited-volume overlays. Test unchanged/changed/reverted inputs, absent/corrupt/interrupted preparation, completed-checkpoint reuse and refusal of stale artifacts. A valid build cache never substitutes for fresh positive runtime results and confirmed cleanup.

The stack remains pinned Rust/Cargo 1.99.1, Docker BuildKit, Cargo's existing caches, Alpine ash/core utilities already present in the compiler, and modular Node ES-module qualification helpers. Add no dependency, external artifact service, release or unrelated cache pruning. Optional repository configuration not copied into the compiler must not be represented as effective compiler configuration.

## Scope and next work

This change owns retry durability for the recovery/VOD qualification recipe. Cache retention remains subject to local BuildKit storage and garbage collection; unavailable checkpoints cause a rebuild. It does not promise bit-for-bit reproducible builds, resume failed runtime tests, alter the FFmpeg sandbox, enable viewer enforcement or migrate clients. The next product item remains the coordinated VOD admission/delivery/client migration described in the VOD authority.

The user confirmed this complete contract through the repository [review procedure](../../.agents/skills/grilling/SKILL.md) before implementation. Original VOD lifecycle work remains separately qualified.

## Implementation details and qualification plan

The recovery recipe separates integration and VOD library compiler instructions. Both copy a uniquely selected executable and its SHA256 outside the target mount; runtime construction verifies those copies. A recipe-versioned `duskcue-recovery-v1-${TARGETPLATFORM}` target cache uses locked sharing, alongside locked registry/git mounts. The new namespace deliberately does not trust legacy target contents: its initial cold population can still reach the existing deadline, while completed integration and same-input partial library work remain reusable on the next explicitly initiated attempt.

Focused shell helpers compute separate compiler-environment and source/preparation identities. The environment covers pinned image/platform, rustc/Cargo/C compiler/protoc versions, installed Alpine package records, release profile and compiler/linker configuration. The preparation identity adds actual compiler COPY inputs, recipe/helper bytes and qualification modes. A missing, malformed or mismatched prepared receipt causes package-scoped release cleanup of the four workspace packages and vendored plist when the environment remains verified; unknown or changed environments clear this recipe's release cache. Receipts are removed before their required cleanup and renamed atomically after success. Batched, NUL-delimited hashing propagates collection/read errors. No timestamp copying or unstable Cargo checksum feature is introduced. Configuration outside the existing COPY set remains outside effective compiler configuration.

A miniature real Cargo workspace tests absent/matching/corrupt receipts, failure/interruption during preparation, compatible dependency sentinel retention, unknown-environment invalidation, toolchain/mode identity changes, and changed/reverted backdated source that must produce the new verified binary. Artifact checks refuse feature, path, profile and ambiguous-candidate mismatches and verify real feature-enabled artifacts. Missing inputs and simulated per-file hashing failure refuse preparation. This tooling evidence is separately labeled; cached tooling checks do not replace fresh VOD runtime execution. The verification record below retains exact source, compiler log/cache transitions, real test results, unchanged resource guards and service restoration.

## Verified outcome

Final source inventory: `76390e26307d14083a1c2f7b365f84c2ee7be962c49f98cf8e7d0620cacfe78a`. Immutable amd64 runtime: `sha256:9614b5ad74e0f917264a22af628f23584bcc18c2214277e3a5bbea089a995832`, based on `sha256:f45eda1c8bbe3a9685196aae1200543601ec5efc2fb0f858ca863dd9d42ae8dc`. Product Rust and dependency locks remain unchanged from VOD implementation `28f3f9c`; its native/default-library/root qualification remains separately recorded in the VOD authority.

| Check | Observed result |
|---|---|
| Controlled interruption | Resource `4b577d35-babe-4370-8f3d-73af88e8f240`: integration instruction completed in 27 minutes 56 seconds; library instruction verified matching cache and began. At `23:16:57 UTC`, the agent deliberately stopped only the uniquely identified owned Docker build command/tree. The attempt remained failed, owned cleanup passed and Classifarr was restored healthy. No runtime pass was claimed. |
| Checkpoint reuse | Final resource `7f86b948-6577-47c6-a60f-c705121be6ea`: integration instruction reported `CACHED`; the library guard reported `matching`; its release target completed in six minutes one second. The completed 28-minute target was not rebuilt. Both copied executable hashes, FFmpeg and its bootstrap library verified during runtime construction. |
| Actual compiler inputs | All 712 compiler/tooling COPY inputs remained byte-for-byte identical between attempts; subset SHA256 `68741c781ea08f9025beb223297f4f7121facec509dfb746b0a36891361eada1`. The full qualification inventory changed only to move the proof-parser case into the existing CI test suite and remove its standalone test file. These files are outside the compiler COPY set. The final runtime validates the final complete inventory above. |
| Cargo/cache fault checks | Seventeen real Cargo checks passed, including actual rebuilt outputs for changed and reverted backdated source, compatible reuse, unknown-environment invalidation, failed/interrupted preparation, file-read failure and real feature-enabled artifact validation. The bounded exported tooling log contains exactly one successful result; it does not substitute for runtime execution. |
| Fresh managed Linux runtime | Seventeen VOD tests across six new containers passed, followed by the actual PostgreSQL/FFmpeg Stop/recovery test in 41.96 seconds. The held-file/root test observed its real thirty-second refusal and subsequent successful join. All owned runtime/database/probe cleanup passed. |
| Workflow/CI checks | Twenty-seven Node cases passed with two existing skips. The proof refusal case is included in `scripts/qualification/qualification.test.mjs`, already invoked by both existing hosted qualification jobs. No workflow permissions or trigger changed. |

Final local evidence: `.cache/testing-memory/vod-linux-7f86b948-6577-47c6-a60f-c705121be6ea/result.json`, `compiler.log`, `compile-cache-tests.log` and runtime logs. Memory receipt: `2026-10-26T23-18-04-713Z-b3d88ae0-925b-4beb-a2b3-564cc760f96a.jsonl`. Controlled interruption receipt: `.cache/vod-lifecycle/durability-interruption.json`; its memory receipt is `2026-10-26T22-47-41-320Z-28d5ae14-a6a4-43a9-8f76-090ccf40ad48.jsonl`. Compiler-input comparison: `.cache/vod-lifecycle/durability-compiler-input-comparison.json`. Final Node receipt: `2026-10-26T23-17-44-824Z-fffa7de9-33f0-4e3a-ab1a-0bc67dd31837.jsonl`.

Final formatting and staged whitespace pass. Copyright checks include all new source files: 905 files pass. Publication checks cover twelve staged task files, 625 local Markdown links with no missing targets, and zero supplied-private-value matches. Recomputed qualification source still matches the final runtime receipt after documentation changes.

Two earlier development attempts were intentionally stopped during review to correct per-file hashing-error propagation and conservative native-environment invalidation. They remain failed/interrupted evidence, not passing qualification. Classifarr was restored after every authorized handoff, with the same exact container/image; Harmoniarr was untouched. Neither guest allocation nor preflight/active reserve, freshness, timeout, Cargo jobs, log or runtime limits changed. No production bypass, private feed content, dependency update or playback retry change was introduced.

An interrupted Rust compiler unit can still rebuild; reuse applies to completed Cargo units and completed Docker checkpoints. Cache loss or incompatible compiler/native environment can still require a cold build. This qualification covers the normal VOD lane on amd64 Linux, not a new execution claim for ARM64, paced native/format/source-capability lanes or browser playback. Existing product strict-lint baseline and the two already documented low SvelteKit advisories remain separate; the [dependency-maintenance record](../security/DEVELOPMENT_TOOLING_SECURITY.md#remaining-dependency-maintenance) retains their applicability and major-version follow-up. A fresh GitHub MCP search found no open PR, so no additional PR patch or rebase applied.

Next: coordinated production VOD admission, direct/HLS delivery and online-client start/renewal/recovery/Stop outcomes, with bounded terminal receipts, before shared cap enforcement or Live TV play actions. Retain the confirmed cleanup and authority boundaries.

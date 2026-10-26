# Linux build durability

Status: proposed retry-durability contract, October 26, 2026. This supplements [VOD output lifecycle](../design/VOD_OUTPUT_LIFECYCLE.md). It concerns Linux Rust compilation and qualification, following the user's clarification of “durable due to the retry.”

## Problem and evidence

The recovery Dockerfile currently compiles the integration executable, optional pacing checks and VOD library executable in one RUN. A later failure prevents that instruction's completed filesystem from becoming a reusable checkpoint. Its mutable Cargo target mount can retain partial work, but that is a separate cache with different correctness and garbage-collection properties.

One controlled attempt reached the existing thirty-minute compiler deadline after completing the integration executable. A subsequent attempt completed both executables in approximately seventeen minutes, then correctly refused runtime admission: the new library runner had not overlaid the runtime image's inherited data/cache volumes with bounded tmpfs. Neither attempt establishes passing Linux runtime qualification. The temporary Classifarr handoff restored the exact original container and image after each stop; Harmoniarr was untouched.

After correcting those overlays within the already approved VOD scope, the final retry reused completed compilation and passed all fresh Linux runtime groups and cleanup. The [VOD verification record](../design/VOD_OUTPUT_LIFECYCLE.md#verification-outcome) owns its source, image and receipts. That successful retry demonstrates existing completed-layer reuse; the separate checkpoints and guarded target-cache architecture proposed below remain unimplemented pending confirmation.

The existing full-source digest is introduced in the runtime stage, so changing only a run identity or qualification script does not by itself invalidate the earlier compiler COPY layers. The durability improvement must preserve completed compiler checkpoints and validate mutable target reuse; merely adding another source hash is insufficient.

## Official research

Sources were discovered through MCP search and opened on October 26, 2026. Docker excludes modification time from COPY cache checks. Cargo uses both fingerprints and filesystem times for freshness. Consequently, copied or reverted source and an independently mutable target directory need an explicit input fence; copying timestamps is not a correctness proof. [Docker cache invalidation](https://docs.docker.com/build/cache/invalidation/), [Cargo fingerprint implementation documentation](https://doc.rust-lang.org/stable/nightly-rustc/cargo/compiler/fingerprint/index.html).

Cache mounts are optional acceleration: their contents can be absent, overwritten or garbage-collected. Locked sharing serializes cooperating writers; it is not a durability guarantee or protection from a privileged builder administrator. Cargo supports repeated package selectors for targeted cleanup. [Dockerfile cache mounts](https://docs.docker.com/reference/dockerfile/#run---mounttypecache), [cargo clean](https://doc.rust-lang.org/cargo/commands/cargo-clean.html).

## Alternatives

| Option | Benefit | Cost |
|---|---|---|
| Increase timeouts or repeat the whole build | Minimal implementation | Repeats completed work; weakens the existing bound if increased. |
| New target cache for every source digest | Simple source isolation | Discards compiled dependency reuse and grows cache storage. |
| Separate binary checkpoints plus guarded, locked target reuse | Retains completed executables and unchanged dependency work | Requires explicit input identity, interrupted-preparation handling and artifact validation. |
| Persist successful runtime results and skip tests on retry | Faster apparent completion | Does not test the new attempt's actual runtime, ownership or cleanup. |

Recommendation: separate completed integration/library compiler checkpoints, with guarded target reuse. Keep runtime execution fresh.

## Proposed implementation contract

1. Split integration and VOD library compilation into separate completed Docker instructions. Copy each verified executable outside the mutable target mount before completing its instruction. Keep recipe-controlled compilation modes and independent runtime source/image checks.
2. Use a recipe-versioned, platform-scoped target-cache namespace with locked sharing. Before every uncached Cargo instruction, verify the prepared-cache identity against actual copied compiler inputs, recipe/helper bytes, installed toolchain/native package identity, profile and enabled modes. A missing, malformed or mismatched receipt invalidates the owned workspace/path packages before compilation; unrelated dependency work remains reusable. Publish a prepared receipt atomically only after invalidation succeeds. It records preparation, never compilation or test success. Same-input partial work may be resumed. Existing legacy caches are not silently trusted.
3. Validate a unique expected Cargo artifact, release path, executable status and feature parity for both executables. Record hashes alongside completed binaries. Reject ambiguous or mismatched artifacts.
4. Keep the owned Docker telemetry transport, current engine/boot identity checks, five-second read timeout, ten-second sample freshness, two Cargo jobs, one runtime test thread, thirty-minute compiler deadline, bounded logs and existing memory/container limits. Retain explicit failures and cleanup evidence. Do not add automatic qualification replay or change playback retry behavior.
5. Execute fresh restricted runtime containers on every attempt, including inherited-volume overlays. Test unchanged/changed/reverted inputs, absent/corrupt/interrupted preparation, completed-checkpoint reuse and refusal of stale artifacts. A valid build cache never substitutes for fresh positive runtime results and confirmed cleanup.

The stack remains pinned Rust/Cargo 1.99.1, Docker BuildKit, Cargo's existing caches, POSIX shell/core utilities already present in the compiler, and modular Node ES-module qualification helpers. Add no dependency, external artifact service, release or unrelated cache pruning. Optional repository configuration not copied into the compiler must not be represented as effective compiler configuration.

## Scope and next work

This change owns retry durability for the recovery/VOD qualification recipe. Cache retention remains subject to local BuildKit storage and garbage collection; unavailable checkpoints cause a rebuild. It does not promise bit-for-bit reproducible builds, resume failed runtime tests, alter the FFmpeg sandbox, enable viewer enforcement or migrate clients. The next product item remains the coordinated VOD admission/delivery/client migration described in the VOD authority.

The repository [review procedure](../../.agents/skills/grilling/SKILL.md) requires confirmation of shared understanding before implementation of this newly clarified architecture. Original VOD lifecycle work remains separately authorized.

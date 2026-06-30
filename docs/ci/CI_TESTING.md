# CI & Testing Strategy

## Overview

This document defines how the project validates code changes, schema changes, backup recoverability, and release readiness. The goal is not only to catch regressions before merge, but to prove that the system can be restored, upgraded, and released safely.

This document complements:

- [BACKUP_RECOVERY.md](../operations/BACKUP_RECOVERY.md) - backup verification, WAL archival, PITR, and integrity checks
- [MIGRATION_STRATEGY.md](../design/MIGRATION_STRATEGY.md) - migration lifecycle and sqlx rules
- [MIGRATION_VERIFICATION.md](MIGRATION_VERIFICATION.md) - disposable Docker PostgreSQL migration verification
- [RELEASE_ENGINEERING.md](RELEASE_ENGINEERING.md) - release classes, rollback boundaries, and upgrade preflight gates
- [ADVANCED_DOC_DEFER_POLICY.md](../governance/ADVANCED_DOC_DEFER_POLICY.md) - which advanced governance docs are active now versus retained as deferred guidance
- [TRUSTED_AUTOMATION_INDEX.md](TRUSTED_AUTOMATION_INDEX.md) - maintainer-facing index for the advanced trusted-automation review set
- [TRUSTED_AUTOMATION_RELEASE_BLOCKERS.md](TRUSTED_AUTOMATION_RELEASE_BLOCKERS.md) - release-blocking change classes that require trusted-automation doc-set re-review
- [TRUSTED_AUTOMATION_MANUAL_VALIDATION.md](TRUSTED_AUTOMATION_MANUAL_VALIDATION.md) - dedicated manual validation step for release-blocking trusted-automation changes when the risk class warrants it
- [ENVIRONMENT_RUNNER_EMERGENCY_GOVERNANCE.md](ENVIRONMENT_RUNNER_EMERGENCY_GOVERNANCE.md) - who may freeze privileged workflows, reject pending protected jobs, and isolate self-hosted runner groups during incidents
- [SECRET_BROKERAGE_ROTATION.md](SECRET_BROKERAGE_ROTATION.md) - credential-source order and protected secret-bearing workflow rules
- [TRUSTED_RUNNER_STATE_DISPOSAL.md](TRUSTED_RUNNER_STATE_DISPOSAL.md) - disposal requirements for trusted self-hosted runner exception paths
- [TRUSTED_RUNNER_COMPROMISE_RESPONSE.md](TRUSTED_RUNNER_COMPROMISE_RESPONSE.md) - containment and rebuild flow when a trusted runner is suspected compromised
- [PROJECT_STRUCTURE.md](../design/PROJECT_STRUCTURE.md) - repository layout for server and clients
- [API_SECURITY.md](../security/API_SECURITY.md) - dependency auditing, SBOM generation, and API-layer safeguards
- [CLIENT_PACKAGING.md](CLIENT_PACKAGING.md) - desktop/mobile packaging smoke workflow, signing placeholders, and store-readiness notes

## Goals

1. Catch schema drift, query drift, and client regressions before merge.
2. Prove backups are restorable, not merely present.
3. Keep migration verification explicit for fresh installs and upgrades.
4. Build a legal, deterministic media-fixture corpus that exercises edge cases without relying on copyrighted production media.
5. Separate fast pull-request checks from heavier scheduled drills.
6. Publish release artifacts only after objective quality gates pass.
7. Keep CI workflow security least-privilege by default.

## Official Research Findings (May 2026)

### GitHub Actions workflow security and workflow design

- GitHub's workflow syntax docs state that when `permissions` are specified, any unspecified `GITHUB_TOKEN` scopes become `none`, which makes least-privilege workflow design practical and enforceable.
- GitHub's secure-use guidance recommends setting the default `GITHUB_TOKEN` to minimal access, then increasing permissions only for the specific jobs that need them.
- GitHub's secure-use guidance states that pinning third-party actions to a full-length commit SHA is the safest option for stability and security.
- GitHub's reusable-workflow docs state that reusable workflows referenced from other repositories are safest when pinned by commit SHA rather than tag or branch.
- GitHub recommends CODEOWNERS protection for `.github/workflows` so workflow changes require explicit review.
- GitHub recommends OpenID Connect for cloud authentication instead of long-lived secrets.
- GitHub's runner hardening guidance states that GitHub-hosted runners are ephemeral and isolated, while self-hosted runners can be persistently compromised by untrusted workflow code.
- GitHub's package-publishing example for secure artifact publication uses `contents: read`, `packages: write`, `attestations: write`, and `id-token: write`, then generates provenance with `actions/attest`.

### Cargo and sqlx validation

- Cargo's official docs state that `cargo test --workspace` tests all workspace members.
- Cargo's official docs describe `--locked` as the mode for deterministic CI, failing if `Cargo.lock` is missing or would change.
- sqlx-cli's official docs state that `cargo sqlx prepare --workspace` generates one shared `.sqlx` directory at the workspace root.
- sqlx-cli's official docs require `.sqlx` to be checked into version control if offline query checking is part of the build contract.
- sqlx-cli's official docs describe `cargo sqlx prepare --check --workspace` as CI-oriented verification that fails when `.sqlx` is out of date.
- sqlx-cli's docs also note that feature-gated or test-only queries should be covered by passing additional cargo flags such as `--all-targets --all-features`.

### PostgreSQL verification and upgrade rehearsal

- PostgreSQL's official docs for `pg_basebackup` and `pg_verifybackup` treat manifest validation as a useful integrity layer, but not as a substitute for real restore testing.
- PostgreSQL's official docs for `pg_upgrade` recommend testing deployment procedures with a schema-only copy of the old cluster plus dummy data.
- PostgreSQL's official docs for continuous archiving and PITR make WAL replay verification a first-class operational concern, not just a backup-storage concern.
- Docker Compose and PostgreSQL official image docs support disposable PostgreSQL verification with explicit volume cleanup, loopback-only port binding, `POSTGRES_INITDB_ARGS=--data-checksums`, and readiness gating via `pg_isready`. The local implementation is `scripts/verify-migrations.ps1`; see [MIGRATION_VERIFICATION.md](MIGRATION_VERIFICATION.md).

### Browser, web, and mobile test guidance

- Playwright's CI docs recommend uploading the HTML report as an artifact and using retries in CI.
- Playwright's trace docs recommend `trace: 'on-first-retry'` on CI so failures produce actionable traces without the overhead of tracing every successful run.
- Playwright's CI docs warn that traces, reports, and logs can contain sensitive data and should only be uploaded to trusted stores or encrypted.
- Vitest's official coverage docs recommend `vitest run --coverage`, use V8 coverage by default, and allow explicit coverage include/exclude rules for accurate CI reporting.
- Vitest's browser-mode docs state that CI should use the Playwright or WebdriverIO provider rather than the preview provider, and recommend Playwright as the default starting point.
- Flutter's testing overview defines three distinct layers: unit tests, widget tests, and integration tests.
- Flutter's integration-test docs show that `integration_test` can run on desktop, mobile devices, emulators, browsers, and Firebase Test Lab.

## Validation Model

The project uses four validation lanes. Each lane exists for a different failure mode and should not be collapsed into one oversized workflow.

| Lane | Trigger | Purpose | Runtime Budget |
|---|---|---|---|
| Fast PR | `pull_request`, protected-branch push | Catch normal code regressions quickly | Minutes |
| Mainline | Merge to `main` | Reconfirm full repo health on canonical branch | Tens of minutes |
| Scheduled Ops | Nightly / weekly / monthly | Verify restore, PITR, extended fixtures, and upgrade drills | Heavier |
| Release | Version tag / manual dispatch | Gate publishable artifacts and provenance | Heaviest |

### Design rule

Fast PR validation must stay fast enough to block bad merges without becoming so heavy that developers bypass it. Restore drills, extended fixture sweeps, and major-upgrade rehearsals belong in scheduled or release lanes, not every pull request.

## Test Scope by Surface

### 1. Rust server

Required layers:

- Unit tests for pure logic, parsing, policy evaluation, and domain invariants.
- Integration tests for HTTP handlers, auth flows, and database-backed services.
- Migration and startup tests that boot the server against disposable PostgreSQL instances.

Required baseline command:

```text
cargo test --workspace --locked
```

Additional rule:

- `--locked` is mandatory in CI for all Cargo commands that can mutate dependency resolution.

### 2. Web client

Required layers:

- `clients/web/tests/unit` for fast Vitest unit tests.
- `clients/web/tests/browser` for browser-mode component tests using Vitest with the Playwright provider.
- `clients/web/tests/e2e` for Playwright end-to-end flows.

Required baseline commands:

```text
vitest run --coverage
npx playwright test
```

Playwright CI defaults:

- `retries: 1`
- `trace: 'on-first-retry'`
- HTML report uploaded only to trusted artifact storage

### 3. Desktop wrapper

Tauri should inherit almost all behavioral coverage from the web client. The desktop-specific requirement is a thin smoke layer that proves:

- the shell boots,
- the bundled web app loads,
- server URL/bootstrap configuration is passed through correctly,
- playback shell integration does not crash at startup.

The desktop wrapper should not fork a second full UI test suite unless a platform-specific bug class proves it necessary.

Phase 16a adds `client-packaging.yml`, which runs Tauri debug package smoke builds on Linux, Windows, and macOS after building the shared web UI through the desktop static adapter path. Linux prerequisite installation follows Tauri's official WebKitGTK dependency guidance.

### 4. Flutter mobile client

Required layers:

- Unit tests for isolated logic.
- Widget tests for UI behavior.
- Integration tests under Flutter's `integration_test` flow for end-to-end behavior.

Required baseline commands:

```text
flutter test
flutter test integration_test/<suite>.dart
```

Device-matrix rule:

- Pull requests run unit and widget tests.
- Nightly and release lanes run selected integration suites on emulators and, when mobile parity matters, Firebase Test Lab.

Phase 16a adds pure Flutter tests for API error mapping, server URL validation, auth/session state, playback DTO/state helpers, notification handling, and quality payloads. The Android packaging lane runs `flutter analyze`, `flutter test`, the integration smoke test, and debug/release APK builds. The iOS lane validates plist/app-icon metadata, runs Flutter tests on macOS, and attempts a simulator build when the generated Xcode target exists.

## CI Workflow Security Posture

### Global rules

1. Default workflow permission should be either `contents: read` or `{}`.
2. Job-level permissions must be increased only where required.
3. Third-party actions must be pinned to full commit SHAs.
4. Reusable workflows from other repositories must also be pinned to full commit SHAs.
5. Workflow file changes require CODEOWNERS approval.
6. Untrusted pull requests use GitHub-hosted runners only.
7. Self-hosted runners are reserved for trusted branches, private scheduled drills, or tightly scoped release jobs.
8. Cloud authentication uses OIDC where possible, not long-lived cloud credentials.
9. Secret-bearing release and maintenance jobs should run through reviewed reusable workflows and protected environments, not through broad `secrets: inherit` patterns.
10. Emergency freeze authority must be limited to the designated governance roles defined in [ENVIRONMENT_RUNNER_EMERGENCY_GOVERNANCE.md](ENVIRONMENT_RUNNER_EMERGENCY_GOVERNANCE.md), and production bypass must not be used as a substitute for containment.

### Runner policy

| Scenario | Runner choice | Reason |
|---|---|---|
| Public or untrusted PR validation | GitHub-hosted | Ephemeral, lower persistence risk |
| Internal PR with private fixtures | GitHub-hosted preferred | Keeps secrets boundary small |
| Heavy restore drill using isolated infra | Trusted self-hosted or protected larger runner with disposal controls | Resource-heavy but still access-controlled |
| Publishing job | Protected runner plus environment approval if secrets are needed | Limits blast radius |

### Artifact handling rule

Playwright traces, HTML reports, browser screenshots, restore logs, and migration evidence can contain sensitive operational detail. They must be retained only in trusted artifact stores and should be encrypted if they leave GitHub's protected artifact flow.

Trusted self-hosted runner exception paths must also satisfy [TRUSTED_RUNNER_STATE_DISPOSAL.md](TRUSTED_RUNNER_STATE_DISPOSAL.md) so local credentials, workspaces, and builder state do not survive between privileged jobs.

## Recommended Workflow Set

### 1. `ci-pr.yml`

Trigger:

- `pull_request`
- push to protected development branches when needed

Purpose:

- fast merge protection

Required jobs:

1. Rust fmt/lint/type/test job.
2. `cargo test --workspace --locked`.
3. `cargo sqlx prepare --check --workspace -- --all-targets --all-features`.
4. Fresh database migration smoke against disposable PostgreSQL via `scripts/verify-migrations.ps1`.
5. Web unit tests with coverage.
6. Browser-mode component tests for the high-value web UI surface.
7. Playwright smoke E2E.
8. Flutter unit and widget tests.
9. Workflow/dependency security checks for action changes.

### 2. `ci-main.yml`

Trigger:

- push to `main`

Purpose:

- canonical branch health
- evidence generation for later release jobs

Additional jobs beyond PR:

- wider browser matrix when useful
- packaging smoke for Tauri and mobile builds
- SBOM generation

The desktop/mobile packaging smoke implementation is `client-packaging.yml`; detailed artifact, signing, and privacy placeholders are documented in [CLIENT_PACKAGING.md](CLIENT_PACKAGING.md).

### 3. `restore-drill.yml`

Trigger:

- weekly schedule
- manual dispatch

Purpose:

- prove the latest backup chain is actually restorable

Required concurrency rule:

```text
concurrency: restore-drill
```

This prevents overlapping restore drills from fighting over the same protected infrastructure or artifact retention space.

### 4. `release.yml`

Trigger:

- protected tag such as `v*`
- manual release dispatch for release candidates

Purpose:

- build, verify, attest, and publish

Publish-job permissions should be scoped to the job that actually publishes artifacts:

```text
contents: read
packages: write
attestations: write
id-token: write
```

## Migration Verification Strategy

Migration validation has to prove three different things:

1. A fresh install can build the schema from zero.
2. An existing installation can upgrade from the previous stable release.
3. SQLx metadata is in sync with the checked-in queries and schema.

### A. Fresh-install lane

Steps:

1. Start disposable PostgreSQL 18.
2. Run the full migration chain from an empty database.
3. Boot the server against that database.
4. Run minimal application smoke checks.
5. Assert the migration table is at head.

Failure meaning:

- broken migration ordering
- non-idempotent bootstrap SQL
- app startup assumptions no longer match a clean install

### B. Upgrade lane from previous stable

Steps:

1. Restore a sanitized database snapshot from the previous stable release, or build a schema-only old cluster with dummy data for upgrade rehearsal.
2. Run current migrations.
3. Boot current server binaries.
4. Run targeted post-migration assertions.
5. Confirm no backfilled or transformed data violates current API expectations.

Required assertions:

- schema version advanced exactly as expected
- core row counts survive upgrade
- nullable/default/backfilled columns match the migration contract
- representative read/write flows still pass after migration

### C. SQLx metadata lane

Required command:

```text
cargo sqlx prepare --check --workspace -- --all-targets --all-features
```

Policy:

- `.sqlx` is part of the repository contract and must be updated in the same change as new queries or schema changes.
- A migration change that does not refresh `.sqlx` is not mergeable.

### D. PostgreSQL major-upgrade rehearsal

This is not a normal PR check. It belongs to scheduled or release preparation workflows.

Required steps:

1. Build a schema-only old cluster with representative dummy data.
2. Run `pg_upgrade --check`.
3. Run full `pg_upgrade` in disposable infrastructure when the release branch is preparing for a PostgreSQL major transition.
4. Start the application against the upgraded cluster.
5. Run smoke and invariant checks.

## Restore Drill Strategy

Restore verification is split into three layers because each layer catches a different failure mode.

### Layer 1: backup-file verification

Cadence:

- every new base backup

Required tools:

- `pg_verifybackup`
- WAL archive continuity check (`wal-g wal-verify` in current design)

Purpose:

- catch incomplete or corrupted backup artifacts early

### Layer 2: scheduled full restore drill

Cadence:

- weekly minimum

Required steps:

1. Pull the latest verified base backup.
2. Restore it into a disposable PostgreSQL cluster.
3. Replay WAL to the intended recovery target.
4. Start PostgreSQL and verify readiness.
5. Run structural assertions against restored data.
6. Boot the application against the restored cluster.
7. Run smoke flows that touch auth, library listing, playback metadata, and admin health endpoints.

Required evidence:

- restore start/end time
- backup age
- WAL continuity result
- restored schema version
- row-count and invariant checks for critical tables
- pass/fail result for app smoke tests

### Layer 3: point-in-time recovery drill

Cadence:

- monthly minimum
- mandatory before major release milestones that alter recovery assumptions

Purpose:

- prove that recovery to an intermediate timestamp works, not just recovery to latest state

Required scenario:

1. Create a known marker transaction.
2. Create a second known marker transaction after it.
3. Recover to a time between them.
4. Assert the first marker exists and the second does not.

### Release-candidate restore rehearsal

Before a release candidate is promoted to stable:

1. Restore the previous stable snapshot.
2. Start the candidate build.
3. Run migrations.
4. Execute smoke and compatibility tests.
5. Record the full upgrade-and-restore evidence bundle.

This is the operational proof that the upgrade path described in release notes actually works.

## Media-Fixture Corpus Strategy

The fixture corpus must be representative enough to catch real media-server failures, but safe enough to store, redistribute, and hash in CI.

### Corpus rules

1. Do not commit copyrighted commercial media into the repository.
2. Prefer self-generated, procedurally generated, public-domain, or clearly licensed fixtures.
3. Every fixture must have a manifest entry with provenance, checksum, and expected outcomes.
4. The manifest, not the filename alone, is the contract.
5. Fast PR workflows use only the smallest corpus tier.
6. Larger and more operationally realistic tiers are allowed only in trusted scheduled or release workflows.

### Corpus tiers

#### Tier 0: repository-safe synthetic corpus

Stored in git. Small, deterministic, and fast.

Use cases:

- parser tests
- scanner path handling
- metadata matching edge cases
- subtitle and chapter parsing
- migration smoke that needs tiny media references

Representative coverage:

- common containers (`mp4`, `mkv`, `webm`)
- common codecs (`h264`, `hevc`, `aac`, `ac3`, `opus`)
- text subtitles
- multiple audio tracks
- chapter markers
- awkward file and folder naming
- duplicate or ambiguous naming cases
- intentionally damaged sample for error-path handling

#### Tier 1: trusted regression corpus

Stored outside git in protected object storage or release artifacts.

Use cases:

- nightly scanner regressions
- browser/player compatibility checks
- subtitle burn-in decision logic
- direct-play vs transcode policy validation

Representative coverage:

- HDR and SDR variants
- high-bitrate files
- multi-edition naming
- image-based subtitles
- extras folders and special-season layouts
- multiple providers and ID-tag variants

#### Tier 2: restore and upgrade corpus

Stored as protected database snapshots plus media manifests.

Use cases:

- restore drills
- migration rehearsals
- release-candidate upgrade proof

Contents:

- sanitized PostgreSQL snapshot
- expected library/media counts
- expected migration head version
- reference media manifest hashes

### Fixture manifest contract

Each fixture entry should record at least:

- fixture ID
- legal provenance or license
- SHA-256
- container/codec/subtitle characteristics
- expected scan outcome
- expected playback/transcode classification
- allowed workflow tier

This turns fixture validation into a deterministic machine check instead of informal test author knowledge.

## Release Quality Gates

Release quality gates are cumulative. A stable release cannot skip a failed lower-level gate.

### Pull request gate

Required green checks:

1. Rust workspace tests with locked dependencies.
2. SQLx metadata verification.
3. Fresh migration smoke on disposable PostgreSQL.
4. Web unit coverage run.
5. Web browser/component smoke.
6. Playwright smoke E2E.
7. Flutter unit/widget suite.

### Mainline gate

Required green checks:

1. All PR gates.
2. Packaging smoke for the artifacts built on `main`.
3. Security workflow checks for dependencies and workflow changes.
4. Artifact generation for reports and SBOMs.

### Release-candidate gate

Required green checks:

1. All mainline gates.
2. Previous-stable restore and upgrade rehearsal.
3. Extended fixture-corpus run.
4. Selected mobile integration tests.
5. Release notes and rollback notes prepared.
6. If the deferred trusted-automation path defined in [ADVANCED_DOC_DEFER_POLICY.md](../governance/ADVANCED_DOC_DEFER_POLICY.md) has been activated, any open release-blocking trusted-automation documentation review defined in [TRUSTED_AUTOMATION_RELEASE_BLOCKERS.md](TRUSTED_AUTOMATION_RELEASE_BLOCKERS.md) is completed before candidate promotion.
7. If the active release path includes a blocker class that requires human confirmation, the dedicated manual validation step defined in [TRUSTED_AUTOMATION_MANUAL_VALIDATION.md](TRUSTED_AUTOMATION_MANUAL_VALIDATION.md) has passed.

### Stable-release gate

Required green checks:

1. All release-candidate gates.
2. Most recent scheduled restore drill is green and within the release freshness window.
3. Publish artifacts are generated from the protected release workflow only.
4. SBOMs are generated for shipped artifacts.
5. Provenance attestations are generated for publishable artifacts.
6. Any environment-protected publish secrets or approvals have passed.
7. If the deferred trusted-automation path defined in [ADVANCED_DOC_DEFER_POLICY.md](../governance/ADVANCED_DOC_DEFER_POLICY.md) has been activated, no release-blocking trusted-automation change remains undocumented or marked pending re-review under [TRUSTED_AUTOMATION_RELEASE_BLOCKERS.md](TRUSTED_AUTOMATION_RELEASE_BLOCKERS.md).
8. Any required trusted-automation manual validation step defined in [TRUSTED_AUTOMATION_MANUAL_VALIDATION.md](TRUSTED_AUTOMATION_MANUAL_VALIDATION.md) has passed before protected publication.

### Freshness windows

Recommended defaults:

| Evidence | Maximum age at stable release |
|---|---|
| Last successful full restore drill | 7 days |
| Last PITR drill | 30 days |
| Last extended fixture sweep | 7 days |
| Last previous-stable upgrade rehearsal | Same release candidate cycle |

## Evidence Retention

Different evidence classes need different retention windows.

Durable release evidence lifecycle is governed by [RELEASE_ARTIFACT_RETENTION.md](RELEASE_ARTIFACT_RETENTION.md); the table below applies to raw CI evidence and short-lived workflow storage.

| Evidence | Retention |
|---|---|
| PR logs and reports | 14-30 days |
| Nightly restore evidence | 30-90 days |
| Release evidence bundle | Keep raw workflow artifacts only until the durable release evidence manifest is published; durable retention then follows [RELEASE_ARTIFACT_RETENTION.md](RELEASE_ARTIFACT_RETENTION.md) |
| Provenance attestations and SBOMs | Ship with the release artifact lifecycle |

The release evidence bundle should contain:

- workflow run identifiers
- artifact hashes
- SBOM references
- attestation references
- restore drill results
- migration rehearsal results
- test summary across server, web, desktop, and mobile surfaces

## Final Recommendation Stack

1. Fast PR gate: `cargo test --workspace --locked`, `cargo sqlx prepare --check --workspace -- --all-targets --all-features`, fresh migration smoke, Vitest coverage, browser-mode smoke, Playwright smoke, Flutter unit/widget tests.
2. Mainline gate: everything from PR plus packaging smoke, SBOM generation, and broader regression coverage.
3. Scheduled ops: weekly full restore drills, monthly PITR drills, and extended trusted fixture sweeps.
4. Release gate: previous-stable restore-and-upgrade rehearsal, protected publish workflow, SBOMs, and provenance attestations.
5. Security baseline: pinned SHA actions, minimal token permissions, CODEOWNERS on workflows, GitHub-hosted runners for untrusted code, OIDC for cloud auth.

## Official Sources

- GitHub Actions workflow syntax: https://docs.github.com/en/actions/reference/workflows-and-actions/workflow-syntax
- GitHub Actions secure use reference: https://docs.github.com/en/actions/reference/security/secure-use
- GitHub reusing workflow configurations: https://docs.github.com/en/actions/concepts/workflows-and-actions/reusing-workflow-configurations
- GitHub package publishing with attestations example: https://docs.github.com/en/packages/managing-github-packages-using-github-actions-workflows/publishing-and-installing-a-package-with-github-actions
- Cargo `cargo test`: https://doc.rust-lang.org/cargo/commands/cargo-test.html
- Cargo workspaces: https://doc.rust-lang.org/cargo/reference/workspaces.html
- sqlx-cli docs: https://docs.rs/crate/sqlx-cli/latest
- sqlx-cli README: https://docs.rs/crate/sqlx-cli/latest/source/README.md
- PostgreSQL `pg_basebackup`: https://www.postgresql.org/docs/current/app-pgbasebackup.html
- PostgreSQL `pg_verifybackup`: https://www.postgresql.org/docs/current/app-pgverifybackup.html
- PostgreSQL continuous archiving and PITR: https://www.postgresql.org/docs/current/continuous-archiving.html
- PostgreSQL upgrading a cluster: https://www.postgresql.org/docs/current/upgrading.html
- PostgreSQL `pg_upgrade`: https://www.postgresql.org/docs/current/pgupgrade.html
- Playwright CI intro: https://playwright.dev/docs/ci-intro
- Playwright Trace Viewer: https://playwright.dev/docs/trace-viewer
- Vitest coverage: https://vitest.dev/guide/coverage.html
- Vitest browser mode: https://vitest.dev/guide/browser/
- Flutter testing overview: https://docs.flutter.dev/testing/overview
- Flutter integration tests: https://docs.flutter.dev/testing/integration-tests

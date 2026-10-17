# Playback dependency security follow-up

## Design and evidence — October 17, 2026

This review extends the [playback compatibility foundation](../design/PLAYBACK_COMPATIBILITY.md) and the [locally applied dependency PR](../ci/PLAYBACK_DEPENDENCY_REVIEW.md). The first push reported 17 GitHub alerts. The subsequent authenticated GitHub dependency page, checked through browser MCP, showed one remaining moderate [Rustls alert](https://github.com/cloudbyday90/Duskcue/security/dependabot/23). The push banner and the later scan represent different points in time.

The broader RustSec check then identified nine vulnerability entries across h2, quinn-proto and ZIP, plus informational maintenance, soundness and yanked-version warnings. A lockfile match establishes an affected dependency version; runtime exploitability still depends on the APIs and features used. All selected releases were available by October 17, 2026.

## Recommendations and tradeoffs

| Dependency | Selected version | Reason and official source | Tradeoff |
|---|---|---|---|
| Rustls | 0.23.46 | The [October 14 release](https://github.com/rustls/rustls/releases/tag/v/0.23.46) fixes additional issues beyond the observed [handshake-boundary advisory](https://github.com/rustls/rustls/security/advisories/GHSA-2mjx-qc3c-rqvc) | Requires AWS-LC and webpki dependency updates and fresh compilation |
| h2 | 0.4.16 | [Release](https://github.com/hyperium/h2/releases/tag/v0.4.16), [empty DATA-frame advisory](https://rustsec.org/advisories/RUSTSEC-2026-0258) | Compatible patch; validate networking consumers |
| quinn-proto | 0.11.18 | [Release](https://github.com/quinn-rs/quinn/releases/tag/quinn-proto-0.11.18) covers the five reported QUIC advisories | Presence in Cargo.lock does not prove active HTTP/3; the workspace client enables HTTP/2 |
| ZIP | 9.0.5 | [October 17 release](https://github.com/zip-rs/zip2/releases/tag/v9.0.5) includes the fixed extraction line and subsequent parser fixes; [MSRV 1.88](https://raw.githubusercontent.com/zip-rs/zip2/v9.0.5/Cargo.toml) | Major upgrade changes the filename API and compression dependencies; retain existing default compression support |
| event-listener | 5.4.2 | [Tagged-listener soundness advisory](https://rustsec.org/advisories/RUSTSEC-2026-0221) | Compatible patch; the specific non-Send tag precondition was not established in the inspected SQLx path |
| spin | 0.9.9 | [Official registry metadata](https://crates.io/api/v1/crates/spin/0.9.9) identifies a compatible non-yanked replacement | Changes a transitive dependency within its existing range |
| chacha20 | 0.10.2 | [Maintainer changelog](https://github.com/RustCrypto/stream-ciphers/blob/master/chacha20/CHANGELOG.md) documents the RNG/backend fix | 0.10.1 is also yanked; use the verified non-yanked patch |

The final stack is compatible network/crypto patches, the supported ZIP major with bounded memory-only reading, fresh server/desktop verification, then another no-cache runtime build. Rustls and ZIP minimum versions are explicit in the workspace manifest. The Rustls update resolves aws-lc-rs 1.18.2, aws-lc-sys 0.46.0 and webpki 0.103.15; the workspace's selected TLS features retain the existing configuration.

## Subtitle archive contract

The inspected subtitle path reads ZIP entries into memory. It does not invoke archive filesystem extraction or apply archive permissions, so the three extraction-specific advisories do not establish those filesystem consequences in this call site.

The new focused archive module handles ZIP 9 filename errors and preserves SRT, WebVTT, ASS, SSA and TTML selection. It skips directories, symlinks and unenclosed paths, and never derives a filesystem destination from an archive entry. Product admission defaults are **32 MiB compressed archive input**, **1,024 entries** and **8 MiB for the selected uncompressed subtitle**. Both advertised size and actual bytes read are bounded. These are chosen product limits, not universal ZIP standards. The compressed-input check applies to bytes already downloaded; it does not establish a streaming HTTP response limit for subtitle providers.

Seven regression cases cover real deflated contents and supported extensions, malformed/empty archives, archive/count bounds, advertised-size rejection, the actual read cap, underreported compressed payloads and unsafe entry types/paths.

## Audit outcome and remaining debt

GitHub's authenticated dependency page was rechecked after the security push at **18:18 EDT on October 17, 2026** and reports [zero open Dependabot alerts](https://github.com/cloudbyday90/Duskcue/security/dependabot). Alert scanning is active; the page separately reports that automatic update PRs are paused. PR #5 remains open and unmerged.

The patched graph reports **zero vulnerability entries under the existing repository audit policy**, and no yanked-version warnings. The existing [RSA exception](../../.cargo/audit.toml) remains in effect for RUSTSEC-2023-0071. That policy records the unpatched WebAuthn dependency and its public-key verification usage; this result is not an exception-free bill of health.

The exception was rechecked against [RustSec database commit 999ec89d](https://github.com/rustsec/advisory-db/blob/999ec89d5d3f10f20be14750ec8c2d8b44e0ac9b/crates/rsa/RUSTSEC-2023-0071.md): `patched = []` remains current. The [maintainer tracking issue](https://github.com/RustCrypto/RSA/issues/626) is open, and the nonoptional WebAuthn/core/crypto-glue chain has no compatible stable patched replacement or RSA-removal feature. The public-key rationale remains the existing policy justification; this dependency review does not newly prove the absence of a reachable private-key timing oracle.

Twelve maintenance warnings remain: proc-macro-error, proc-macro-error2, rustybuzz, ttf-parser, five unic packages, zbus_names, zvariant and zvariant_derive. The font/SVG dependencies reach the server runtime; their advisories concern maintenance rather than a reported exploit. The other paths include build and desktop tooling.

The remaining [GLib soundness warning](https://rustsec.org/advisories/RUSTSEC-2024-0429) affects the Linux/BSD GTK/Tauri dependency line. GLib 0.18.5 has no patched release within that series; the fix begins at 0.20, while the [GTK parent](https://github.com/gtk-rs/gtk3-rs/blob/0.18/gtk/Cargo.toml) requires 0.18. Actual use of the affected VariantStrIter methods is not established. A coordinated desktop dependency migration or reviewed backport is a separate maintenance task. This component is outside the Docker server and Windows dependency paths.

Revalidation passed **870 server library tests**, including all seven archive regressions; server checking; and the dedicated Windows desktop Cargo check. Strict Clippy still reports its **13 documented baseline diagnostics**, with no added diagnostic. The security-updated no-cache Compose build passed, both explicit media tests passed against its immutable image, and the recreated container is healthy with HTTP 200 liveness/readiness/homepage responses and a 401 unauthenticated playback response. The instance is in initial setup mode, so authenticated live playback remains unqualified.

Evidence is retained in ignored `.cache/local-evaluation/playback-security-*`, `playback-cargo-audit-after-patches.*` and resource-runner logs. Final image identity and media outcomes are recorded in the [foundation verification](../design/PLAYBACK_COMPATIBILITY.md#verification-and-outcome).

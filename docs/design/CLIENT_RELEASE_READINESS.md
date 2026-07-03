# Client Release Readiness

## Purpose

This document is the Phase 16d release and store-readiness authority for Duskcue clients. It defines the release checklist shape that downstream desktop, mobile, TV, and console phases must complete before claiming beta or stable readiness.

Phase 16d does not store real signing credentials, provisioning profiles, package passwords, certificates, App Store Connect keys, Play upload keys, or store secrets. It defines the placeholders, evidence, and CI hooks that later platform phases must fill with secure secret storage.

## Official Source Review

Reviewed July 2, 2026.

| Platform area | Official sources reviewed | Release-readiness impact |
|---|---|---|
| Android mobile and Android TV | Android app signing, Android app versioning, Flutter Android deployment, Google Play Data Safety guidance | Android targets need immutable package names, Play App Signing/upload-key separation, monotonically increasing `versionCode`, user-visible `versionName`, Data Safety declarations, content rating, and Play track evidence. |
| Apple iOS, macOS, and tvOS | Apple App Store provisioning profiles, Xcode distribution/versioning, App Store Connect app information, App Privacy Details, Flutter iOS deployment, Tauri macOS signing/notarization | Apple targets need stable bundle IDs, distribution certificates, provisioning profiles, TestFlight/App Store metadata, privacy details, privacy policy or tvOS privacy text, unique build strings, and notarization for direct macOS distribution. |
| Windows desktop and Xbox | Microsoft Store policies, MSIX app certification, privacy/support information, age ratings, Tauri Windows signing | Windows/Xbox targets need package identity, Store policy/certification evidence, privacy policy when personal data is transmitted, IARC age ratings, MSIX/AppX signing expectations, and direct-download signing where applicable. |
| Fire TV | Amazon Appstore submission guidance and Fire TV app testing/submission guidance | Fire TV needs stable package names before live submission, Appstore metadata, content rating, privacy policy/data-use answers, Live App Testing evidence, and Android signing divergence notes. |
| Roku | Roku certification criteria, channel publishing, deep-linking requirements | Roku public channels need certification/pre-certification evidence, deep-link/Direct to Play behavior, channel package signing, privacy policy, content rating, and Roku Search/feed notes when public discovery is targeted. |
| Samsung Tizen | Samsung TV Seller Office publication process and launch checklist | Samsung needs Seller Office App ID tracking, Tizen certificate profiles, WGT package signing, content-policy/rating fields, privacy/credential-use answers, and certification evidence. |
| LG webOS | LG app approval process and app self checklist | LG needs IPK package metadata, app approval evidence, privacy and credential-use checklist answers, content test posture, and real-device/simulator evidence. |
| CI and supply chain | GitHub Actions artifacts, GitHub artifact attestations, GitHub SBOM export, CycloneDX tooling | CI placeholders need named artifacts, checksums, SBOM output, provenance/artifact attestations, signing/notarization hooks, and manual stable-release gates. |

## Decisions

### Release Checklist Pack

Task 11 adds the versioned release-readiness pack under [docs/api/fixtures/release/v1](../api/fixtures/release/v1/manifest.json). It is intentionally machine-readable so platform phases cannot hand-wave release requirements in prose.

The pack covers:

- per-platform app identity, package or bundle name, display name, store/distribution channel, signing identity, certificate/key placeholder, provisioning/profile placeholder, notarization or store-signing behavior, permission/capability declarations, privacy disclosures, age/content rating, and review notes;
- CI release placeholders for artifact names, build commands, signing hooks, notarization/store-processing hooks, SBOM outputs, provenance/attestation outputs, and initial release channels;
- versioning rules for server, web, desktop, Android, Apple, and TV client targets;
- release-channel mapping for local, internal, beta, and stable builds;
- release-blocking smoke checks and rollback/update expectations per platform;
- privacy, permission, and review-note requirements for self-hosted/no-bundled-catalog disclosure.

Run:

```bash
node scripts/verify-release-readiness.mjs
```

### Signing And Secret Handling

All signing material is represented as a placeholder. Actual material must live only in CI secrets, key vaults, local developer keychains, platform consoles, or platform-specific secure stores.

The repository may contain:

- package IDs, bundle IDs, app IDs, and display names;
- names of required CI secret slots;
- public review notes and privacy disclosures;
- placeholder build commands and artifact naming;
- checksums, SBOM outputs, and provenance metadata generated for release artifacts.

The repository must not contain:

- keystore files or passwords;
- Apple certificates, provisioning profiles, App Store Connect API keys, or notarization credentials;
- Roku package passwords;
- Tizen/webOS signing keys;
- Microsoft Store signing credentials;
- Amazon/Google app-store API credentials.

### Store Metadata And Privacy

Duskcue's release posture is self-hosted and no-bundled-catalog. Every platform review note must explain that users connect to their own Duskcue server, that Duskcue does not ship a media catalog, and that playback activity is sent only to the selected server.

Privacy declarations must cover the data Duskcue clients actually handle:

- account and session identifiers;
- selected server origin;
- stable client/device identifier where used;
- diagnostics and request IDs;
- push tokens where enabled;
- playback progress, watch state, QoE, and offline-download state where implemented.

Advertising or third-party tracking must stay absent from disclosures unless a future task adds those integrations.

### CI, SBOM, And Provenance

Task 11 defines placeholders only. Task 12 owns the executable CI/smoke harness. The release checklist requires that future build jobs produce:

- named platform artifacts;
- checksums;
- SBOM output, using SPDX or CycloneDX-compatible formats;
- GitHub artifact attestations or equivalent provenance records;
- signing/notarization/store-processing hooks wired to secure secret references;
- manual approval before stable promotion.

### Versioning

Server, web, desktop, mobile, and TV clients share a Duskcue SemVer-compatible human release train. Each platform also follows its native monotonic upload/build identifier:

- Android uses monotonically increasing `versionCode` and user-visible `versionName`.
- Apple platforms use stable bundle IDs, `CFBundleShortVersionString`, and unique `CFBundleVersion` build strings.
- Microsoft/MSIX, Roku, Samsung, LG, and desktop packages use their platform package versions and must preserve stable app/package identity across updates.
- All clients must declare the minimum compatible server API contract version before release.

### Smoke, Update, And Rollback

Release-blocking smoke tests build on the device-lab and conformance packs:

- install or launch a release-channel artifact;
- select a Docker deployment on `:48027`;
- authenticate or complete device linking;
- browse seeded library/media data;
- start playback and send heartbeat/stop;
- export redacted diagnostics;
- prove an update, rollback, staged rollout halt, or hotfix path.

Rollback expectations are platform-specific. Mobile and store platforms generally cannot assume user downgrade, so the default is staged rollout halt plus higher-build hotfix. Desktop direct downloads can keep previous signed installers available when local storage schema compatibility allows it.

## Relationship To Other Phase 16d Artifacts

| Artifact | Relationship |
|---|---|
| [CLIENT_PLATFORM_READINESS.md](CLIENT_PLATFORM_READINESS.md) | Lists release/store readiness as a mandatory gate for Phases 17-23. |
| [CLIENT_DEVICE_LAB.md](CLIENT_DEVICE_LAB.md) | Provides the representative hardware evidence required before beta/stable release claims. |
| [CLIENT_DIAGNOSTICS.md](CLIENT_DIAGNOSTICS.md) | Defines the redacted diagnostics bundle required by release smoke tests. |
| [CLIENT_ACCESSIBILITY_INPUT.md](CLIENT_ACCESSIBILITY_INPUT.md) | Supplies accessibility/input evidence for store review and certification. |
| [CLIENT_DESIGN_ASSETS.md](CLIENT_DESIGN_ASSETS.md) | Supplies app-icon and store artwork source assets that platform phases export into store-required dimensions. |
| [CLIENT_CONTRACTS.md](../api/CLIENT_CONTRACTS.md) | Links to the machine-readable release fixture pack and verifier. |

## Deferred Work

- Real store automation remains deferred until each platform client exists and has a platform account.
- Real signing/notarization credentials are never added to the repo.
- Task 12 will add CI and smoke harness jobs that consume this checklist.
- Platform phases own final store screenshots, localized listing copy, and certification uploads.

## Research Sources

- Android app signing: https://developer.android.com/studio/publish/app-signing
- Android app versioning: https://developer.android.com/studio/publish/versioning
- Flutter Android deployment: https://docs.flutter.dev/deployment/android
- Flutter iOS deployment: https://docs.flutter.dev/deployment/ios
- Apple App Store provisioning profile: https://developer.apple.com/help/account/provisioning-profiles/create-an-app-store-provisioning-profile/
- Apple Xcode distribution/versioning: https://developer.apple.com/documentation/xcode/preparing-your-app-for-distribution/
- Apple App Store Connect app information: https://developer.apple.com/help/app-store-connect/reference/app-information/app-information/
- Apple App Privacy Details: https://developer.apple.com/app-store/app-privacy-details/
- Tauri macOS signing: https://v2.tauri.app/distribute/sign/macos/
- Tauri Windows signing: https://v2.tauri.app/distribute/sign/windows/
- Microsoft Store policies: https://learn.microsoft.com/en-us/windows/apps/publish/store-policies
- Microsoft app certification process: https://learn.microsoft.com/en-us/windows/apps/publish/publish-your-app/msix/app-certification-process
- Microsoft privacy/support info: https://learn.microsoft.com/en-us/windows/apps/publish/publish-your-app/msix/support-info
- Microsoft age ratings: https://learn.microsoft.com/en-us/windows/apps/publish/publish-your-app/msix/age-ratings
- Amazon Appstore submission FAQ: https://developer.amazon.com/docs/app-submission/faq-submission.html
- Roku certification criteria: https://developer.roku.com/dev/docs/certification
- Roku channel publishing: https://developer.roku.com/dev/docs/channel-publishing-guide
- Roku deep linking: https://developer.roku.com/dev/docs/implementing-deep-linking
- Samsung application publication process: https://developer.samsung.com/tv-seller-office/application-publication-process.html
- Samsung launch checklist: https://developer.samsung.com/tv-seller-office/checklists-for-distribution/launch-checklist.html
- LG app approval process: https://webostv.developer.lge.com/distribute/app-approval-process
- LG app self checklist: https://webostv.developer.lge.com/distribute/app-self-checklist
- GitHub artifact attestations: https://docs.github.com/actions/security-for-github-actions/using-artifact-attestations/using-artifact-attestations-to-establish-provenance-for-builds
- GitHub workflow artifacts: https://docs.github.com/en/actions/tutorials/store-and-share-data
- GitHub SBOM export: https://docs.github.com/en/code-security/how-tos/secure-your-supply-chain/establish-provenance-and-integrity/export-dependencies-as-sbom
- CycloneDX tool center: https://cyclonedx.org/tool-center/

# Client Packaging and Release Smoke

## Overview

This document defines the Phase 16a packaging and release-smoke baseline for the desktop and mobile clients. It complements [CI_TESTING.md](CI_TESTING.md), [RELEASE_ENGINEERING.md](RELEASE_ENGINEERING.md), and [DESKTOP_MOBILE_CLIENTS.md](../design/DESKTOP_MOBILE_CLIENTS.md).

The goal is to prove that Duskcue's online desktop/mobile MVP can be built into platform artifacts before downstream TV/client-readiness work depends on it. Public store publication, paid signing identities, and notarized production releases remain release-operator work until credentials exist.

## Official Research Findings

Research used official vendor/project documentation current as of June 30, 2026.

| Area | Finding | Duskcue decision |
|---|---|---|
| Tauri build prerequisites | Tauri's Linux build path requires WebKitGTK and platform libraries in addition to Rust and Node. | The client packaging workflow installs Linux Tauri prerequisites in shell and runs `tauri build --debug` on Linux, Windows, and macOS. |
| Tauri icons/signing/updater | Tauri separates app icons, packaging, signing, and updater setup. The updater is plugin/config driven and should not be enabled without signing and release-channel policy. | Keep `duskcue://` protocol registration active, keep placeholder icons in the repo, document final branded icon generation as a release asset, and defer auto-update until signing material and channel policy exist. |
| Flutter CI/testing | Flutter's official testing docs split unit, widget, and integration tests; Flutter CLI builds Android APKs and iOS packages where platform SDKs allow. | CI installs the Flutter SDK from Flutter's official release manifest, runs `flutter analyze`, `flutter test`, the integration smoke test, Android debug APK, and Android release APK. |
| iOS CI constraints | iOS build and device signing require macOS/Xcode and a generated Xcode target/provisioning setup. | CI validates iOS metadata and runs Flutter tests on macOS. The simulator build runs automatically when a generated Runner native target exists; device/archive signing remains a protected release placeholder. |
| Android signing | Flutter/Android release builds need signing configuration before store upload. | CI proves release packaging can assemble. Production keystore paths, passwords, and Play signing are release secrets and are not committed. |
| Store privacy | Google Play Data safety and Apple App Privacy require declaration of collected/shared data and permission-sensitive behavior. | Duskcue documents local-network access, push tokens, server URL storage, media playback/watch progress, diagnostics, and notification metadata before store submission. |

## Workflow

`.github/workflows/client-packaging.yml` is the Phase 16a packaging smoke workflow.

| Job | Runner | Checks |
|---|---|---|
| `desktop` | `ubuntu-latest`, `windows-latest`, `macos-latest` | `npm ci` for web/desktop, Tauri static web build, `tauri build --debug` |
| `mobile-android` | `ubuntu-latest` | Official Flutter SDK install, `flutter pub get`, `flutter analyze`, `flutter test`, integration smoke test, debug APK, release APK |
| `mobile-ios` | `macos-latest` | Official Flutter SDK install, plist/app-icon metadata lint, `flutter pub get`, `flutter analyze`, `flutter test`, simulator build when the generated Xcode target exists |

Workflow security posture:

- `permissions: contents: read`
- Third-party GitHub Actions pinned to full commit SHA
- Flutter SDK installed from Flutter's official release manifests instead of an unpinned setup action
- No signing secrets, keystores, provisioning profiles, notarization credentials, Firebase credentials, APNs keys, or store API tokens are used in PR packaging smoke

## Desktop Packaging

Current package identity:

| Field | Value |
|---|---|
| Product name | `Duskcue` |
| Tauri identifier | `com.duskcue.desktop` |
| Version source | `clients/desktop/src-tauri/tauri.conf.json` |
| Protocol | `duskcue://` |
| Web bundle | `clients/web/build/client` generated through `clients/desktop/scripts/build-web-static.mjs` |

Smoke artifacts are runner-local Tauri debug bundles. They are validation evidence, not durable release payloads.

Release placeholders:

- Generate final `.ico`, `.icns`, and PNG icon set from the approved Duskcue brand asset before a public release.
- Configure Windows code signing with a protected certificate and timestamp server.
- Configure macOS Developer ID signing and notarization in a protected release workflow.
- Decide auto-update only after signing and release-channel policy are available. The MVP intentionally ships without updater configuration.
- Keep `duskcue://` protocol handling in every package and test deep-link routing before release.

## Mobile Packaging

Current package identity:

| Platform | Field | Value |
|---|---|---|
| Android | Application ID | `com.duskcue.mobile` |
| Android | Minimum SDK | `26` |
| Android | Permissions | `INTERNET`, `ACCESS_NETWORK_STATE`, `POST_NOTIFICATIONS` |
| iOS | Bundle URL scheme | `duskcue` |
| iOS | Local network text | `Duskcue connects to your self-hosted media server on your local network.` |

Release placeholders:

- Android production signing must use a protected keystore or Play App Signing. Keystore files and passwords must not be committed.
- iOS production signing requires Apple Team ID, bundle ID registration, provisioning profiles, push notification entitlement configuration, and archive/export settings.
- Firebase and APNs provider credentials remain server/admin configuration, not mobile repo secrets.
- Store metadata must disclose push notifications, local-network access, media playback, account/session data, server URL storage, diagnostics, and watch-progress behavior.

## Automated Test Coverage

Phase 16a Task 12 adds focused Flutter tests for the client logic that can run without a device:

| Test file | Coverage |
|---|---|
| `api_client_error_test.dart` | Dio/RFC 9457 error conversion, retry-after handling, fallback HTTP errors |
| `server_profile_test.dart` | Server URL canonicalization, exposed HTTPS requirement, `48028` rejection, saved profile round trip |
| `session_store_test.dart` | Auth/session state clearing while preserving selected server |
| `playback_models_test.dart` | Playback start/watch/segment DTO state-machine helpers |
| `notification_handling_test.dart` | SSE notification decoding, notification read state, push-device invalidation DTOs |
| `quality_service_test.dart` | Quality-mode playback payloads and default fallback behavior |

These tests do not replace device validation for passkeys, push receipt, local-network prompts, real HLS playback, Android notification permission UX, iOS APNs entitlement behavior, or mobile Wi-Fi/cellular transitions.

## Release Gate

A client release candidate cannot be promoted unless:

1. `client-packaging.yml` passes on the protected release branch or tag.
2. Desktop deep links, tray actions, native notifications, saved server state, and secure token revocation are manually smoke-tested against a Docker deployment on `:48027`.
3. Android debug and release packages are installed on a representative device or emulator and complete server selection, auth, browse/search/detail, playback resume, foreground SSE notification, push registration, and quality telemetry flows.
4. iOS simulator/device packaging is green in a macOS/Xcode environment with a generated Runner target, and physical-device push/passkey/local-network checks are completed before App Store submission.
5. Signing/notarization/store metadata is present for any artifact intended for public distribution.

## Official Sources

- Tauri Linux prerequisites: https://v2.tauri.app/start/prerequisites/#linux
- Tauri building: https://v2.tauri.app/distribute/
- Tauri icons: https://v2.tauri.app/learn/icons/
- Tauri updater: https://v2.tauri.app/plugin/updater/
- Flutter continuous delivery: https://docs.flutter.dev/deployment/cd
- Flutter Android deployment: https://docs.flutter.dev/deployment/android
- Flutter iOS deployment: https://docs.flutter.dev/deployment/ios
- Flutter testing overview: https://docs.flutter.dev/testing/overview
- Flutter integration tests: https://docs.flutter.dev/testing/integration-tests
- Google Play Data safety: https://support.google.com/googleplay/android-developer/answer/10787469
- Apple App Privacy details: https://developer.apple.com/help/app-store-connect/manage-app-privacy/app-privacy-details/

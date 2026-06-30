# Desktop and Mobile Clients

## Overview

This document is the authoritative Phase 16a design for Duskcue's desktop and mobile client foundations. It captures the June 30, 2026 research pass required before implementation and defines the decisions that downstream Phase 16a tasks must follow.

Phase 16a builds two online clients:

- **Desktop:** a Tauri 2 native wrapper around the existing SvelteKit web client, with a thin Rust shell for secure storage, native notifications, tray/menu behavior, file dialogs, deep links, and packaging.
- **Mobile:** a Flutter Android/iOS app with native server selection, auth, browsing, playback, foreground SSE, mobile push registration, and quality telemetry.

Offline downloads are explicitly out of scope for Phase 16a and remain Phase 16c.

## Research Summary

Research used official vendor/project documentation current as of June 30, 2026.

| Area | Finding | Duskcue decision |
|---|---|---|
| Tauri 2 security | Tauri v2 capabilities and plugin permissions are the boundary for frontend access to native APIs. Deep links must be configured statically on desktop/mobile, and desktop second-instance handling should use the single-instance integration. | Keep desktop as a minimal capability app. Add only the plugins required by a completed task: deep-link, dialog, notification, opener, OS/window state, single-instance, updater, and Stronghold or OS-backed storage for secrets. |
| Tauri updater/signing | Tauri's updater is plugin-based and distribution signing/notarization remains platform-specific. | Do not wire auto-update in the MVP unless the release channel and signing material exist. Phase 16a creates signing/notarization placeholders and a deferred updater decision. |
| Flutter structure | Flutter expects generated platform folders and standard Android/iOS deployment flows. Plugins are the normal route for native APIs, and platform channels are the escape hatch. | Replace the stub with a generated Flutter project under `clients/mobile/`; keep app code in `lib/`, platform manifests in `android/` and `ios/`, and integration tests under `integration_test/`. |
| Passkeys | Android passkeys are exposed through Credential Manager; iOS uses AuthenticationServices public-key credential APIs. | Mobile passkeys must use native platform APIs through a maintained Flutter plugin or a small platform-channel adapter. Do not depend on a WebView passkey flow for the mobile MVP. |
| Push | FCM HTTP v1 is the supported server send path with OAuth 2.0 service-account credentials; FCM token lifecycle guidance requires clients to refresh and servers to invalidate stale/rejected tokens. APNs token auth uses `.p8` key material and provider JWTs. UnifiedPush is Android/Linux only and depends on a distributor. | Implement provider clients in the server: FCM HTTP v1, APNs token auth, and UnifiedPush-as-endpoint delivery. Keep webhook as default; mobile-native push remains opt-in. Invalidate provider-rejected tokens without logging full tokens. |
| Playback | Android Media3 ExoPlayer has first-class HLS, track selection, analytics, media sessions, and background playback support. Apple HLS is native and AVFoundation/AVPlayer is the platform playback authority. Flutter `video_player` is useful for basic playback but may not expose all Duskcue controls and telemetry. | Use Flutter for UI/navigation, but treat Android Media3 and iOS AVPlayer/AVFoundation as the playback authorities. Select a plugin or build a thin native adapter only if it exposes HLS, audio/subtitle tracks, seek/resume, media-session controls, and QoE events. |
| Background execution | Android media playback must use foreground-service/media-session patterns; iOS background execution is constrained and media background behavior must use platform playback capabilities. | Maintain SSE only while foregrounded. Background delivery uses push. Playback lifecycle must stop/heartbeat cleanly on app background unless platform media playback is active. |
| Deep links | Android App Links and iOS Universal Links require HTTPS-hosted association files; custom schemes are easier but unverified. Tauri supports desktop custom schemes and mobile app/universal links. | Phase 16a uses `duskcue://` for desktop and mobile MVP. Verified `https://<server>/open/*` links are optional until the server can publish association files per deployment. Every link resolves through the server and revalidates auth/access before playback. |
| Local network and TLS | Android cleartext/private CA behavior is governed by network security config. iOS local-network access needs a usage description when local network discovery/access triggers the platform privacy gate. | Clients support manual `http://` LAN URLs in local mode, but exposed mode must use HTTPS. Document private CA/self-signed limitations and require explicit platform config for cleartext/private CA testing. |
| Store/privacy packaging | Google Play requires Data safety declarations; Apple requires App Privacy details. Flutter has separate Android and iOS release workflows. | Phase 16a adds manifest/privacy placeholders for local network, notifications, media playback, diagnostics, and server URL storage. Store release remains a smoke-tested placeholder unless signing material exists. |

## Pros, Cons, and Recommendation

### Desktop Strategy

**Option A: Tauri wrapper reusing SvelteKit**

Pros:

- Reuses the implemented web UI, Paraglide catalog, API client, auth screens, SSE store, and player workflow.
- Keeps native code focused on features browsers cannot do well: secure storage, tray/menu, OS notifications, file dialogs, protocol handling, and packaging.
- Matches the existing `clients/desktop` direction and `PROJECT_STRUCTURE.md`.

Cons:

- WebView behavior varies by OS.
- Static/SvelteKit build issues must be resolved carefully because the web app was originally built for adapter-node.
- Native passkeys and secure token storage need deliberate bridging if the web UI keeps browser assumptions.

**Option B: Separate native desktop UI**

Pros:

- Maximum native control and avoids WebView quirks.
- Could share more Rust types directly.

Cons:

- Duplicates nearly all existing web UI and i18n work.
- Larger maintenance surface before Duskcue has validated non-web usage.

**Recommendation:** Use Option A. The desktop app is a native shell around the web client, with a strict Tauri capability file and a small Rust command surface. Native UI is limited to tray/menu/notifications/dialogs.

### Mobile Strategy

**Option A: Full Flutter app with native platform adapters**

Pros:

- One UI codebase for Android and iOS while still allowing native passkey, push, and playback integrations.
- Works with app-store packaging, native push permissions, local-network prompts, and media-session controls.
- Keeps TV/platform client work independent of mobile toolkit choices.

Cons:

- Requires careful plugin selection and platform-channel escape hatches.
- More initial scaffolding than a WebView wrapper.

**Option B: Mobile WebView wrapper**

Pros:

- Fastest way to reuse the web app.
- Less initial UI work.

Cons:

- Poor fit for native passkeys, push tokens, background behavior, media sessions, and store-quality playback.
- Harder to deliver offline downloads in Phase 16c.

**Recommendation:** Use Option A. Generate a real Flutter Android/iOS project and reserve native platform channels for passkeys, playback, push, and secure storage when plugins do not expose required features.

## Phase 16a Implementation Decisions

1. **Server origin:** Desktop and mobile clients use the public `http(s)://<server>:48027` origin. They never target Docker's internal `48028` API listener.
2. **Token storage:** Bearer/session tokens, push tokens, signed URLs, and package/download secrets must not be stored in plaintext app preferences. Desktop uses Tauri Stronghold or OS-backed secure storage; mobile uses Android Keystore and iOS Keychain through a vetted Flutter plugin or platform channel.
3. **Client auth:** Desktop may reuse the web auth UI, but native token persistence must be outside browser localStorage. Mobile implements passkey, device-linking, re-auth code, invite/password fallback, logout, logout-all, and session deletion using native credential APIs where applicable.
4. **Passkey binding:** Android passkey work targets Credential Manager. iOS passkey work targets AuthenticationServices. The server's WebAuthn ceremonies remain the source of truth.
5. **Playback:** Use HLS for remux/transcode paths and direct file URLs only when the server decision engine returns Direct Play. Android playback must be Media3/ExoPlayer-backed; iOS playback must be AVPlayer/AVFoundation-backed. A Flutter package is acceptable only if it exposes required track, lifecycle, and telemetry controls.
6. **Foreground real time:** SSE is foreground-only on mobile. On resume, mobile clients reconnect with replay where possible and refresh notification/playback state through REST if replay is unavailable.
7. **Push:** FCM, APNs, and UnifiedPush provider clients are server-side Phase 16a work that completes the existing structured push stub. Client registration calls `POST /api/v1/user/push-devices` on login and app launch, with heartbeat refresh and re-registration after invalidation.
8. **Deep links:** `duskcue://` is the MVP protocol for desktop/mobile. Verified HTTPS links need `.well-known/assetlinks.json` and `apple-app-site-association`, so they are optional until server/operator support exists.
9. **Local network:** Manual server URL entry is required. Discovery/QR/link handoff is optional. Local HTTP is allowed only for local/VPN deployments; exposed mode requires HTTPS.
10. **Store readiness:** Phase 16a adds package IDs, permissions, signing placeholders, app icons placeholder, privacy declarations, and CI smoke builds. Actual public-store publication is not required for Phase 16a completion.

## Required Phase 16a Outputs

Task 1 must produce:

- A buildable `clients/desktop` Tauri 2 shell with valid `tauri.conf.json`, default capabilities, Rust entrypoint, icons placeholder, scripts, and a static/shared web build path.
- A generated `clients/mobile` Flutter project with Android/iOS folders, package IDs, lints, tests, icons placeholder, routing/state/http/storage/playback/push dependency baseline, and CI-friendly commands.

Task 2 must produce:

- A documented route/DTO inventory for desktop/mobile.
- A chosen contract source of truth for Flutter DTOs and client error mapping.
- Typed client handling for RFC 9457 Problem Details.

Tasks 3-12 must follow the decisions above and update this document with implementation notes as each task completes.

## Relationship to Other Documents

| Document | Relationship |
|---|---|
| [PROJECT_STRUCTURE.md](PROJECT_STRUCTURE.md) | Defines the monorepo client folders and build relationships. This document refines the Phase 16a client layout and native adapter decisions. |
| [AUTH.md](AUTH.md) | Defines WebAuthn, device linking, invitations, sessions, and re-auth. This document defines how desktop/mobile consume those auth flows. |
| [STREAMING.md](STREAMING.md) | Defines HLS/direct/remux/transcode behavior. This document defines mobile playback authority and client responsibilities. |
| [REAL_TIME_PUSH.md](REAL_TIME_PUSH.md) | Defines SSE foreground events. This document defines mobile foreground-only SSE behavior and polling fallback expectations. |
| [MOBILE_PUSH.md](MOBILE_PUSH.md) | Defines push channels and token lifecycle. This document binds Phase 16a client/provider implementation to that design. |
| [SECURITY.md](../security/SECURITY.md) | Defines network modes, TLS, and token expectations. This document applies them to desktop/mobile clients. |
| [API_SECURITY.md](../security/API_SECURITY.md) | Defines validation, BOLA, and secret-handling constraints that client deep links and stored credentials must respect. |
| [BUILD_ORDER.md](../../BUILD_ORDER.md) | Phase 16a task list and completion criteria. |

## Research Sources

- Tauri capabilities: https://v2.tauri.app/security/capabilities/
- Tauri deep linking: https://v2.tauri.app/plugin/deep-linking/
- Tauri updater: https://v2.tauri.app/plugin/updater/
- Tauri Stronghold: https://v2.tauri.app/plugin/stronghold/
- Flutter install/project docs: https://docs.flutter.dev/install
- Flutter packages/plugins: https://docs.flutter.dev/packages-and-plugins/using-packages
- Flutter video playback cookbook: https://docs.flutter.dev/cookbook/plugins/play-video
- Flutter Android release: https://docs.flutter.dev/deployment/android
- Flutter iOS release: https://docs.flutter.dev/deployment/ios
- Android Credential Manager: https://developer.android.com/identity/credential-manager
- Apple passkeys: https://developer.apple.com/documentation/authenticationservices/supporting-passkeys
- FCM HTTP v1: https://firebase.google.com/docs/cloud-messaging/send/v1-api
- FCM token management: https://firebase.google.com/docs/cloud-messaging/manage-tokens
- FCM Flutter setup: https://firebase.google.com/docs/cloud-messaging/flutter/get-started
- APNs token auth: https://developer.apple.com/documentation/usernotifications/establishing-a-token-based-connection-to-apns
- UnifiedPush intro/specs: https://unifiedpush.org/developers/intro/
- UnifiedPush ntfy distributor: https://unifiedpush.org/users/distributors/ntfy/
- Android Media3 HLS: https://developer.android.com/media/media3/exoplayer/hls
- Android Media3 background playback: https://developer.android.com/media/media3/session/background-playback
- Android foreground services: https://developer.android.com/develop/background-work/services/fgs
- Apple HTTP Live Streaming: https://developer.apple.com/streaming/
- Android App Links: https://developer.android.com/training/app-links
- Apple Universal Links: https://developer.apple.com/documentation/xcode/supporting-universal-links-in-your-app
- Android network security config: https://developer.android.com/privacy-and-security/security-config
- Apple local network privacy key: https://developer.apple.com/documentation/bundleresources/information-property-list/nslocalnetworkusagedescription
- Google Play Data safety: https://support.google.com/googleplay/android-developer/answer/10787469
- Apple App Privacy details: https://developer.apple.com/help/app-store-connect/manage-app-privacy/app-privacy-details/

# Client Device Lab

## Purpose

This document is the Phase 16d device lab and compatibility matrix contract for Duskcue clients. It defines the minimum and representative devices that downstream desktop, mobile, TV, and console phases must test against, the media capability fields they must report, the Docker-backed manual smoke scripts they must run, and the line between release-required hardware and best-effort compatibility targets.

The machine-readable pack starts at [docs/api/fixtures/device-lab/v1/manifest.json](../api/fixtures/device-lab/v1/manifest.json).

## Official Source Review

Reviewed July 2, 2026.

| Platform area | Official sources reviewed | Device-lab impact |
|---|---|---|
| Android mobile and Android TV | [Android supported media formats](https://developer.android.com/media/platform/supported-formats), [Media3 ExoPlayer supported formats](https://developer.android.com/media/media3/exoplayer/supported-formats), [Android TV app creation](https://developer.android.com/training/tv/get-started/create) | Android clients must treat codec support as device-dependent, use Media3/ExoPlayer for mobile/TV playback, validate HLS on the actual device class, and keep TV D-pad/focus evidence separate from mobile touch evidence. |
| Apple iOS, macOS, and tvOS | [Apple HLS authoring specification](https://developer.apple.com/documentation/http-live-streaming/hls-authoring-specification-for-apple-devices), [HTTP Live Streaming overview](https://developer.apple.com/streaming/), [AVFoundation HLS variants](https://developer.apple.com/videos/play/wwdc2021/10143/), [TestFlight](https://testflight.apple.com/) | AVFoundation/AVKit HLS playback and media selection are native validation paths. Simulators are useful for layout/auth but do not prove physical hardware decode, HDR, Dolby Vision, HDMI audio, Top Shelf, or local-network prompts. |
| Amazon Fire TV | [Connect to Fire TV through ADB](https://developer.amazon.com/docs/fire-tv/connecting-adb-to-device.html), [Install and Run Your App](https://developer.amazon.com/docs/fire-tv/installing-and-running-your-app.html), [Test on Fire OS 14+ TV devices](https://developer.amazon.com/docs/app-testing/test-on-fire-os-14.html) | Fire TV requires physical-device ADB/sideload smoke, remote navigation, installation/uninstallation behavior, and Fire OS-specific divergence notes instead of assuming Google TV behavior is enough. |
| Roku | [Certification criteria](https://developer.roku.com/dev/docs/certification), [Deep linking](https://developer.roku.com/dev/docs/implementing-deep-linking), [Direct to Play](https://developer.roku.com/dev/docs/direct-to-play) | Roku release claims require physical-device evidence, deep-link/Direct to Play validation, model/OS capture, and certification-oriented pre-checks. |
| Samsung Tizen | [Media specifications](https://developer.samsung.com/smarttv/develop/specifications/media-specifications.html), [General specifications](https://developer.samsung.com/smarttv/develop/specifications/general-specifications.html), [Remote Test Lab](https://developer.samsung.com/remote-test-lab) | Samsung emulator evidence is not sufficient for AVPlay, HDR, audio, or model-year media claims. Physical TV or Remote Test Lab evidence is required before release. |
| LG webOS | [Streaming Protocol and DRM](https://webostv.developer.lge.com/develop/specifications/streaming-protocol-drm), [webOS TV emulator](https://webostv.developer.lge.com/develop/tools/emulator-introduction), [Developer Mode app](https://webostv.developer.lge.com/develop/getting-started/developer-mode-app) | LG validation must distinguish emulator UI evidence from physical webOS TV playback evidence, including Magic Remote pointer and D-pad behavior. |
| Flutter mobile | [Flutter integration testing](https://docs.flutter.dev/testing/integration-tests) | Flutter integration tests cover mobile UI/auth flows, but platform release claims still require native Android/iOS device evidence for media, permissions, and storage behavior. |
| Windows and Xbox | [Windows Device Portal overview](https://learn.microsoft.com/en-us/windows/uwp/debug-test-perf/device-portal), [Device Portal API](https://learn.microsoft.com/en-us/windows/uwp/debug-test-perf/device-portal-api-core), [Xbox Device Portal](https://learn.microsoft.com/en-us/xbox/gdk/docs/tools/tools-console/wdp/wdp), [UWP media playback on Xbox](https://learn.microsoft.com/en-us/shows/one-dev-minute/media-playback-in-uwp-app-xbox) | Windows and Xbox compatibility depends on the selected app shell, declared capabilities, installed codecs, GPU/display chain, and console generation. Xbox Series S/X is the release-required console class for public Xbox claims; Xbox One-family support is best-effort unless explicitly claimed. |

## Decisions

### Device Lab Matrix

Phase 16d defines a versioned device lab pack under `docs/api/fixtures/device-lab/v1`. The pack is a contract for future platform phases, not proof that every device is already owned or automated.

The required platform IDs are:

- `android_mobile`
- `ios_mobile`
- `windows_desktop`
- `macos_desktop`
- `linux_desktop`
- `android_tv_google_tv`
- `fire_tv`
- `roku`
- `samsung_tizen`
- `lg_webos`
- `apple_tvos`
- `xbox`

Every platform entry must track:

- OS or firmware version baseline
- browser/webview/runtime engine
- video/container codec capabilities
- HLS support
- HDR support
- audio support
- subtitle support
- remote/input behavior
- storage constraints
- known platform limitations

### Media Capability Posture

The shared conservative direct-play baseline is H.264 High Profile 8-bit 1080p video, AAC stereo audio, MP4 container, and WebVTT/SRT subtitles. Advanced codecs, HDR formats, object-based audio, and image/styled subtitles are device-profile inputs, not global platform assumptions.

Clients must report observed capabilities back through the quality/device profile system. When a device fails a direct-play or subtitle path, future platform code should update capability evidence or prompt a capability wizard run instead of adding special-case playback branches.

### Manual Smoke Target

All manual smoke scripts target the Docker deployment at:

```text
http://<server>:48027
```

Use the LAN-reachable host address from the target device. Do not use Docker's internal API port. Exposed-mode release claims require HTTPS and OS-trusted certificates, but local/VPN smoke tests may use local HTTP when the platform phase documents that mode.

The required smoke steps are:

- Docker readiness through `/health/ready`
- server selection
- auth through device-linking or platform login
- library browse
- playback start
- resume/heartbeat/stop
- subtitle/audio selection
- diagnostics export
- logout and credential cleanup

### Release Required vs Best Effort

Simulator/emulator evidence is valid for layout, navigation, auth, fixture parsing, and some packaging checks. It is not valid evidence for hardware codec support, HDR, surround audio, HDMI/display-chain behavior, remote control behavior, Top Shelf/launcher surfaces, or TV storage constraints.

Release-required hardware means a platform phase cannot claim release readiness for that platform without passing the manual smoke script or explicitly documenting a release-blocking gap. Best-effort hardware is useful compatibility evidence but does not block first release unless the platform phase claims that hardware class.

## Machine-Readable Artifacts

| Artifact | Purpose |
|---|---|
| [manifest.json](../api/fixtures/device-lab/v1/manifest.json) | Required platform list, capability fields, smoke steps, Docker target, and fixture inventory. |
| [device-matrix.json](../api/fixtures/device-lab/v1/device-matrix.json) | Minimum and representative devices, OS/runtime tracking, input behavior, storage constraints, and known limitations. |
| [media-capability-matrix.json](../api/fixtures/device-lab/v1/media-capability-matrix.json) | HLS, codec, HDR, audio, subtitle, input, storage, and platform-limitation coverage. |
| [manual-smoke-scripts.json](../api/fixtures/device-lab/v1/manual-smoke-scripts.json) | Common and per-platform manual smoke scripts against `:48027`. |
| [release-validation-policy.json](../api/fixtures/device-lab/v1/release-validation-policy.json) | Release-required and best-effort hardware policy per platform. |
| [known-platform-limitations.json](../api/fixtures/device-lab/v1/known-platform-limitations.json) | Known limitations, workarounds, and fallback behavior. |
| [hardware-gap-report.json](../api/fixtures/device-lab/v1/hardware-gap-report.json) | Allowed Phase 16d hardware gaps and downstream release blockers. |

Run:

```bash
node scripts/verify-device-lab.mjs
```

The verifier checks required platform coverage, required capability fields, Docker port `48027`, smoke-step coverage, release-required/best-effort classifications, hardware-gap coverage, and fixture leak patterns.

## Relationship to Capability Profiles

[QUALITY_MANAGEMENT.md](QUALITY_MANAGEMENT.md) remains the authority for how Duskcue stores device profiles and chooses direct play, remux, or transcode. This device lab pack defines which device evidence should feed those profiles.

The capability wizard should use this matrix to decide which sample files matter for a platform family:

- H.264/AAC/MP4 for baseline direct play
- HEVC 8-bit and 10-bit for modern mobile/TV/desktop checks
- AV1 for newer device classes
- HDR10/HLG/Dolby Vision where hardware/display-chain support is claimed
- AAC/AC3/EAC3 and passthrough validation where surround output is claimed
- WebVTT/SRT plus ASS/PGS fallback behavior for subtitle decisions

## Relationship to TV Platform Surfaces

[TV_PLATFORM_SURFACES.md](TV_PLATFORM_SURFACES.md) defines the feed, stable platform IDs, deep-link resolution, and living-room adapter expectations. This device lab pack defines the hardware and manual evidence required before those adapters can claim platform readiness.

For TV/console phases, device lab evidence must include:

- app-local TV surface browse
- deep-link resolve and access revalidation
- D-pad/controller focus behavior
- playback start/resume/heartbeat/stop
- subtitle/audio behavior
- diagnostics export redaction
- model, OS/firmware, and app build identifier

## Deferred Work

- Automated device-farm integration is deferred to Phase 16d Task 12 or platform-specific CI work.
- Real store submission/certification evidence belongs to Phase 16d Task 11 and the downstream platform phases.
- Partner-gated devices and storefront surfaces remain advisory until access is confirmed.

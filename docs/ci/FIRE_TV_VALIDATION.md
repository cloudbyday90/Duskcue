# Fire TV Validation

## Purpose

This runbook defines the physical-device evidence required for the Phase 18 Fire OS target. It applies to `clients/tv/fire/`, whose Appstore application ID is `com.duskcue.firetv` and whose supported baseline is Fire OS 7/API 28 and later.

CI verifies the source, manifest, unit tests, lint, and debug APK. It cannot prove Appstore availability, Fire remote/media-button behavior, audio focus, codec/HDR/audio outcomes, VoiceView, standby/resume, or Amazon launcher/personalization behavior. A Fire TV device is required for every such claim.

## Repository Checks

```powershell
node scripts/verify-fire-tv-target.mjs
./scripts/verify-migrations.ps1 -RunFireRegistryTests
Set-Location clients/tv/fire
./gradlew.bat :app:testDebugUnitTest :app:lintDebug :app:assembleDebug
```

The debug APK is an internal diagnostic artifact. It is not an Appstore submission and does not prove Appstore review, signing, Amazon SDK approval, catalog acceptance, or availability on a customer device.

## Physical Device Matrix

Run the baseline on at least one Fire OS 7/8/14 family device and one Fire OS 16 device before claiming the corresponding support range. Record model, Fire OS version, app build version, display connection, and audio route outside the repository. Do not commit server URLs, account names, profile names, raw media IDs/titles, catalog IDs, screenshots containing household information, tokens, signed URLs, or ADB bug reports.

| Area | Required observation | Evidence boundary |
|---|---|---|
| Install and launcher | Sideload/debug install for engineering; Appstore install/update only after signing/store setup; visible Fire launcher entry. | Model/OS/build/pass-fail summary only. |
| Profile picker | No profile-scoped content before selection; remembered profile only after account authentication; a Kids-to-standard switch requires server parent unlock. | Never capture or retain PINs, profile names, session data, or artwork. |
| Playback and account state | Device link/login, browse, direct Duskcue deep link, latest server resume, stop/heartbeat, account change and logout cleanup. | Use synthetic/non-sensitive test media. |
| Remote and audio focus | D-pad, Back, Play/Pause, FF, Rewind, Menu, media button receiver, external audio interruption/resumption. | Record behavior, not microphone/audio content. |
| Accessibility and video | VoiceView focus/labels, captions/audio track selection, overscan, HLS, codecs/HDR/audio actually advertised by the release. | Do not infer support from a decoder list alone. |
| Lifecycle and diagnostics | Standby/resume, app restart, server/session revocation, exported diagnostics redaction. | Export only an intentionally sanitized diagnostics bundle; do not commit it. |
| Watch Activity/catalog | Only after Amazon grants access, the Fire customer opts in, an exact accepted catalog ID exists, and a standard Duskcue profile is active. Validate start/seek/pause/resume/60-second/exit signals and launcher behavior. | Kids and ambient activity must not be reported. A missing or delayed home row is not proof of an app failure without Amazon integration diagnostics. |

## Explicitly Deferred

- Customer-opted-in physical Watch Activity validation and partner enablement (the Task 2 compile-only SDK adapter and fixture/unit coverage are implemented);
- real Amazon catalog/CDF acceptance, EMBER, global search, Alexa discovery, and catalog deep-link admission (the server-owned registry and repository eligibility checks are implemented);
- Vega implementation;
- Amazon catalog launch wiring and live accepted-catalog verification remain pending partner requirements; repository tests cover the authenticated Duskcue-owned launch path, fresh resume, scope/intent replacement, and history-preserving cancellation;
- Appstore signing, registration, listing metadata, screenshots, review access, and production submission.

These remain feature/partner/release gates in [FIRE_TV.md](../design/FIRE_TV.md), not failures of the app-local baseline.

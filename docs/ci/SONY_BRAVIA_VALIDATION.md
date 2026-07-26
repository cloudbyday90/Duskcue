# Sony BRAVIA Google TV / Android TV Validation

## Status

Phase 17 Task 14 is repository-ready but physical evidence is pending. The checked-in fixture, bounded Android capability export, and ADB preflight tool make the eventual test repeatable; they do not claim that a Sony BRAVIA, Google Play listing, HDMI device, AVR, display, remote microphone, network, Watch Next row, or voice-discovery route has been observed.

The machine-readable authority is [sony-bravia-validation.json](../api/fixtures/device-lab/v1/sony-bravia-validation.json). Run `node scripts/verify-sony-bravia-validation.mjs` for static drift and `node scripts/sony-bravia-validation.mjs --plan` for the physical sequence.

## Research Outcome

| Area | Official finding | Validation decision |
|---|---|---|
| Store visibility | Sony states that Google Play displays only apps compatible with a specific TV, and availability can vary by model and country. | Test intended signed-track visibility and install separately on a Sony BRAVIA Google TV and a Sony BRAVIA Android TV; package presence or ADB installation is not listing evidence. |
| Google TV and Android TV UI | Sony documents distinct installation paths for Google TV and Android TV. | Record the observed UI generation and do not use a Google TV result as an older Android TV result. |
| HDR | Sony's model table ties HDR support to the exact model year and HDMI input. | Treat HDR10 and Dolby Vision as actual route observations, with TV or AVR readout and a fallback result rather than a codec-API claim. |
| Audio | Sony documents ARC/eARC, pass-through, receiver, cable, and settings dependencies; incompatible routes can downmix or produce no sound. | Test the exact audio route for PCM or AAC fallback, AC-3, E-AC-3 or Atmos, and DTS where available; record not_supported rather than inferring passthrough. |
| Voice | Sony states that voice support depends on model, paired remote, internet, Google account, country, language, and app/search support. | Separate remote voice observation from Duskcue custom deep-link delivery. Voice may be `not_supported` without invalidating a local deep-link test. |
| Watch Next | Android TV owns the Watch Next row and app writes alone do not prove user-visible placement. | Verify Continue Watching and Next Up on both launcher generations after provider diagnostics succeed. |
| Play quality policy | Android's current TV quality guidance includes TV submission requirements and an August 1, 2026 64-bit/16 KB policy boundary. | Retain Task 12 artifact/quality gates and observe signed Play behavior on the actual BRAVIA target. |

Sony BRAVIA provides direct coverage for the largest Android TV / Google TV variance that cannot be inferred from SHIELD or an emulator: versioned home surfaces, model/region-specific Play compatibility, TV-owned audio and HDR routing, supplied remote behavior, and voice prerequisites. The cost is a broader matrix: firmware, panel, HDMI input, AVR/soundbar, region, Google account, language, and remote hardware can all change a result.

Recommendation: close Task 14 only with one physical Sony BRAVIA Google TV and one physical Sony BRAVIA Android TV evidence set. Keep each result scoped to the observed model, firmware, UI generation, and display/audio route; use Duskcue direct-stream or transcode fallback when the target cannot sustain the requested format.

## Repository Support

The Android support-bundle capability report classifies Sony TV hardware as `sony_bravia` and captures a bounded model, Android release/API, display mode and advertised HDR types, advertised video decoders, current audio output types/encodings, and coarse network class/metered state. It never includes a serial, build fingerprint, MAC address, SSID, IP address, server credential, signed URL, raw media path, title, account, or profile.

```powershell
node scripts/sony-bravia-validation.mjs --plan
node scripts/sony-bravia-validation.mjs --serial <adb-serial> --experience google_tv --apk clients/tv/android/app/build/outputs/apk/debug/app-debug.apk
node scripts/sony-bravia-validation.mjs --serial <adb-serial> --experience android_tv
```

The physical command verifies only a non-emulated Sony Leanback device, presence of the Google Play package, optional APK installation, Duskcue Leanback launch, and valid-shape deep-link handoff. The declared `google_tv` or `android_tv` value is tester-provided because ADB cannot reliably prove the launcher generation. The command prints a privacy-safe preflight record and does not write evidence into the repository.

## Physical Test Sequence

1. Update each model to the current supported Sony firmware. Record the BRAVIA model, Android or Google TV generation, app build, display/AVR/soundbar, HDMI input, eARC/ARC or pass-through setting, remote type, and only coarse network transport. Use legal test media and a non-production Duskcue server.
2. On both a Sony BRAVIA Google TV and Sony BRAVIA Android TV, verify signed Play track visibility and installation. The Google Play package is not proof that the app listing is eligible.
3. Run device link, profile selection, browse, direct play, direct stream, HLS transcode, seek, resume, stop, subtitle/audio selection, and redacted diagnostics export. Record the actual stream decision and fallback.
4. Test SDR, HDR10, and Dolby Vision only where the exact model, input, HDMI chain, and source route advertise support. Record the real TV or AVR output mode, not just Android capability tokens.
5. Test PCM or AAC fallback, AC-3, E-AC-3 or Atmos, and DTS where the route permits. Capture the exact downmix, pass-through, no-audio, or not_supported outcome; menu names and available settings are model-specific.
6. Use the supplied remote for D-pad, Select, Back, Home return, play/pause, seek, captions, audio, and Settings. Verify focus after returning from the launcher or playback.
7. Test standby/resume through the configured BRAVIA and HDMI-CEC route. Duskcue must pause, revalidate, and reconnect rather than reuse a stale stream URL.
8. Test voice only after recording supported remote, account, country, language, and settings. Record a separate valid Duskcue deep-link handoff; it does not prove voice discovery.
9. Verify Watch Next with a profile-scoped movie resume and completed-episode Next Up replacement. Successful provider writes are not proof of launcher visibility; observe the launcher itself on both UI generations.

## Evidence Contract

For each case, retain `test_case_id`, `result` (`passed`, `failed`, `not_supported`, or `not_tested`), device target, BRAVIA model, Google TV or Android TV generation, Android release, firmware, app build, display/audio chain, stream decision or fallback, and a reference to a redacted diagnostics export. Keep this in a protected release record or issue/PR artifact.

Do not commit diagnostics bundles, ADB serials, MAC addresses, IP addresses, SSIDs, credentials, signed URLs, private media, screenshots containing personal catalogs, Play account data, or voice transcripts. Google Play visibility, launcher behavior, and voice discovery remain external device/account/region gates and cannot be proved by a debug APK.

## Official Sources

- [Sony: install apps on Google TV or Android TV](https://www.sony.com/electronics/support/articles/00147386)
- [Sony: BRAVIA app availability and compatibility](https://www.sony.com/electronics/support/televisions-projectors-lcd-tvs/xbr-65x900a/articles/00114472)
- [Sony: BRAVIA HDR-compatible models and HDMI inputs](https://www.sony.com/electronics/support/televisions-projectors-lcd-tvs-android-/xbr-75x950h/articles/00167110)
- [Sony: ARC/eARC audio troubleshooting and PCM fallback](https://www.sony.com/electronics/support/articles/00020051)
- [Sony: Dolby Atmos compatibility and fallback](https://www.sony.com/electronics/support/televisions-projectors-lcd-tvs-android-/xbr-65x900c/articles/00237077)
- [Sony: remote voice-search prerequisites and limits](https://www.sony.com/electronics/support/televisions-projectors-lcd-tvs-android-/kd-65x80j/articles/00127005)
- [Android TV Watch Next guidelines](https://developer.android.com/training/tv/discovery/guidelines-app-developers)
- [Android TV app-quality guidance](https://developer.android.com/develop/adaptive-apps/quality-guidelines/tv-app-quality)

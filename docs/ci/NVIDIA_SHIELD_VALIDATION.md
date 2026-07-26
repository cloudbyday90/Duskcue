# NVIDIA SHIELD TV / Pro Validation

## Status

Phase 17 Task 13 is repository-ready but physical evidence is pending. The checked-in fixture, Android capability export, and ADB preflight tool make the eventual run repeatable; they do not claim that a SHIELD device, AVR, display, network, Google Play listing, or Watch Next row has been observed.

The machine-readable authority is [nvidia-shield-validation.json](../api/fixtures/device-lab/v1/nvidia-shield-validation.json). Run `node scripts/verify-nvidia-shield-validation.mjs` for static drift and `node scripts/nvidia-shield-validation.mjs --plan` for the physical sequence.

## Research Outcome

| Area | Official finding | Validation decision |
|---|---|---|
| Device role | NVIDIA positions SHIELD TV and NVIDIA SHIELD TV Pro as 4K HDR/Dolby Vision, Dolby Atmos, AI-upscaling Android TV devices. | Treat both as high-capability Android TV reference targets, not evidence for every Android TV OEM. |
| Network | NVIDIA recommends Ethernet for maximum performance and documents 5 GHz Wi-Fi guidance. | Run the same legal test profile on Ethernet and Wi-Fi, recording only the transport class. |
| HDR | SHIELD HDR10/Dolby Vision depends on the whole HDMI/HDCP display chain. | Record SHIELD, cable, AVR/TV, display mode, and actual display/AVR readout; a codec list never proves HDR output. |
| Audio | NVIDIA documents HDMI AC-3, E-AC-3, Atmos, TrueHD, DTS:X, and DTS support, while unsupported receiver routes can result in no audio. | Test every legal sample against the routed AVR/TV and capture passed, failed, or not_supported; never infer passthrough from branding alone. |
| AI upscaling | AI-Enhanced is unavailable below 480p, above 30 Hz, and for RGB video, where SHIELD falls back to Enhanced. | Verify Duskcue does not override the user setting and record expected fallback observations without making an image-quality claim. |
| Android audio routing | Android says output capability depends on the currently routed device and can change during playback. | Export current audio-route/encoding context, then confirm the actual AVR result manually. |

The benefits of SHIELD are its mature Android TV stack, Ethernet, high-capability display/audio chains, remote/gamepad support, and current NVIDIA update channel. The costs are that SHIELD is neither a generic Google TV device nor a substitute for Sony validation; actual HDR, passthrough, CEC/standby, Play visibility, and launcher behavior remain dependent on the connected display, AVR, firmware, and account/track state.

Recommendation: use SHIELD TV or NVIDIA SHIELD TV Pro to establish the high-capability envelope, retain the exact evidence externally, and keep unsupported audio/HDR routes on Duskcue's direct-stream/transcode fallback rather than promoting an unobserved capability.

## Repository Support

The Android support-bundle capability report captures a bounded device family/model, Android release/API, display mode and advertised HDR types, advertised video decoders, current audio output types/encodings, and coarse network class/metered state. It never includes a device serial, build fingerprint, MAC address, SSID, IP address, server credential, signed URL, raw media path, title, account, or profile.

```powershell
node scripts/nvidia-shield-validation.mjs --plan
node scripts/nvidia-shield-validation.mjs --serial <adb-serial> --network ethernet --apk clients/tv/android/app/build/outputs/apk/debug/app-debug.apk
node scripts/nvidia-shield-validation.mjs --serial <adb-serial> --network wifi
```

The physical command verifies a non-emulated NVIDIA SHIELD with Leanback, optionally installs the supplied APK, verifies the Duskcue launcher and a valid-shape playback deep link, and prints a privacy-safe preflight record. It does not write a report into the repository. Redirect output only to protected release evidence storage if needed.

## Physical Test Sequence

1. Update the device; record SHIELD firmware/version, app build, display/AVR model, HDMI topology, and selected transport. Use legal test media and a non-production Duskcue server.
2. Run the Ethernet path, then the Wi-Fi path. Verify device link, browse, direct play, direct stream, HLS transcode, seek/resume/stop, subtitle/audio selection, and redacted diagnostics export.
3. Test SDR, HDR10, and Dolby Vision where the full route advertises support. Record actual display/AVR mode and fallback; advertised HDR types are not a pass.
4. Test AAC/PCM fallback, AC-3, E-AC-3/Atmos, TrueHD, DTS, and DTS-HD with SHIELD connected to the AVR/TV route. An unsupported receiver result is `not_supported`, not a silent pass or a global app failure.
5. Compare Basic, Enhanced, and AI-Enhanced where the sample is eligible. Confirm expected AI fallback for sub-480p, over-30 Hz, and RGB content, and confirm Duskcue does not change the SHIELD display/upscaling setting.
6. Use the SHIELD remote for D-pad, Select, Back, Home, play/pause, seek, captions, audio, and Settings. Repeat core controls with a paired gamepad if available.
7. Verify standby/resume through the configured HDMI-CEC route. Duskcue must pause/revalidate/reconnect rather than reusing a stale playback URL.
8. Verify Watch Next with a profile-scoped movie resume and completed-episode Next Up replacement. Successful provider writes are not proof of launcher visibility; observe the launcher itself.

## Evidence Contract

For each case, retain `test_case_id`, `result` (`passed`, `failed`, `not_supported`, or `not_tested`), device target, Android release, firmware, app build, network transport, display/audio chain, stream decision or fallback, and a reference to a redacted diagnostics export. Keep this in a protected release record or issue/PR artifact.

Do not commit diagnostics bundles, ADB serials, MAC addresses, IP addresses, SSIDs, credentials, signed URLs, private media, screenshots containing personal catalogs, or Play account data. Google Play visibility remains an external signed-track gate and cannot be proven using the debug APK.

## Official Sources

- [NVIDIA SHIELD product and capability overview](https://www.nvidia.com/en-us/shield/)
- [NVIDIA SHIELD software updates](https://www.nvidia.com/en-us/shield/software-update/)
- [NVIDIA HDR10 / Dolby Vision display setup](https://www.nvidia.com/en-us/shield/support/shield-tv-pro/4k-hdr-dolby-vision-display-setup/)
- [NVIDIA Dolby Vision / HDR10 settings](https://www.nvidia.com/en-gb/shield/support/shield-tv/enable-dolby-vision-hdr10-on-shield/)
- [NVIDIA AVR and surround-audio setup](https://www.nvidia.com/en-us/shield/support/shield-tv-pro/avr-surround-audio-setup/)
- [NVIDIA AI upscaling behavior](https://www.nvidia.com/en-us/shield/support/shield-tv-pro/ai-upscaling/)
- [NVIDIA network and video performance guidance](https://support-shield.nvidia.com/shield-tv-user-guide/How_to_Optimize_Internet_and_Video_Performance.htm)
- [NVIDIA SHIELD remote behavior](https://www.nvidia.com/en-us/shield/support/shield-tv/know-your-shield-tv/)
- [Android TV audio capabilities](https://developer.android.com/training/tv/playback/audio-capabilities)
- [Media3 audio capabilities](https://developer.android.com/reference/androidx/media3/exoplayer/audio/AudioCapabilities)
- [Android TV app-quality guidance](https://developer.android.com/docs/quality-guidelines/tv-app-quality)

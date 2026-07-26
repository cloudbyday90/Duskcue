# Android TV / Google TV Release Readiness

## Status

Phase 17 Task 12 establishes the repository-side Android TV / Google TV release baseline. It does not claim a Play Console app, an upload key, a signed release bundle, a published listing, a completed Data Safety form, a content rating, reviewer access, TV screenshots, or physical-device certification. Those are external gates with durable evidence still required before any beta or public-release claim.

The machine-readable authority is [android-tv-google-tv-readiness.json](../api/fixtures/release/v1/android-tv-google-tv-readiness.json), checked by [verify-android-tv-release-readiness.mjs](../../scripts/verify-android-tv-release-readiness.mjs).

## Application And Policy Baseline

| Item | Checked-in value | Release rule |
|---|---|---|
| Application ID | `com.duskcue.tv` | Register and retain this package identity; never reuse a different Play package accidentally. |
| Minimum SDK | 26 | Retains the Android TV/Watch Next capability baseline. |
| Target SDK | 36 | Meets the current Android TV Play target-API floor of 34; recheck policy immediately before each upload. |
| Package registration | External evidence pending | Record the Play Console registration before the `2026-09-30` package-registration deadline. |
| 16 KB / ABI policy | External evidence pending | Before the August 1, 2026 Android TV deadline, prove the required 32-bit/64-bit and 16 KB page-size compatibility for the exact upload artifact. |

Android TV form-factor opt-in and a dedicated **Android TV release track** are the intended distribution shape. The choice is deliberate: after the form factor is enabled, manage TV artifacts on the dedicated track and remove redundant TV artifacts from the mobile track. Do not make the irreversible Play Console setting until the listing and approval owner is ready.

## Artifacts And Versioning

The app accepts release values only through Gradle properties:

```powershell
./gradlew :app:bundleRelease "-PduskcueVersionCode=<unused-positive-code>" "-PduskcueVersionName=<semver>"
```

`duskcueVersionCode` must be a positive unused integer no greater than `2100000000`; it is monotonic across every Play upload. `duskcueVersionName` is the user-visible SemVer-compatible Duskcue release-train version. The recommended planning format is `YYYYMMDDNN`, such as `2026072501`.

The command can produce a candidate AAB now. It is not uploadable until the protected signing lane exists. The debug APK is CI/AVD evidence only; the release APK is a controlled device-lab or recovery artifact, not the normal Play upload. The AAB path, APK paths, and expected SBOM/provenance names live in the machine-readable fixture.

## Signing And Supply Chain

Use Play App Signing with a distinct upload key. The protected release environment will provide these secret references, never their values:

- `DUSKCUE_ANDROID_TV_UPLOAD_KEYSTORE`
- `DUSKCUE_ANDROID_TV_UPLOAD_KEY_ALIAS`
- `DUSKCUE_ANDROID_TV_UPLOAD_STORE_PASSWORD`
- `DUSKCUE_ANDROID_TV_UPLOAD_KEY_PASSWORD`

No keystore, certificate, alias value, password, Play service-account credential, or signed release artifact belongs in the repository. A future protected workflow must retain the AAB digest, `cyclonedx-android-tv.json` SBOM, provenance/attestation reference, approval record, and the reviewer runbook with the durable release evidence.

If the upload key is compromised, reset or revoke it in Play Console, rotate the protected references, and record the incident in release evidence.

## Store Assets And Screenshots

The source assets are ready for a listing review:

| Asset | Checked-in source | Status |
|---|---|---|
| Play icon | `docs/branding/assets/store/android-tv/play-icon-512.png` | 512×512 PNG |
| Play TV banner | `docs/branding/assets/store/android-tv/play-banner-1280x720.png` | 1280×720 non-transparent PNG |
| Runtime launcher banner | `clients/tv/android/app/src/main/res/mipmap-xhdpi/tv_banner.png` | Android TV density set: 160×90 through 640×360, with the required xhdpi 320×180 PNG |
| TV screenshots | Not checked in | `pending_real_capture` |

Capture at least one unaltered current Android TV screen from the approved AVD or physical-TV reviewer flow. It must show the actual app, not a mockup, private media, a test credential, or a composited marketing image. Complete the final Play listing requirements and localization review in the Play Console.

## Play App Content And Reviewer Access

The policy owner must complete and preserve external evidence for the privacy policy, Data Safety form, content rating questionnaire, target-audience review, ads declaration, and App Access form. The initial data-review candidates are account identity for authentication, an app-scoped random device identifier for device linking/remembered profile preference, profile-scoped playback activity sent to the selected self-hosted server, and a user-initiated redacted support-bundle export.

Never predeclare that Duskcue collects no data. The owner must verify actual data, purpose, sharing, encryption, retention, ads, target audience, and account-deletion obligations for the artifact being released.

Reviewer App Access must provide a time-bounded account or device-link path to a non-production Duskcue server with legal test media. The runbook must cover server selection, device linking, profile selection, playback, captions/audio, Watch Next settings, and manual support-bundle export. It must state that Duskcue ships no catalog and accesses only the reviewer-selected server. Credentials stay in Play Console, not this repository.

## Quality, Device, And Rollback Evidence

Automated evidence includes the shared client contracts/fixtures, Android TV unit tests, lint, debug assembly, Leanback launcher/deep-link AVD smoke, client-CI checks, and this release-readiness verifier. It does not replace these manual release gates:

- real Android TV screenshot and store-asset review;
- Google TV launcher/Watch Next visibility and direct deep-link launch;
- remote focus, TalkBack, captions/audio, overscan, reduced motion, and Back-to-launcher behavior;
- NVIDIA SHIELD and Sony BRAVIA playback, HDR/audio/subtitle, standby/resume, and diagnostics export;
- full Android TV quality checklist evidence for the signed candidate AAB.

The NVIDIA SHIELD TV / Pro high-capability evidence sequence is defined in [NVIDIA_SHIELD_VALIDATION.md](NVIDIA_SHIELD_VALIDATION.md). It is a physical-device release gate and does not replace the separate Sony BRAVIA validation.

For rollback, never reuse or lower a `versionCode`. Halt a staged rollout where possible, retain the previous Play artifact reference, and publish a higher-version-code hotfix or follow the approved Play rollback path. Any rollback must preserve or explicitly migrate encrypted session, profile, Watch Next, and artwork state.

## Official Sources

- [Android TV dedicated release tracks](https://support.google.com/googleplay/android-developer/answer/13295490?hl=en)
- [Google Play Data Safety](https://support.google.com/googleplay/android-developer/answer/10787469?hl=en)
- [Google Play content rating](https://support.google.com/googleplay/android-developer/answer/9859655?hl=en)
- [Google Play App content and App Access](https://support.google.com/googleplay/android-developer/answer/9859455?hl=en)
- [Google Play package setup and versioning](https://support.google.com/googleplay/android-developer/answer/9859152?hl=en)
- [Google Play target API policy](https://support.google.com/googleplay/android-developer/answer/11926878?hl=en)
- [Google Play package registration](https://support.google.com/googleplay/android-developer/answer/16984799?hl=en-EN)
- [Google Play preview assets](https://support.google.com/googleplay/android-developer/answer/9866151?hl=en)
- [Android TV app quality guidelines](https://developer.android.com/docs/quality-guidelines/tv-app-quality)
- [Android TV icon and banner design guidelines](https://developer.android.com/design/ui/tv/guides/system/tv-app-icon-guidelines)
- [Android app signing](https://developer.android.com/studio/publish/app-signing)
- [Android app versioning](https://developer.android.com/studio/publish/versioning)

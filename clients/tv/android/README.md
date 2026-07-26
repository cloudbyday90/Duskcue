# Duskcue Android TV

Native Android TV / Google TV client for Duskcue.

## Requirements

- Android SDK Platform 36 and Build-Tools 36
- Temurin JDK 17
- A running Duskcue server on its public port, `48027`

## Verify

```powershell
$env:JAVA_HOME = 'C:\Program Files\Eclipse Adoptium\jdk-17.0.19.10-hotspot'
$env:ANDROID_HOME = "$env:LOCALAPPDATA\Android\Sdk"
$env:ANDROID_SDK_ROOT = $env:ANDROID_HOME
./gradlew.bat :app:testDebugUnitTest :app:lintDebug :app:assembleDebug
```

The checked-in wrapper entry point delegates to the repository's Gradle 8.14 wrapper. Do not commit `local.properties`, signing material, or any server credential.

## Current Scope

The project validates the shared TV fixtures through typed Kotlin models, private ETag revalidation, RFC 9457 error decoding, encrypted profile-gated session state, living-room browsing, and Media3 playback. AndroidX Watch Next publication is active for fresh eligible Continue Watching, Next Up, and New Episodes rows; mappings are Keystore-encrypted, change-only, profile-isolated, and removed on account/profile/server/logout cleanup or Android's disabled-row signal. Watch Next posters are fetched only from the authenticated canonical artwork endpoint, conditionally revalidated by ETag, stored as opaque app-private files, and published as local `content://` URIs; no bearer, signed, or raw server artwork URL is exposed.

The current UI has conservative TV safe-area margins, minimum 20sp supporting text, visible focus/pressed/disabled states, logical Back handling, remote/gamepad/media shortcuts, live Media3 audio/caption selection, and TalkBack-oriented semantics. Run `node scripts/verify-accessibility-input.mjs` from the repository root together with the Gradle checks. Physical TalkBack, overscan, reduced-motion, remote, and launcher evidence remain later Phase 17 device/release work.

Settings can export a manually shared, privacy-safe support JSON bundle through Android's document picker. It is generated from an in-memory 24-hour/1,000-record ledger with host-only server information, bounded request/trace/playback/TV-surface correlation IDs, playback and Watch Next summaries, and no tokens, signed URLs, private paths, media IDs, titles, or profile/account data. Run `node scripts/verify-client-diagnostics.mjs` with the Gradle checks. Device validation of the document-picker handoff remains a later Phase 17 gate.

The shared CI workflow automatically runs the Android TV contract/conformance suite, `:app:testDebugUnitTest`, `:app:lintDebug`, and `:app:assembleDebug`, then retains the debug APK and reports for diagnosis. A maintainer can request the optional Android TV AVD smoke or run it locally after booting exactly one Android TV AVD:

```powershell
node scripts/android-tv-emulator-smoke.mjs --apk clients/tv/android/app/build/outputs/apk/debug/app-debug.apk
```

It verifies the TV runtime feature, APK install, Leanback launcher, and valid custom deep-link handoff. It does not replace authenticated playback, Watch Next, accessibility, remote, HDR/audio, standby/resume, document-picker, Google Play, or physical-device release evidence.

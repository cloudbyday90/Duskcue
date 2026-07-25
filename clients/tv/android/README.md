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

The project validates the shared TV fixtures through typed Kotlin models, private ETag revalidation, RFC 9457 error decoding, encrypted profile-gated session state, living-room browsing, and Media3 playback. AndroidX Watch Next publication is active for fresh eligible Continue Watching, Next Up, and New Episodes rows; mappings are Keystore-encrypted, change-only, profile-isolated, and removed on account/profile/server/logout cleanup or Android's disabled-row signal. Artwork URI publication, CI integration, and physical-device evidence remain later Phase 17 work.

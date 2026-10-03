# Duskcue Fire TV

Dedicated Fire OS Android application for Duskcue. The Fire target uses the Amazon Appstore application ID `com.duskcue.firetv`, supports Fire OS 7/API 28 and later, and consumes the Android TV target's platform-neutral API, profile, Media3 playback, UI, and diagnostics code without including AndroidX Watch Next or `tvprovider`.

## Verify

```powershell
$env:JAVA_HOME = 'C:\Program Files\Eclipse Adoptium\jdk-17.0.19.10-hotspot'
$env:ANDROID_HOME = "$env:LOCALAPPDATA\Android\Sdk"
$env:ANDROID_SDK_ROOT = $env:ANDROID_HOME
./gradlew.bat :app:testDebugUnitTest :app:lintDebug :app:assembleDebug
```

The wrapper delegates to the checked-in repository Gradle 8.14 wrapper. Do not commit `local.properties`, Appstore signing material, Amazon integration credentials, server credentials, or physical-device evidence.

## Scope

This target provides the native Fire OS living-room baseline: secure server selection/device linking, server-authoritative profile selection and remembered-device defaults, Kids policy/parent-unlock flow, app-local TV surfaces, Media3 playback, Duskcue deep-link revalidation, remote focus behavior, media-button handling, and privacy-safe diagnostics. It has no AndroidX Watch Next provider/receiver/artwork path.

The Fire-only Watch Activity reporter uses the official compile-only Amazon SDK. Gradle downloads its checksum-pinned archive into ignored build storage; no SDK classes are packaged in the APK. The optional system library is checked before every SDK send. `-PduskcueFireWatchActivityEnabled=true` enables the build capability, but reporting still requires an expiring, server-authorized binding, positive customer consent, a standard profile, interactive playback, an exact accepted catalog ID, and an opaque Fire profile key. The server-backed source obtains a private, at-most-60-second authorization lease and refreshes it every 30 seconds. Unknown customer consent denies reporting; an approved integration must supply a positive device consent provider.

Fixture-backed unit tests cover loaded resume position, state/seek events, 60-second playing and paused cadence, exit/completion, SDK failure, authorization expiry, late profile replies, denied renewal, and Kids/ambient denial. `scripts/verify-migrations.ps1 -RunFireRegistryTests` verifies the server registry, revocation, profile-key isolation, concurrent revision writes, and permanent ID retirement against disposable PostgreSQL 18. Physical Amazon integration remains a release gate.

The authenticated Duskcue-owned launch path rechecks account/profile scope and intent generation across resolve, server playback creation, and local player handoff. It uses the freshly resolved server resume position and cancels known unplayed attempts without changing watch history. The Fire test lane also runs shared launch/profile-session regressions; 37 tests pass. Amazon-provided catalog intents and EMBER publication await partner requirements. [The Vega track](../../../docs/design/FIRE_TV.md#separate-vega-track--task-5) is separate and has not triggered implementation for the current Fire OS release.

Watch Activity, Content Personalization, Amazon catalog IDs, EMBER, global Alexa/catalog discovery, and Vega remain gated as defined in [FIRE_TV.md](../../../docs/design/FIRE_TV.md). A physical Fire TV device is required for Appstore, remote/audio-focus, playback, accessibility, HDR/audio, standby, and any Amazon integration claim.

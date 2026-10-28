# Duskcue Mobile

Phase 16a creates the Flutter Android/iOS online client foundation.

The current app includes server selection/onboarding:

- manual `http(s)://<server>:48027` entry;
- Local, Remote VPN, and Exposed network modes;
- connection testing against `/health/ready`;
- saved server profiles and last-used server origin in OS-backed secure storage;
- rejection of Docker's internal `48028` API port.

The current auth/session foundation includes:

- secure-stored session token and cached user summary;
- stable secure-stored mobile device identifier;
- password, invite, re-auth, device-linking, and passkey method-channel sign-in paths;
- logout, logout-all, active-session listing, and per-session deletion.

Local and VPN modes may use HTTP for LAN/VPN deployments. Exposed mode requires HTTPS with a certificate trusted by Android/iOS. Private CA and self-signed certificates must be installed and trusted at the OS/profile level before the app can connect.

## Commands

```bash
flutter pub get
flutter analyze
flutter test
flutter build apk --debug --flavor production
flutter build ios --simulator
```

Browsing, playback, foreground SSE, push registration, and quality reporting are implemented by later Phase 16a tasks.

## Android command-player checkpoint

The project selects Flutter 3.47.8/Dart 3.13.7; [toolchain evidence](../../docs/ci/FLUTTER_TOOLCHAIN.md) records the official release/checksum and SDK-required lock changes. On Windows, use the repository-managed `flutter-check`, `flutter-test`, `flutter-apk` and `flutter-apk-production` presets through `node scripts/testing-memory/run.mjs`; keep heavy work serial. The qualification APK uses a separate application ID and test entry, with no built-in fixture credentials.

The [interactive migration](../../docs/design/VOD_FLUTTER_CLIENT_MIGRATION.md) now has app-owned protocol-two commands, journal/cleanup, renewal/checkpoints and a bearer-free HLS relay. The normal server still keeps production attachment disabled and unsupported servers use the existing player. Offline and native ambient lifetimes remain separate. Analysis and 52 Flutter tests pass; both Android debug flavors compile. Actual emulator/backend/native qualification remains required; see [Android qualification](../../docs/ci/ANDROID_PLAYBACK_QUALIFICATION.md). WHPX is enabled with restart required; no restart is authorized or performed.

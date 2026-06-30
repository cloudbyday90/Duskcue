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
flutter build apk --debug
flutter build ios --simulator
```

Browsing, playback, foreground SSE, push registration, and quality reporting are implemented by later Phase 16a tasks.

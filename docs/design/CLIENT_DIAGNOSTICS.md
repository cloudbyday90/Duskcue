# Client Diagnostics

## Purpose

This document is the authoritative Phase 16d Task 9 outcome for client diagnostics, logging, privacy-safe support bundles, and server-side correlation across Duskcue desktop, mobile, TV, and console clients.

The goal is to let a user or admin export useful troubleshooting evidence without exposing bearer tokens, passwords, signed media URLs, push tokens, private filesystem paths, raw watch history, or unnecessary filenames.

## Official Source Review

Reviewed July 2, 2026.

| Area | Official sources reviewed | Diagnostics impact |
|---|---|---|
| Structured logs | OpenTelemetry Logs Data Model | Use timestamped structured records with severity, body/message, attributes, resource/client metadata, and trace/request correlation fields. |
| Security logging | OWASP Logging Cheat Sheet and Secrets Management Cheat Sheet | Log security-relevant failures and state transitions, but exclude secrets, credentials, sensitive personal data, and raw request bodies. |
| Android and Google Play privacy | Android data-use declarations and Google Play Data safety guidance | Diagnostics and support bundles can be data collection; release checklists must disclose collection/sharing behavior accurately and keep consent explicit where export is user-initiated. |
| Apple app privacy | App Privacy Details and User Privacy/Data Use | Clients must describe collected diagnostics and whether data is linked to a user or device before store release. |
| Microsoft privacy | Microsoft Privacy Statement and Windows privacy compliance guidance | Treat diagnostic data as personal data when it can identify a user, device, server, or behavior; provide transparency and user control. |
| TV platform certification | Roku certification and authenticated-channel testing guidance | TV clients need testable diagnostics and bounded app behavior evidence, but support bundles must remain safe for communal devices. |

## Machine-Readable Pack

The versioned fixture pack starts at [../api/fixtures/diagnostics/v1/manifest.json](../api/fixtures/diagnostics/v1/manifest.json).

Run:

```bash
node scripts/verify-client-diagnostics.mjs
```

The verifier checks required log fields, privacy classifications, bundle sections, redaction rules, platform checklists, and correlation identifiers.

## Client Log Schema

Every platform client should be able to emit structured log records with these required fields:

| Field | Purpose |
|---|---|
| `timestamp` | UTC RFC3339 event time |
| `client_version` | App version/build identifier |
| `platform` | Platform target, such as `flutter_mobile`, `roku`, or `apple_tvos` |
| `route_or_screen` | Current route, screen, or bounded surface name |
| `request_id` | Last relevant `x-request-id` response/request header where applicable |
| `event_type` | Stable snake_case event name |
| `severity` | `trace`, `debug`, `info`, `warn`, `error`, or `fatal` |
| `privacy_classification` | Classification from the fixture pack |

Recommended optional fields:

- `server_origin_redacted`
- `user_id_hash`
- `device_id_hash`
- `playback_session_id`
- `notification_id`
- `download_job_id`
- `package_id`
- `tv_surface_event_id`
- `network_state`
- `error_code`
- `trace_id`

## Privacy Classifications

The shared classes are:

| Class | Meaning | Export behavior |
|---|---|---|
| `public` | Non-sensitive product/platform value | May appear in support bundles |
| `operational` | Troubleshooting data that is not user-identifying by itself | May appear in support bundles |
| `user_private` | Identifies a user, device, server, household, or private behavior | Redact, hash, aggregate, or require explicit consent |
| `secret` | Grants access or can be used to impersonate a user/device/server | Never export |
| `consent_required` | Useful but behaviorally sensitive, such as raw watch history or filenames | Export only after explicit, narrow consent |

Default support bundles should include only `public` and `operational` data plus hashed/redacted `user_private` values.

## Support Bundle Contents

Every platform should support a local export bundle with:

1. App logs, capped by count and time window.
2. Device capability report, using the same capability vocabulary as quality/playback fixtures.
3. Redacted server URL in host-only or origin-hash form.
4. Playback failure summaries with bounded error codes, stream decision, and request/playback IDs.
5. Network state summary, such as online/offline, metered, Wi-Fi/cellular class, proxy/VPN hint, and last reachability result.
6. Recent request IDs for server-side log lookup.

Optional sections:

- recent notification IDs
- recent download job/package IDs
- TV surface event IDs
- platform permission state
- user-consented filenames or watch-history extracts

Bundles must be generated on device and handed to the user/admin for manual sharing unless a future release explicitly adds authenticated upload with consent.

## Redaction Rules

Never include:

- bearer tokens, refresh tokens, session cookies, re-auth codes, device-link codes, invite codes, API keys, encryption keys, or passkey private material
- passwords or password hashes
- push tokens
- signed media URLs or package transfer URLs
- private filesystem paths
- raw watch history unless the user explicitly consents to that exact section
- media filenames unless a narrow troubleshooting flow explicitly asks for filename evidence
- full server URL when host/IP disclosure is unnecessary

Allowed transformations:

- `host_only` for server origins when needed for troubleshooting
- `stable_hash` for user, device, and server identifiers
- `strip_query` for URLs that are not secret after query removal
- `error_code_only` for Problem Details when detail text may include private context
- `bounded_title` only for user-visible media titles already visible in the current screen

## Server Correlation

The server already generates and propagates `x-request-id` and exposes Problem Details `trace_id`. Client diagnostics must capture these identifiers rather than raw request/response bodies.

Primary correlation fields:

- `request_id` from `x-request-id`
- `trace_id` from Problem Details
- `playback_session_id`
- `notification_id`
- `download_job_id`
- `package_id`
- `tv_surface_event_id`

When a support bundle is shared with an admin, these IDs let the admin correlate with server logs, playback events, notification rows, download rows, and TV surface diagnostics without needing secrets or private paths.

## Platform Expectations

| Platform family | Minimum behavior |
|---|---|
| Web/Tauri | Export JSON bundle from client state; include recent API errors and request IDs; omit cookies and localStorage secrets. |
| Flutter mobile | Export from app-private storage; include permission/network state and download/playback summaries; use OS share sheet only after local redaction. |
| Android TV/Fire TV | Include remote/focus, launcher/deep-link, playback, and network state summaries; avoid exposing profile data on communal screens. |
| Roku | Export or display bounded diagnostics suitable for developer sideload/certification workflows; no tokens in registry or SceneGraph logs. |
| Tizen/webOS | Export web-app JSON bundle with bounded platform media/network details; no signed URLs or private server paths. |
| tvOS | Use user-initiated export or on-screen code-based handoff; respect Apple privacy disclosure requirements. |
| Windows/Xbox | Include request IDs, media stack summary, capability state, and controller/focus failures; align with Microsoft diagnostic-data transparency expectations. |

## Implementation Notes

Phase 16d Task 9 adds:

- [../api/fixtures/diagnostics/v1](../api/fixtures/diagnostics/v1) as the versioned diagnostics and support-bundle fixture pack
- [../../scripts/verify-client-diagnostics.mjs](../../scripts/verify-client-diagnostics.mjs) as the drift gate
- cross-references in `BUILD_ORDER.md`, `PROJECT.md`, `CLIENT_CONTRACTS.md`, `CLIENT_PLATFORM_READINESS.md`, and `LOGGING_OBSERVABILITY.md`

## Research Sources

- OpenTelemetry Logs Data Model: https://opentelemetry.io/docs/specs/otel/logs/data-model/
- OWASP Logging Cheat Sheet: https://cheatsheetseries.owasp.org/cheatsheets/Logging_Cheat_Sheet.html
- OWASP Secrets Management Cheat Sheet: https://cheatsheetseries.owasp.org/cheatsheets/Secrets_Management_Cheat_Sheet.html
- Android declare your app's data use: https://developer.android.com/privacy-and-security/declare-data-use
- Google Play Data safety section: https://support.google.com/googleplay/android-developer/answer/10787469
- Apple App Privacy Details: https://developer.apple.com/app-store/app-privacy-details/
- Apple User Privacy and Data Use: https://developer.apple.com/app-store/user-privacy-and-data-use/
- Microsoft Privacy Statement: https://www.microsoft.com/en-us/privacy/privacystatement
- Windows privacy compliance guide: https://learn.microsoft.com/en-us/windows/privacy/windows-privacy-compliance-guide
- Roku certification criteria: https://developer.roku.com/dev/docs/certification

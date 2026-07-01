# Offline Downloads

## Overview

This document is the authoritative Phase 16c design for mobile offline downloads. Offline downloads are a user-requested durable copy of playable media for authenticated mobile devices, not a cache and not a TV/desktop/web feature in v1.

**Supported v1 clients:** Android and iOS mobile apps only.

**Deferred clients:** web browsers, desktop/Tauri, TV, console, and casting surfaces.

**Research refresh:** June 2026. Official platform references used for this design:

| Area | Official source |
|---|---|
| Android long-running user downloads | Android Developers: [User-initiated data transfer jobs](https://developer.android.com/develop/background-work/background-tasks/uidt) |
| Android background constraints | Android Developers: [WorkManager constraints](https://developer.android.com/develop/background-work/background-tasks/persistent/getting-started/define-work#work-constraints) |
| Android app storage and backup boundaries | Android Developers: [Data and file storage overview](https://developer.android.com/training/data-storage) and [Back up user data](https://developer.android.com/identity/data/autobackup) |
| Android protected metadata | Android Developers: [Security with data](https://developer.android.com/privacy-and-security/security-data) and [Android Keystore system](https://developer.android.com/privacy-and-security/keystore) |
| iOS background transfer | Apple Developer Documentation: [URLSessionConfiguration](https://developer.apple.com/documentation/foundation/urlsessionconfiguration) and [Downloading files in the background](https://developer.apple.com/documentation/foundation/url_loading_system/downloading_files_in_the_background) |
| iOS HLS offline playback | Apple Developer Documentation: [AVAssetDownloadURLSession](https://developer.apple.com/documentation/avfoundation/avassetdownloadurlsession) |
| iOS file protection and backup exclusion | Apple Developer Documentation: [FileProtectionType](https://developer.apple.com/documentation/foundation/fileprotectiontype) and [URLResourceKey.isExcludedFromBackupKey](https://developer.apple.com/documentation/foundation/urlresourcekey/1414219-isexcludedfrombackupkey) |
| iOS review constraints | Apple: [App Review Guidelines](https://developer.apple.com/app-store/review/guidelines/) |

## Scope

Phase 16c adds:

- Server APIs for planning, creating, serving, revoking, expiring, and syncing downloads.
- Durable server-side package preparation.
- Resumable transfer from server to mobile client.
- Protected mobile local storage for package files, manifests, metadata, sync queues, and server/user/device bindings.
- Offline playback for downloaded movies and episodes.
- Reconnect sync for resume position, completion, watch state, and policy revalidation.

Phase 16c does not add:

- Offline downloads to web browsers, desktop, TV, console, or partner platforms.
- DRM or circumvention of protected commercial streams.
- Public CDN distribution of packages.
- Shared packages across users, servers, or devices.

## Package Format Decision

V1 uses a **manifest-backed hybrid package model**:

1. **Canonical package:** HLS with fMP4 segments in a package directory.
2. **Optimization:** single MP4 package only when the selected source/version is already mobile-compatible and the requested audio/subtitle selection can be represented without losing required behavior.

Every package has the same server-authored manifest shape regardless of physical layout. The manifest includes schema version, package ID, server ID, user ID, device ID, media IDs, selected source/version, selected quality, selected audio/subtitle tracks, artwork files, storyboard files if included, per-file sizes, per-file checksums, package integrity hash, expiry, and sync metadata. It never contains bearer tokens, refresh tokens, filesystem paths, raw signed URLs, or reusable package secrets.

| Option | Pros | Cons | V1 decision |
|---|---|---|---|
| HLS/fMP4 directory | Works with native mobile players; supports adaptive-compatible segmenting; naturally resumable per file/range; clean subtitle/audio sidecars; aligns with existing streaming fMP4 direction; bad segments can be repaired individually | More files; package manifest and directory lifecycle are required; direct copy may still need remuxing | Canonical |
| Single MP4 file | Simple inventory; efficient for direct-compatible sources; native playback is straightforward; fewer filesystem entries | Harder to repair partially; less flexible for alternate audio/subtitle choices; trickplay/storyboards still need sidecars; poor fit when transcode/remux is required | Allowed optimization only |
| Always hybrid | Preserves a common manifest/API while choosing the cheapest safe physical representation | Requires clients to support both physical layouts | Selected |

### Track and Trickplay Rules

- Audio: package includes the selected audio track and may include additional tracks only when policy and device support make them useful. Lossless/surround audio is not preserved by default if it makes mobile offline playback unreliable or too large.
- Subtitles: text subtitles are normalized to mobile-playable sidecars. Bitmap subtitles require pre-existing OCR/conversion or video burn-in when no text alternative exists and policy allows the cost.
- Storyboards: offline packages may include pre-sized WebP storyboard sprites and WebVTT indexes from the storyboard domain when available; missing storyboards do not block download.
- Artwork: package includes poster/backdrop/thumb assets sized for offline library and detail views.
- Resumability: HLS/fMP4 packages resume by package-file range and checksum; single MP4 packages resume by HTTP Range and file checksum.

## Platform Constraints

### Android

- Use Android's user-initiated data transfer job path for long user-started downloads on modern Android, with WorkManager constraints for queued work and app-restart recovery where appropriate.
- Expose user controls for Wi-Fi-only, cellular opt-in, charging-only, and pause/resume/cancel/delete.
- Apply unmetered-network constraints for Wi-Fi-only downloads and storage-not-low constraints before starting or resuming transfer.
- Store package files under app-specific storage. Downloaded media must not require broad external-storage permissions.
- Exclude downloaded package files and regenerable artwork/storyboard copies from cloud/device backup. Metadata required to bind a package to a server/user/device may be backed up only if it contains no secrets and does not resurrect deleted/revoked media.
- Store tokens, package keys if introduced later, server/user/device bindings, sync queue state, and download inventory metadata using OS-backed encryption. Prefer Android Keystore-backed secure storage for secrets and encrypted app-private metadata for non-large structured state. Do not use deprecated broad storage or all-files access.
- Treat Android low-storage signals as a hard pause/fail condition for new work. Cleanup is limited to Duskcue temp, failed, expired, revoked, and user-deleted packages.

### iOS

- Use background `URLSession` downloads for package files and `AVAssetDownloadURLSession` where native HLS asset-download behavior is the better fit for HLS/fMP4 packages.
- Set expensive/constrained network behavior from user settings: Wi-Fi-only disables expensive cellular paths, cellular opt-in allows them, and Low Data Mode/constrained paths are respected unless the user explicitly overrides where iOS permits.
- Use iOS app-container storage for package files.
- Apply file protection to offline metadata and downloaded package files. Default to protection that keeps data unavailable until first unlock after boot while avoiding playback breakage for a user who starts playback after unlocking.
- Exclude downloaded media, artwork copies, storyboard files, and other large/regenerable package files from iCloud/iTunes backup.
- Store tokens, package keys if introduced later, server/user/device bindings, sync queue state, and download inventory metadata in Keychain or app-private encrypted/protected storage according to sensitivity.
- Treat low-storage conditions as a hard pause/fail condition for new work. Cleanup is limited to Duskcue temp, failed, expired, revoked, and user-deleted packages.

## Server Contract

The downloads domain owns these server responsibilities:

- Plan downloads from current user access, library access, streaming policy, device capability, selected quality, selected streams, source version, and storage policy.
- Create durable package jobs that survive server restart.
- Prepare packages in bounded work directories with retry, cancellation, timeout, progress, and cleanup.
- Keep offline package job concurrency separate from live playback/transcode concurrency.
- Serve manifests and package files only after revalidating authenticated user/session/device access.
- Support resumable transfer with HTTP Range or equivalent chunk-level retry.
- Return private cache headers and avoid public cache/CDN assumptions.
- Record audit events for create, serve, cancel, delete, quota denial, expiry, cleanup, and revoke/delete.
- Emit foreground SSE and mobile push notifications for important state changes without high-frequency progress spam.

Phase 16c Task 1 added the database foundation:

| Table | Responsibility |
|---|---|
| `download_jobs` | Durable package planning/preparation queue with user/session/device/media ownership, selected quality/streams/artwork, package strategy, progress, retries, failure reason, cancellation marker, policy snapshot, expiry, and cleanup eligibility. |
| `download_packages` | Server package inventory with logical storage key, relative manifest path, package format, byte/file counts, hashes, selected streams, included artwork/storyboards, sync metadata, policy snapshot, serve timestamps, expiry, revocation, and cleanup eligibility. |
| `download_package_files` | Per-file manifest for relative package paths, roles, content types, byte sizes, SHA-256 checksums, segment indexes, track identifiers, and required/optional flags. |
| `download_device_state` | Per-user/device local inventory and sync state with local status, transferred bytes, verified file count, local manifest hash, online/download/play timestamps, resume position, pending sync queue, deletion marker, and local failure details. |
| `download_events` | Explicit event rows for job/package/quota/policy/checksum/sync/cleanup audit history. |

Phase 16c Task 2 added the `server/src/domains/downloads/` five-file domain shell:

| Route | Purpose | Current Task 2 behavior |
|---|---|---|
| `GET /api/v1/downloads/plan/{media_item_id}` | Planning contract for a movie or episode | Validates query and returns `DOWNLOAD_015` until Task 4 |
| `POST /api/v1/downloads/jobs` | Create durable package job | Validates body and returns `DOWNLOAD_015` until Tasks 3-6 |
| `GET /api/v1/downloads/jobs/{id}` | Read job status | Returns `DOWNLOAD_015` until Task 6 |
| `POST /api/v1/downloads/jobs/{id}/cancel` | Cancel job | Validates body and returns `DOWNLOAD_015` until Task 6 |
| `GET /api/v1/downloads/inventory` | List user/device inventory | Validates query and returns `DOWNLOAD_015` until Tasks 7 and 10 |
| `DELETE /api/v1/downloads/packages/{id}` | Delete package/local state | Validates body and returns `DOWNLOAD_015` until Tasks 7, 10, and 13 |
| `GET /api/v1/downloads/packages/{id}/manifest` | Fetch package manifest | Returns `DOWNLOAD_015` until Tasks 5 and 7 |
| `POST /api/v1/downloads/packages/{id}/transfer-urls` | Create short-lived transfer URLs | Validates body and returns `DOWNLOAD_015` until Task 7 |
| `GET /api/v1/downloads/packages/{id}/files/{*file_path}` | Serve package file/range | Returns `DOWNLOAD_015` until Task 7 |
| `POST /api/v1/downloads/sync` | Submit reconnect sync state | Validates body and returns `DOWNLOAD_015` until Task 12 |

`DOWNLOAD_001`-`DOWNLOAD_015` are registered in [ERROR_HANDLING.md](ERROR_HANDLING.md). Planning and job creation require the `can_download` capability; read/delete/manifest/sync routes are authenticated user-scoped and will enforce BOLA/policy checks in the service layer as implementation lands.

Phase 16c Task 3 added policy enforcement foundations:

- `server_config.downloads` stores global enablement, max quality/resolution, max bytes per user/device, max active jobs per user/device, max retained packages per user/device, LAN/remote restrictions, transcode-download allowance, default expiry, ready-package retention, and per-user/library override maps.
- Planning and job creation now check the authenticated user's library access, verify that the item has at least one healthy media file, enforce Android/iOS route payloads, enforce global enablement and LAN/remote restrictions, and enforce active-job, retained-package, and retained-byte quotas before returning later-task not-implemented responses.
- Job/package/manifest/transfer/file/sync routes verify job/package ownership before returning later-task not-implemented responses so future implementation starts from BOLA-safe boundaries.
- Policy and quota denials create `download_events` rows with bounded reasons and no filesystem paths, tokens, signed URLs, or private package internals.

## Mobile Contract

The mobile clients own these responsibilities:

- Keep download inventory scoped by server origin, server ID, user ID, and device ID.
- Never show another server/user's packages after account switch, logout, server removal, or app restore.
- Support queue, pause, resume, cancel, retry, delete item, and delete all.
- Show states: planning, queued, preparing, ready to download, downloading, paused, playable offline, failed, expired, unavailable, revoked, and syncing.
- Preserve local resume position, completion, watched state, and play events while offline.
- Sync queued playback changes idempotently on reconnect.
- Revalidate access, expiry, and policy before allowing new downloads, package refresh, or continued online-backed package use.
- Explain storage and network policy in normal settings language, without implementation details.

## Revocation Model

Offline revocation has two enforcement classes:

| Revocation class | Server can enforce immediately | Requires mobile reconnect or app open |
|---|---|---|
| New downloads | Yes. Planning/job creation/serving is denied after library access, item availability, policy, user, or session revocation. | No |
| In-progress transfer | Yes while the client is online and requesting manifest/files. Existing transfer URLs or authenticated file requests stop renewing/serving after revalidation fails. | Yes if the device is fully offline and already has all bytes needed for playback |
| Fully downloaded package | No. The server cannot reach a powered-off or offline device. | Yes. The app disables/deletes packages on next online check, sync, auth refresh, logout, session invalidation, or server instruction |
| Expiry | Server refuses refresh/renew/serve after expiry. | The app enforces local expiry timestamp while offline and revalidates at reconnect |

Required UI language:

- Download detail screens show an availability date such as "Available until <date>" when expiry applies.
- Settings and first-use copy explain that downloads require periodic online checks.
- If access changes while a device is offline, Duskcue removes or disables the download the next time the app reconnects.
- Revoked and expired packages are distinguishable from temporary server-unavailable states.

## Storage and Cleanup

Offline packages are durable user data, not cache. They are stored separately from `/cache/hls`, `/cache/storyboards`, image cache, and tmpfs transcode output.

Server cleanup may delete:

- Failed package work directories.
- Cancelled packages.
- Expired packages after the retention window.
- Orphaned package files with no database record.
- Never-downloaded ready packages after policy retention expires.
- Revoked packages after the server has recorded the revoke/delete instruction.

Server cleanup must not delete a user's active offline package solely because a cache threshold is exceeded. Download quotas and package retention policies govern offline package cleanup.

Mobile cleanup may delete:

- Temp and partial files for cancelled/failed transfers.
- Expired packages after user-visible state update.
- Revoked packages after reconnect policy sync.
- Packages explicitly deleted by the user, logout, delete-all, server removal, user deletion, or session invalidation.

Low-storage handling:

- Preflight expected package size plus safety margin before job creation and before mobile transfer.
- Pause or fail gracefully when the OS reports storage-not-low constraints are unmet.
- Offer user-controlled deletion of existing downloads.
- Never delete non-Duskcue files.

## Security and Privacy

- Every download API reuses library/item BOLA checks before planning, job creation, package serving, sync, refresh, or delete.
- Download manifests and package files contain no source filesystem paths.
- Transfer URLs, if used, are short-lived and package/session/device scoped.
- Package metadata does not store bearer tokens, refresh tokens, raw signed streaming URLs, or reusable package secrets.
- Logs, diagnostics, crash reports, and fixtures must not contain local package paths, bearer tokens, package secrets, signed URLs, or private media filenames beyond bounded user-visible titles where explicitly allowed.
- Logout, session invalidation, server removal, user deletion, and delete-all disable or delete protected local offline state.
- App backup restore must not resurrect playable media for a user/server/session that has not reauthenticated and revalidated access.

## Quality Selection

Download quality reuses the Phase 7 quality-management model but is not identical to online streaming:

- Inputs: device profile, selected source/version, selected audio/subtitle tracks, user quality preference, download policy, current server capability, and estimated package size.
- Choices: Auto, Data Saver, Standard, Maximum, plus explicit resolution choices where policy allows.
- Prefer direct-compatible source versions and remux/direct-copy over full transcode.
- Avoid starting from the largest source if a lower-resolution source is already the better target.
- Offline transcodes may use slower/higher-quality presets than live playback because they are not latency-bound.
- Live playback capacity is protected from offline package work.

## Implementation Status

Phase 16c Tasks 0-3 are complete. The design/research outcome, database schema, downloads domain route/DTO/error shell, and access/quota/policy foundations are in place. Planning implementations, package manifest generation, package workers, package serving, notifications, mobile download manager, protected local storage, offline playback, reconnect sync, settings, observability, and tests are pending Phase 16c Tasks 4-15.

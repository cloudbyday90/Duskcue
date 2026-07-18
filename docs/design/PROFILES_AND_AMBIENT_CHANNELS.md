# Household Profiles, Kids Mode, and Ambient Channels

## Outcome

Duskcue has a Netflix-style household model: one authenticated Duskcue user owns one or more selectable profiles. A profile, rather than the authenticated account, owns viewing history, resume points, favorites, ratings, and TV-surface personalization.

The first implementation establishes the server contract and data model for three distinct experiences:

- standard profiles for independent household viewing history;
- Kids profiles with parent-selected library and age-rating limits enforced by the server; and
- ambient channels: continuous, curated playback that is observable for operational diagnostics but never changes a profile's watch history, resume position, play count, recommendations, or Trakt state.

## Research and Decisions

| Topic | Finding | Decision |
|---|---|---|
| Parent unlock secret | OWASP recommends a deliberately slow password-hashing function, with Argon2id preferred, for password-equivalent secrets. OWASP also recommends bounded failed-authentication attempts and avoids detailed retry signals that help automation. | Each new Kids profile has its own 4–12 digit parent PIN. The server stores only an Argon2id hash using the OWASP 19 MiB, two-iteration, single-lane baseline. A PIN never appears in a response, client cache, diagnostic signal, or remembered-profile mapping. Five failed attempts lock that Kids profile for 15 minutes in PostgreSQL; a successful check creates a ten-minute unlock for that profile on the current server session only. |
| Children's data | COPPA focuses on online collection from children under 13. A local-first server should still minimize child data and put control with the parent. | A Kids profile stores only a display name, avatar choice, and policy. It does not need an email, birth date, advertising identity, or external-service account. Unknown ratings are denied in Kids mode. |
| Remembered profile | Apple TV exposes a platform decision for whether a shared device should retain the current user selection; OWASP treats session credentials as authentication secrets, not preference data. | Remember a selected profile only as a server-side, account-scoped mapping to a stable per-installation device ID. It is opt-in, revocable, and never a cached session token, password, parental PIN, hardware identifier, or standalone profile credential. |
| First-use selection and tab consistency | Apple directs tvOS apps to show a picker when there is no saved selection or when it cannot store one. The HTML Broadcast Channel API and Web Storage `storage` event provide same-origin notification only. | Record whether a new session requires a selection, block the web shell from mounting profile-scoped routes until the user chooses, and propagate a minimal profile-change signal to other same-origin tabs. |
| Android background media | Android Media3 documents a `MediaSessionService` for playback that continues outside the activity. | Android clients consume the ambient queue contract through a native Media3 player/service in the Android/TV phases; web playback is a functional fallback, not a substitute for native background playback. |
| Apple background media | AVFoundation supports queue-based playback and application background audiovisual configuration. | Apple clients consume the same queue contract with `AVQueuePlayer` and the appropriate audio-session/background capability in the iOS/tvOS phases. |

Sources: [OWASP Password Storage Cheat Sheet](https://cheatsheetseries.owasp.org/cheatsheets/Password_Storage_Cheat_Sheet.html), [OWASP Session Management Cheat Sheet](https://cheatsheetseries.owasp.org/cheatsheets/Session_Management_Cheat_Sheet.html), [FTC COPPA guidance](https://www.ftc.gov/business-guidance/resources/complying-coppa-frequently-asked-questions), [Apple TV user preference guidance](https://developer.apple.com/documentation/tvservices/tvusermanager), [RFC 8628 device authorization](https://www.rfc-editor.org/rfc/rfc8628), [Android Media3 background playback](https://developer.android.com/media/media3/session/background-playback), [Apple AVPlayer](https://developer.apple.com/documentation/avfoundation/avplayer), and [Apple media playback configuration](https://developer.apple.com/documentation/avfoundation/configuring-your-app-for-media-playback?changes=la_4__5). The Apple, OWASP, and RFC sources were rechecked on 2026-07-18.

### First-Use Gate Research (2026-07-18)

| Option | Advantages | Limitations | Decision |
|---|---|---|---|
| Rely only on the default active profile | No session/schema change. | A first-use shared client can mount personalized rows before the person chooses; it cannot state whether the default is a deliberate selection. | Rejected. |
| Store the selected profile only in browser storage | Simple local handoff. | It can drift from server session state, cannot safely replace the account-scoped mapping, and does not give native clients an API contract. | Rejected. |
| Server session selection flag plus same-origin client notification | Distinguishes a compatibility default from an explicit current-session choice; lets web and native clients apply the same gate; BroadcastChannel offers direct tab notification and `storage` is a fallback. | Existing sessions are not retroactively made pending, and clients still must implement the gate. | Selected. |

Apple's current `TVUserManager` guidance says to show the picker when preferences cannot be retained or no saved selection exists. The WHATWG HTML Standard defines BroadcastChannel for unrelated same-origin browsing contexts; the Web Storage `storage` event reaches the other same-origin contexts. Duskcue therefore sends only `{ owner_user_id, profile_id, revision }` through those mechanisms—never credentials, PINs, device IDs, media state, or profile policy—and each receiving tab revalidates through the server before rendering profile-scoped data. Sources: [Apple TVUserManager](https://developer.apple.com/documentation/tvservices/tvusermanager), [WHATWG BroadcastChannel](https://html.spec.whatwg.org/multipage/web-messaging.html#broadcasting-to-other-browsing-contexts), [MDN Broadcast Channel API](https://developer.mozilla.org/en-US/docs/Web/API/Broadcast_Channel_API), and [MDN storage event](https://developer.mozilla.org/en-US/docs/Web/API/Window/storage_event).

## Data Model

`users` remains the authenticated account, authorization owner, and Trakt owner. `user_profiles` is the household-facing identity.

```text
users ──< user_profiles ──< profile_library_access
  │            │
  │            ├──< user_item_data
  │            └──< play_sessions
  ├──< user_sessions (active_profile_id)
  └──< profile_device_preferences >── user_profiles

ambient_channels ──< ambient_channel_items >── media_items
```

Each account receives one default standard profile during migration. Existing user-item data is assigned to that default profile, preserving present history. A session has exactly one active profile, and a switch updates that session only.

Kids policies use an allowlist of libraries plus a canonical maximum rating. The first rating ladder is `TV-Y`, `TV-Y7`, `G`, `TV-G`, `PG`, `TV-PG`, `PG-13`, `TV-14`, `R`, `TV-MA`, `NC-17`; the server normalizes known aliases. Ratings that cannot be normalized are inaccessible to a Kids profile. Parent-controlled flags additionally control search, downloads, external links, and ambient-channel access.

## Access Rules

Profile selection is an authorization boundary, not merely a client preference.

1. Every authenticated request resolves the profile from `user_sessions.active_profile_id` and verifies its owner.
2. A Kids profile may only enumerate, resolve, stream, or receive artwork for permitted libraries and normalized ratings at or below its limit.
3. Direct media routes and playback starts perform the same policy check; a copied media ID cannot bypass the browse UI.
4. Standard profiles inherit the authenticated account's library authorization. Kids profiles can only narrow that scope.
5. The owner configures profiles and channels from a standard profile. Leaving a PIN-protected Kids profile for a standard profile requires a valid, unexpired parent unlock on the current server session; this never replaces normal account authentication for remote/API access.

## Remembered Profile on a Device

`profile_device_preferences` stores one explicit profile choice per authenticated account and stable device ID. Its composite foreign key guarantees that the selected profile belongs to the account. A normal password, invite, passkey, device-link, or re-auth session uses this preference only after its account authentication succeeds; without a valid preference, the server starts the session on the account's default profile.

`POST /api/v1/profiles/{id}/switch` accepts an optional `{ "remember_on_device": true | false }` body. `true` creates or replaces the mapping for the current device, `false` removes it, and an omitted value changes only the current session. `GET /api/v1/profiles` and switch responses expose the current device's remembered profile and whether a usable device ID is available, so clients can render an honest opt-in control.

An explicit sign-out or remote session revocation removes that device's remembered mapping. Signing out everywhere and soft-deleting a user remove every mapping for the account. Deleting a profile cascades its mappings. Browser storage contains only a generated opaque device ID; session credentials remain in the normal cookie/bearer-token mechanism, and neither a profile choice nor a future parent-unlock secret is stored on the client.

This is a convenience default, not a parental-control lock or authentication substitute. Remembering a Kids profile never stores or extends a parent unlock, and changing a profile clears the current session's parent-unlock state. tvOS clients must follow the platform's `shouldStorePreferencesForCurrentUser` decision before retaining a profile selection; shared-device clients should default to showing the picker when that capability or a stable device ID is unavailable.

### Shared-TV Startup and Invalidation Rules

A remembered mapping is intentionally per account and per app installation. Each TV app installation, browser profile, or native client installation generates a random opaque `device_id`; it must not derive the value from a hardware serial number, advertising identifier, network address, or a household-wide identifier. Clearing app data, resetting the TV app, or deleting browser storage creates a new device identity and therefore does not inherit a remembered profile. This gives separate TVs independent defaults even when they use the same Duskcue account.

On a device with a valid mapping, the server selects that profile at normal account-session creation and the TV can enter the mapped profile without another picker. When an account has more than one profile but the new session has no mapping, the server starts on the compatibility default and marks the session `profile_selection_required`. A shared-TV client must show a “Who’s watching?” picker before it fetches, displays, or publishes profile-scoped rows, and it must call the existing switch endpoint after the user chooses a profile. Selecting “remember on this TV” creates the mapping; selecting “forget this TV” removes it. A successful switch clears the current session's selection-required flag even if the choice is deliberately not remembered.

Profile changes are privacy-sensitive cache boundaries. Every client must abort in-flight profile-scoped requests and clear active playback previews, Blob/object URLs, artwork and TV-feed caches, platform row mappings, ambient queue state, and client-side profile summaries before it renders the new profile. A browser must propagate a minimal same-origin invalidation signal to other open tabs, which must revalidate their own server session before rendering profile-scoped data; a native TV must clear the same state before publishing launcher rows. Server authorization remains authoritative for every subsequent request.

## Parent PIN and Timed Unlock

`user_profiles.parent_pin_hash` is nullable only to preserve existing Kids profiles during migration. Creating a new Kids profile requires a `parent_pin`; a parent updates an existing Kids profile from a standard profile to enable its lock. The request accepts exactly 4–12 ASCII digits and the server immediately derives an Argon2id PHC string using a random salt. It never returns the hash or the submitted PIN. A standard profile neither accepts nor exposes a parent PIN.

`POST /api/v1/profiles/parent-unlock` acts only on the session's active Kids profile. It locks the session and profile rows, rejects an expired durable lock without a precise `Retry-After`, verifies the PIN, and either records a profile/session-scoped unlock expiring ten minutes later or persists a failed attempt. The fifth consecutive failure sets `parent_pin_locked_until` for fifteen minutes. Success resets the durable failure state. The `GET /api/v1/profiles` response reports whether the active Kids profile currently requires unlock and its configured state, never a PIN, hash, attempt count, or exact lock expiry.

`POST /api/v1/profiles/{id}/switch` locks the session before changing its active profile. If it leaves a PIN-protected Kids profile for a standard profile, the session must carry a matching, unexpired unlock. A successful switch to a different profile clears that session's unlock; updating a Kids PIN clears every outstanding unlock for that Kids profile. This makes the server, rather than browser or TV cache state, authoritative.

This boundary prevents ordinary profile-picker escape on a shared display. It is not MFA and cannot protect an account bearer token or password that a parent gives to a child; remote/API access continues to rely on normal account authentication and authorization.

## Ambient Channel Contract

An ambient channel is a named, ordered list of media items with an audience of `standard` or `kids`. `GET /api/v1/ambient-channels` presents only channels allowed by the active profile. `POST /api/v1/ambient-channels/{id}/next` chooses the next eligible item and returns an ambient playback request.

The client starts that item with `playback_mode: "ambient"` and the channel ID. The server records a `play_sessions` row for diagnostics with `playback_mode = 'ambient'`, but heartbeat, seek, and stop never write `user_item_data` or publish personal TV-surface updates. Ambient sessions are excluded from Trakt export. A Kids channel is filtered through the same library/rating policy before a queue item is returned.

The channel resolver is intentionally deterministic: it advances past the caller's previous item in channel order and wraps. It does not use a random catalog query, so parents can audit exactly what a Kids channel can play.

## Native Client Handoff

The API is player-agnostic: a native client owns the actual background service/session and calls the normal playback start, heartbeat, seek, stop, and channel-next APIs. Android's Media3 `MediaSessionService` and Apple `AVQueuePlayer` are the target native implementations. The server never pretends a browser tab is a native background player.

## Implementation Status

Implemented in the initial server slice:

- profile-aware session identity and profile CRUD/switch APIs;
- opt-in, per-device remembered profiles with login-time defaulting and sign-out/revocation cleanup;
- default-profile migration and per-profile personal playback data;
- server-side Kids policy checks for media listing/detail, search, TV surfaces, direct streams, and playback starts;
- ordered standard/Kids ambient channels, with parent-controlled creation and profile-filtered queue resolution;
- ambient playback accounting that does not update profile history or TV surfaces.

Implemented in the shared-TV hardening follow-up (`c53dabe`):

- `user_sessions.profile_selection_required`, set for an unremembered multi-profile session while retaining the default-profile compatibility fallback;
- atomic switch/remember/forget behavior that clears the selection requirement only after an explicit profile choice;
- a web shell gate that does not mount profile-scoped routes before the choice, plus explicit “forget this device” UI;
- profile-scoped request cancellation, routed-subtree remounting, local playback reset, and same-origin tab revalidation through BroadcastChannel with a Web Storage fallback; and
- an auth fixture and cross-layer integration verifier for the selection state, transactional switch, migration, web gate, and synchronization contract.

Implemented in the Kids parent-unlock hardening follow-up (2026-07-18):

- per-Kids-profile Argon2id PIN hashes using a random salt and the OWASP 19 MiB/two-iteration/single-lane baseline, with no plaintext or reversible value at rest;
- PostgreSQL-backed five-failure/15-minute throttling and a ten-minute profile/session unlock;
- server-side exit enforcement, revocation on profile/PIN changes, a profile-management PIN editor, and a web parent-access prompt that holds the PIN only for submission; and
- a compatibility path for existing unprotected Kids profiles: a parent enables the PIN from a standard profile before treating that profile as locked.

Deferred follow-up:

- native shared-TV picker enforcement and platform cache invalidation; web applies the session flag and same-origin invalidation contract, while each native TV client must apply the same picker and cache-boundary rules before claiming the shared-TV experience;
- profile-specific Trakt account linking/export selection;
- native Android/iOS background-player implementations and offline ambient queue prefetch;
- time windows, daily limits, and child-safe metadata editorial review.

Related documents: [DATABASE.md](DATABASE.md), [TV_PLATFORM_SURFACES.md](TV_PLATFORM_SURFACES.md), [CLIENT_PLATFORM_READINESS.md](CLIENT_PLATFORM_READINESS.md), and [SECURITY.md](../security/SECURITY.md).

# Authentication & User Management Domain

## Overview

This document is the authoritative design for the authentication and user management domain. The system is fully self-contained — no external identity provider, no central auth server, no SSO, no OAuth, no federation. All user accounts live in the local PostgreSQL database.

The server operates in two network modes: **local** (LAN/VPN, auth optional) or **exposed** (internet-facing, auth mandatory). User onboarding is invite-code-based — admins send invite codes to email addresses, and users authenticate by entering the code on any device.

## Architecture

### Authentication Methods

Users can authenticate via four methods, in order of preference:

| Method | Use Case | Entropy | UX |
|---|---|---|---|
| **Passkey (WebAuthn)** | Primary. Phones, laptops, tablets with biometrics | 128+ bits | Single tap/click |
| **Invite code** | Onboarding. Any device with keyboard input | ~103 bits (24 base-20 chars) | Type code + server address |
| **Device linking code** | Constrained-input devices (TVs, consoles) | ~34.5 bits (8 base-20 chars) | Type short code on authenticated device |
| **Password** | Legacy fallback | Depends on password strength | Type username + password |

### Authentication Flow Priority

When a request arrives, the server checks credentials in this order:

1. **Session token** — existing authenticated session (cookie or header)
2. **API key** — `Authorization: Bearer mv_sk_...` header
3. **Passkey assertion** — WebAuthn authentication response
4. **Invite code** — `POST /api/v1/auth/invite` with code + server address
5. **Device linking code** — `POST /api/v1/device/token` with device_code
6. **Password** — `POST /api/v1/auth/login` with username + password

## Network Modes

### Local Mode (`network_mode: "local"`)

Default. For home LAN, VPN, Tailscale, `localhost`.

- Auth is optional — admin can set `auth_required: false`
- When auth is disabled, all requests run as the `owner` user
- HTTPS not enforced (passkeys work on `localhost`)
- No rate limiting
- Invite codes work over HTTP

### Exposed Mode (`network_mode: "exposed"`)

For internet-facing servers via reverse proxy / subdomain.

- Auth is mandatory — `auth_required` forced to `true`
- HTTPS enforced — HTTP requests rejected with redirect
- Secure cookie flags (`Secure`, `SameSite=Strict`)
- Rate limiting active (per-IP and per-user)
- CSRF protection enabled
- Invite code verification rate-limited

### Mode Transition Validation

When switching from local to exposed, the server validates:

1. At least one user has a password or passkey set
2. The `owner` account has a password or passkey
3. HTTPS is configured (via `ssl_*` columns or reverse proxy)
4. `auth_required` is set to `true`

## First-Run Setup Wizard

When the server starts and `users` is empty, it enters **setup mode**:

1. All normal API endpoints return `503 Service Unavailable` with `X-Setup-Required: true`
2. Only `POST /api/v1/setup` is accessible (unauthenticated)
3. Admin submits: `username` (required), `display_name` (required), `password` (optional)
4. Server creates the `owner` account with `role = 'owner'`, `status = 'active'`
5. `server_config.auth.setup_complete` set to `true`
6. `server_config.auth.rp_id` auto-detected from the request's `Host` header
7. Normal auth enforcement begins

If no password is provided, the owner account has `password_hash = null`. They can add a passkey on first login or continue without credentials in local mode.

## Invite Code System

### Overview

Invite codes are the primary mechanism for onboarding users. The admin enters a remote user's email address, and the server sends the code to that email. The code is the user's authentication credential — they enter it on any device along with the server address to authenticate.

Key properties:
- Each code maps to a single user account
- Admins can issue multiple codes to the same email (for different household members)
- Codes are reusable across devices (up to `max_uses`)
- Codes are revocable by the admin
- Codes are sent via email only — never displayed in the admin UI after creation

### Code Format

- Prefix: `mv_invite-`
- Character set: Base-20 (`BCDFGHJKLMNPQRSTVWXZ`) — consonants only, no ambiguous characters. Per RFC 8628 Section 6.1.
- Length: 24 characters (~103 bits entropy), grouped in 4-char blocks with dashes
- Example: `mv_invite-BCDK-MJHT-WDJB-NPQR-STVW-XZBC`

The high entropy (103 bits) makes brute-force infeasible even without rate-limiting. Rate-limiting is still applied as defense-in-depth.

### Invite Code Lifecycle

```
Admin creates invite ──→ Server generates code, hashes it, sends to email
                                                       │
User receives email ──→ Installs app ──→ Enters code + server address
                                                       │
Server validates ──→ First use: creates user account + session
                   ──→ Subsequent use: creates new session for existing user
                                                       │
Admin revokes ──→ All sessions from that code are terminated
```

**Step-by-step:**

1. Admin navigates to User Management, clicks "Invite User"
2. Admin enters: email, display name, role, capabilities, library access, max uses, optional expiry
3. Server generates a cryptographically random 24-char base-20 code using `rand::rngs::OsRng`
4. Server hashes the code (SHA-256), stores the hash + prefix (first 4 chars)
5. Server sends the code to the email address via SMTP
6. Admin UI shows "Invitation sent to `j***@example.com`" with prefix `BCDK-...`

**On the user's device:**

1. User installs app (phone, laptop, tablet)
2. App shows: "Enter your invite code" + "Enter server address"
3. User enters `mv_invite-BCDK-MJHT-WDJB-NPQR-STVW-XZBC` and `media.example.com`
4. App calls `POST /api/v1/auth/invite` with `{ "code": "...", "server": "media.example.com" }`
5. Server validates: hash matches, not revoked, not expired, use_count < max_uses
6. First use: server creates `users` row with invite's role, capabilities, library access
7. Server creates `user_sessions` row, links `invitations.user_id`
8. Subsequent uses: server creates new session for existing user
9. Server increments `use_count`

**On subsequent devices:**

The user can enter the same invite code on additional devices (up to `max_uses`). Each use creates a new session on a new device.

### Multi-Code Per Email

The admin can create multiple invite codes for the same email address. Each code creates a **separate user account** with:

- Separate watch history
- Separate preferences
- Separate capabilities and library access
- Separate streaming policies

Example: Admin creates `mv_invite-BCDK...` for "Dad" and `mv_invite-NPQR...` for "Mom" — both sent to `family@example.com`. Each parent gets their own account.

The email is the delivery mechanism, not the account identifier. Users are identified by their UUID, not their email.

### Rate Limiting

Per RFC 8628 Section 5.1 and OWASP guidelines:

- **Per-IP rate limit**: Max `invite_code_max_attempts_per_ip` (default: 5) failed attempts within `invite_code_attempt_window_minutes` (default: 15 minutes)
- **After lockout**: IP blocked for `lockout_duration_minutes` (default: 30 minutes)
- **Defense-in-depth**: Even without rate-limiting, the 103-bit entropy makes brute-force infeasible
- **Failed attempt logging**: All failed verification attempts are logged with IP, code prefix, and timestamp

### Email Delivery

Invite codes are delivered via email. The server requires SMTP configuration (`server_config.notifications.smtp_*`) before invite codes can be sent. If SMTP is not configured, the admin can still create invites, but the code must be manually copied and shared with the user.

The email contains:

- The invite code (formatted with dashes)
- The server address (from `server_config.server_name` + `base_url`)
- A one-click link: `https://media.example.com/invite?code=mv_invite-BCDK-MJHT-WDJB-NPQR-STVW-XZBC`
- Brief instructions ("Install the app, enter this code and server address")
- Optional: QR code containing the invite link for mobile users

## Device Linking (RFC 8628 Device Authorization Grant)

### Overview

For devices with constrained input (smart TVs, game consoles, streaming sticks), typing a 24-character invite code is impractical. The device linking flow solves this by showing a short 8-character code on the device, which the user enters on their already-authenticated phone/browser.

This follows the RFC 8628 Device Authorization Grant pattern, adapted for our self-hosted architecture. The key adaptation: our server IS the authorization server — there's no separate OAuth provider.

### Code Format

- Character set: Base-20 (`BCDFGHJKLMNPQRSTVWXZ`). Per RFC 8628 Section 6.1.
- Length: 8 characters (~34.5 bits entropy), formatted as `WDJB-MJHT`
- Internal device_code: 32 bytes, hex-encoded (256 bits entropy). Per RFC 8628 Section 5.2.

### Device Linking Flow

```
┌──────────┐                          ┌─────────────────┐
│  Device   │                          │     Server       │
│  (TV)     │                          │                  │
└─────┬─────┘                          └────────┬─────────┘
      │                                          │
      │  POST /api/v1/device/code                │
      │  { client_name, client_platform }        │
      │─────────────────────────────────────────>│
      │                                          │
      │  { user_code, device_code,               │
      │    verification_uri, expires_in,         │
      │    interval }                            │
      │<─────────────────────────────────────────│
      │                                          │
      │  Display: "Visit media.example.com/link" │
      │  Display: "Enter code: WDJB-MJHT"        │
      │                                          │
      │  Start polling (every 5 seconds)         │
      │  POST /api/v1/device/token               │
      │  { device_code }                         │
      │─────────────────────────────────────────>│
      │                                          │
      │  { error: "authorization_pending" }      │
      │<─────────────────────────────────────────│
      │                                          │
      │                   ┌──────────────────┐    │
      │                   │  User's Phone    │    │
      │                   │                  │    │
      │                   │  Visit /link     │    │
      │                   │  Enter WDJB-MJHT │    │
      │                   │  Approve device  │    │
      │                   └──────────────────┘    │
      │                                          │
      │  POST /api/v1/device/token               │
      │  { device_code }                         │
      │─────────────────────────────────────────>│
      │                                          │
      │  { session_token, user }                 │
      │<─────────────────────────────────────────│
      │                                          │
      │  Authenticated!                          │
```

### Polling Behavior

Per RFC 8628 Section 3.5:

| Response | Action |
|---|---|
| `authorization_pending` | Continue polling at interval |
| `slow_down` | Increase polling interval by 5 seconds |
| `access_denied` | Stop polling — user denied |
| `expired_token` | Stop polling — code expired, request new code |

The device must use exponential backoff on connection timeouts. The default polling interval is 5 seconds.

### Security Considerations

Per RFC 8628 Section 5:

- **User code brute forcing (Section 5.1)**: 8-char base-20 = ~34.5 bits entropy. With rate-limiting (5 attempts per 15 minutes), brute-force probability = 2^-32. Adequate for a Duskcue.
- **Device code brute forcing (Section 5.2)**: 256-bit internal code — infeasible to brute-force.
- **Remote phishing (Section 5.4)**: 15-minute expiry limits phishing viability. Server displays device info during approval.
- **Session spying (Section 5.5)**: User should confirm the code on the TV matches the code they're entering on their phone.
- **Device trustworthiness (Section 5.3)**: Server displays client name and platform during approval so the user can verify the device.

## WebAuthn Passkey Support

### Relying Party ID Configuration

Passkeys are cryptographically bound to a domain (Relying Party ID). The RP ID is stored in `server_config.auth.rp_id`:

| Scenario | RP ID | RP Origin |
|---|---|---|
| Local: `http://localhost:48027` | `localhost` | `http://localhost:48027` |
| Local: `http://192.168.1.100:48027` | `192.168.1.100` | `http://192.168.1.100:48027` |
| Exposed: `https://media.example.com` | `example.com` | `https://media.example.com` |
| Exposed: `https://media.example.com:8443` | `example.com` | `https://media.example.com:8443` |

Rules:
- Auto-detected from Host header during setup
- Admin can override to use the root domain for subdomain support
- Changing RP ID **breaks all existing passkeys** — server warns before allowing changes
- WebAuthn requires HTTPS except for `localhost` — server warns if passkeys are enabled without HTTPS

### Passkey Registration Flow

1. User navigates to Settings > Security > Add Passkey
2. Server generates a WebAuthn registration challenge
3. Browser/platform shows biometric prompt (Face ID, fingerprint, Windows Hello)
4. Authenticator generates a new key pair
5. Public key + credential ID stored in `user_passkeys`
6. Private key stays on the user's device (synced via iCloud Keychain, Google Password Manager, etc.)

### Passkey Authentication Flow

1. User selects "Sign in with passkey" on the login screen
2. Server generates a WebAuthn authentication challenge
3. Browser/platform shows account picker + biometric prompt
4. Authenticator signs the challenge with the private key
5. Server verifies the signature against the stored public key
6. Session created

## Password Authentication

Passwords are a legacy fallback for users who can't or won't use passkeys.

- **Hashing**: Argon2id (OWASP recommended, minimum 19 MiB memory, 2 iterations, 1 parallelism)
- **Nullable**: `password_hash` can be null (passkey-only accounts or invite-code-only accounts)
- **Account lockout**: After `max_login_attempts` (default: 5) consecutive failures, account locked for `lockout_duration_minutes` (default: 30)
- **Password requirements**: Minimum 8 characters. No complexity rules (per NIST SP 800-63B).

## TOTP Two-Factor Authentication

Optional second factor for password-based authentication.

- Standard TOTP (RFC 6238) — compatible with Google Authenticator, Authy, 1Password, etc.
- Encrypted secret stored in `user_totp`
- Backup codes (hashed, single-use) generated on setup
- Must complete one successful TOTP challenge during setup before enabled

## Capability-Based Access Control

### Roles (Default Capability Bundles)

| Role | Description | Default Capabilities |
|---|---|---|
| `owner` | Server owner. Irrevocable. Full access. | All capabilities. Cannot be demoted. |
| `admin` | Trusted administrator. | All except ownership transfer. |
| `member` | Standard user. | `play_media`, `download`, `share_content` |
| `guest` | Restricted access. | `play_media` only |

### Capabilities (Atomic Permissions)

| Capability | Description |
|---|---|
| `play_media` | Play any accessible media |
| `can_transcode` | Request transcoded streams (CPU-intensive) |
| `can_download` | Download media files |
| `can_delete_media` | Delete media from disk |
| `can_manage_libraries` | Create, edit, scan, and delete libraries |
| `can_manage_users` | Create, edit, and delete users |
| `can_view_analytics` | Access analytics dashboard and play history |
| `can_manage_server` | Access server settings, configuration, and logs |
| `can_manage_scheduled_tasks` | Create, edit, and trigger scheduled tasks |
| `can_use_live_tv` | Access live TV features (future) |
| `can_share_content` | Share content links externally |
| `can_remote_control` | Remote control other users' playback sessions |

### Capability Evaluation

1. Check `user_capabilities` for an explicit override
2. If found, use the override (`is_granted = true` or `false`)
3. If not found, use the role's default capability set
4. `owner` role always has all capabilities, regardless of overrides

Most users have zero rows in `user_capabilities` — they use role defaults. Only users with customized permissions need rows.

## Library Access Control

Users see only the libraries they have been granted access to:

- `users.has_all_library_access = true` — access to all current and future libraries
- `users.has_all_library_access = false` — only libraries in `user_library_access`
- Owner always has all library access (enforced by application)

Invite codes pre-configure library access via `invitations.library_ids` and `invitations.has_all_library_access`.

## Session Management

### Session Lifecycle

1. Created on successful authentication (any method)
2. Updated on each authenticated request (`last_active_at`, throttled)
3. Invalidated on: password change, role change, explicit logout, invite code revocation, expiry
4. Cleaned up by scheduled task (`session_cleanup`)

### Session Properties

- `token_hash` — SHA-256 hash of the session token. Raw token sent to client once, never stored.
- `device_id` — client-generated stable identifier for grouping sessions by device
- `is_secure` — set to `true` when created over HTTPS in exposed mode
- `expires_at` — based on configurable timeouts (see below)

### Session Timeouts

Per NIST SP 800-63B (August 2025) and OWASP Session Management Cheat Sheet:

| Timeout | Local Default | Exposed Default | NIST Reference |
|---|---|---|---|
| **Absolute** | 90 days | 30 days | AAL1: SHALL be ≤30 days |
| **Idle** | None | 7 days | AAL1: not required; AAL2: ≤1 hour |
| **Renewal** | 30 days | 7 days | OWASP: regenerate session ID periodically |

- **Absolute timeout** (`session_absolute_timeout_days`): Maximum session lifetime regardless of activity. After this, the user must re-authenticate.
- **Idle timeout** (`session_idle_timeout_hours`): Session expires after inactivity. `None` = no idle timeout (local mode default). Per OWASP, idle timeouts limit the window for session hijacking.
- **Renewal timeout** (`session_renewal_timeout_hours`): Session ID is regenerated periodically during active sessions. The old session ID remains valid briefly during the transition. Per OWASP, this minimizes the time a stolen session ID can be reused.

All timeouts are enforced server-side. Client-side cookie expiry is set to match but is not trusted.

### Authorized Devices

Sessions are device-centric. Each session represents one authenticated device. The "Authorized Devices" view in the user and admin console is a view over `user_sessions`.

**User view** (Settings > Authorized Devices):

| Column | Source |
|---|---|
| Device Name | `user_sessions.device_name` |
| Platform | `user_sessions.client_platform` |
| App | `user_sessions.client_name` + `client_version` |
| IP Address | `user_sessions.ip_address` |
| Last Active | `user_sessions.last_active_at` |
| Authorized | `user_sessions.created_at` |
| Secure | `user_sessions.is_secure` |
| Actions | "Sign Out" button |

**Admin view** (Admin > Users > {user} > Devices): Same columns plus the username.

**Revocation:**
- "Sign Out" on a single device → deletes that `user_sessions` row → device must re-authenticate
- "Sign Out Everywhere" → deletes ALL `user_sessions` for the user + generates a re-auth code (see below)

### Session Anomaly Detection

Per OWASP, sessions should be bound to client properties and anomalies detected mid-session:

| Signal | Detection | Action |
|---|---|---|
| IP address change | `ip_address` differs between requests in the same session | Log trust event (warning level). In exposed mode: terminate session. |
| User-Agent change | `user_agent` differs between requests | Terminate session immediately. Require re-authentication. |
| Impossible travel | Two sessions for same user from geographically distant IPs within short timeframe | Flag trust event. Admin notified. Sessions NOT terminated (may be VPN). |
| `is_secure` upgrade | Session created over HTTP but request comes over HTTPS | Upgrade session to `is_secure = true`. No action needed. |

These integrate with the existing trust scoring system (`user_trust_events`). Anomaly detection is always active in exposed mode. In local mode, only User-Agent changes trigger termination (IP changes are expected on LAN).

### Account Recovery (Compromised Account)

When a user suspects their account is compromised, or when an admin detects suspicious activity, the "Sign Out Everywhere" + re-authentication code flow provides a clean slate.

**"Sign Out Everywhere" flow:**

1. User (or admin on their behalf) triggers `POST /api/v1/auth/logout-all`
2. Server deletes ALL `user_sessions` for that user
3. Server revokes all active `invitations` linked to that user (`is_revoked = true`)
4. Server expires all active `device_linking_codes` for that user
5. Server generates a re-authentication code (16 base-20 chars, `mv_reauth-` prefix)
6. Server sends the re-auth code to the user's registered email
7. User enters `mv_reauth-BCDK-MJHT-WDJB-NPQR` + server address on their trusted device
8. Server validates: code hash matches, not expired, not used
9. Server creates a new session, marks the code as consumed (`is_used = true`)
10. User repeats for each additional trusted device

**Re-authentication code properties:**

- Format: `mv_reauth-` + 16 base-20 characters (~69 bits entropy)
- Example: `mv_reauth-BCDK-MJHT-WDJB-NPQR`
- Lifetime: 24 hours (configurable via `server_config.auth.reauth_code_expiry_hours`)
- Single use: consumed after first authentication
- Rate-limited: max 3 requests per user per 24 hours
- Can be requested by: the user themselves, or an admin on their behalf

**Admin-initiated recovery:**

Admins can trigger "Sign Out Everywhere" for any user via `DELETE /api/v1/users/{id}/sessions`. The admin does NOT receive the re-auth code — it's sent to the user's email. The admin can also request a new re-auth code on behalf of the user via `POST /api/v1/users/{id}/reauth`.

Per NIST SP 800-63B Section 4.2: after account recovery, all existing sessions must be terminated because the subscriber's authenticators may be compromised.

## API Keys

For third-party integrations (Classifarr, automation scripts, custom clients):

- Scoped to specific capabilities (cannot exceed owning user's capabilities)
- Format: `mv_{type}_{random}` where type is `sk` (secret key) or `pk` (public key — read-only)
- Key prefix (first 8 chars) stored in plaintext for identification
- Full key hashed with Argon2id
- Optional expiry
- Last-used tracking

## API Endpoints

For API conventions (versioning, pagination, rate limiting, authentication headers, error format), see [API_CONVENTIONS.md](API_CONVENTIONS.md).

### Setup

| Method | Endpoint | Auth Required | Description |
|---|---|---|---|
| `POST` | `/api/v1/setup` | No (setup mode only) | Create owner account during first-run |

### Authentication

| Method | Endpoint | Auth Required | Description |
|---|---|---|---|
| `POST` | `/api/v1/auth/invite` | No | Authenticate with invite code + server address |
| `POST` | `/api/v1/auth/login` | No | Authenticate with username + password |
| `POST` | `/api/v1/auth/webauthn/start` | No | Begin WebAuthn authentication (get challenge) |
| `POST` | `/api/v1/auth/webauthn/finish` | No | Complete WebAuthn authentication (submit assertion) |
| `POST` | `/api/v1/auth/totp` | No | Submit TOTP code (second factor) |
| `POST` | `/api/v1/auth/logout` | Yes | Terminate current session |
| `POST` | `/api/v1/auth/logout-all` | Yes | Terminate all sessions for current user, send re-auth code |
| `POST` | `/api/v1/auth/reauth` | Yes | Authenticate with re-auth code + server address |
| `POST` | `/api/v1/auth/reauth/request` | Yes | Request a new re-auth code sent to email |

### Device Linking (RFC 8628)

| Method | Endpoint | Auth Required | Description |
|---|---|---|---|
| `POST` | `/api/v1/device/code` | No | Request device linking codes (device initiates) |
| `POST` | `/api/v1/device/token` | No | Poll for device authorization (device polls) |
| `POST` | `/api/v1/device/verify` | Yes | Enter user_code to approve device (user approves) |

### Invite Management (Admin)

| Method | Endpoint | Auth Required | Description |
|---|---|---|---|
| `GET` | `/api/v1/invitations` | Yes (`can_manage_users`) | List all invitations |
| `POST` | `/api/v1/invitations` | Yes (`can_manage_users`) | Create invitation (sends email) |
| `DELETE` | `/api/v1/invitations/{id}` | Yes (`can_manage_users`) | Revoke invitation |
| `POST` | `/api/v1/invitations/{id}/resend` | Yes (`can_manage_users`) | Resend invitation email |

### User Device Management

| Method | Endpoint | Auth Required | Description |
|---|---|---|---|
| `GET` | `/api/v1/user/sessions` | Yes | List current user's active sessions (authorized devices) |
| `DELETE` | `/api/v1/user/sessions/{id}` | Yes | Sign out a specific device |
| `POST` | `/api/v1/user/sign-out-everywhere` | Yes | Sign out all devices, send re-auth code to email |
| `POST` | `/api/v1/user/request-reauth` | Yes | Request a new re-auth code (without signing out everywhere) |

### Passkey Management

| Method | Endpoint | Auth Required | Description |
|---|---|---|---|
| `GET` | `/api/v1/user/passkeys` | Yes | List user's registered passkeys |
| `POST` | `/api/v1/user/passkeys/register/start` | Yes | Begin passkey registration (get challenge) |
| `POST` | `/api/v1/user/passkeys/register/finish` | Yes | Complete passkey registration (submit credential) |
| `DELETE` | `/api/v1/user/passkeys/{id}` | Yes | Remove a passkey |

### User Management (Admin)

| Method | Endpoint | Auth Required | Description |
|---|---|---|---|
| `GET` | `/api/v1/users` | Yes (`can_manage_users`) | List all users |
| `GET` | `/api/v1/users/{id}` | Yes (`can_manage_users`) | Get user details |
| `PUT` | `/api/v1/users/{id}` | Yes (`can_manage_users`) | Update user (role, capabilities, library access) |
| `DELETE` | `/api/v1/users/{id}` | Yes (`can_manage_users`) | Soft-delete user |
| `GET` | `/api/v1/users/{id}/sessions` | Yes (`can_manage_users`) | List user's active sessions (authorized devices) |
| `DELETE` | `/api/v1/users/{id}/sessions/{session_id}` | Yes (`can_manage_users`) | Revoke specific device session |
| `DELETE` | `/api/v1/users/{id}/sessions` | Yes (`can_manage_users`) | Revoke all sessions for user ("Sign Out Everywhere") |
| `POST` | `/api/v1/users/{id}/reauth` | Yes (`can_manage_users`) | Generate re-auth code for user (sent to their email) |

## Security Checklist

Per OWASP Secure Coding Practices:

- [x] Authentication enforced on trusted system (server-side)
- [x] Centralized authentication implementation
- [x] Authentication controls fail securely (deny by default)
- [x] Cryptographically strong one-way salted hashes (Argon2id)
- [x] Authentication failure responses don't indicate which part was incorrect
- [x] POST requests for authentication credentials
- [x] Account disabling after invalid login attempts
- [x] Temporary codes have short expiration (invite: 30 days, device: 15 minutes)
- [x] Cryptographic random number generation for all codes (`OsRng`)
- [x] Rate-limiting on code verification attempts
- [x] Audit logging of all authentication attempts
- [x] Session identifier is cryptographically random, hashed server-side
- [x] New session on re-authentication
- [x] Secure and HttpOnly cookie flags in exposed mode
- [x] TLS required for exposed mode
- [x] Session security varies by network tier — see [SECURITY.md](../security/SECURITY.md) for cookie configuration per tier (local/VPN/exposed)
- [x] Configurable session timeouts (absolute, idle, renewal) per NIST SP 800-63B
- [x] "Sign Out Everywhere" terminates all sessions + sends re-auth code (NIST Section 4.2)
- [x] Session anomaly detection (IP/UA change) per OWASP Session Management Cheat Sheet
- [x] Re-authentication after risk events (password change, account recovery) per OWASP
- [x] Re-auth code rate limiting (3 per user per day)
- [x] Device-centric session view for user and admin consoles

Per RFC 8628:

- [x] Base-20 character set for user-facing codes (no ambiguous characters)
- [x] High-entropy internal device codes (256 bits)
- [x] Rate-limiting on user code verification (Section 5.1)
- [x] Short user code lifetime (15 minutes, Section 5.4)
- [x] Device info displayed during approval (Section 5.3)
- [x] Polling with exponential backoff on timeout

## Database Tables

Full DDL is in [DATABASE.md](DATABASE.md) — User & Authentication Domain section:

- `users` — Core identity table
- `user_passkeys` — WebAuthn credential storage
- `user_totp` — TOTP second factor
- `user_capabilities` — Per-user capability overrides
- `user_library_access` — Per-user per-library grants
- `user_sessions` — Active authentication sessions
- `api_keys` — Scoped integration tokens
- `invitations` — Invite code system (email-delivered, reusable, multi-code per email)
- `device_linking_codes` — RFC 8628 device linking (short codes for constrained-input devices)
- `reauth_codes` — Re-authentication codes (account recovery, "Sign Out Everywhere", compromised account)
- `streaming_policies` — Reusable streaming restriction templates

## Configuration

Auth configuration is stored in `server_config.auth` JSONB column. See [CONFIGURATION.md](../operations/CONFIGURATION.md) for the `AuthConfig` Rust struct and field semantics.

## Error Codes

Auth error codes are defined in [ERROR_HANDLING.md](ERROR_HANDLING.md):

| Code | HTTP | Description |
|---|---|---|
| `AUTH_001` | 401 | Passkey not found |
| `AUTH_002` | 401 | Invalid passkey signature |
| `AUTH_003` | 401 | TOTP verification failed |
| `AUTH_004` | 403 | Account locked |
| `AUTH_005` | 401 | Session expired |
| `AUTH_006` | 401 | Invalid credentials |
| `AUTH_007` | 403 | Insufficient capabilities |
| `AUTH_008` | 401 | API key invalid or revoked |
| `AUTH_009` | 401 | Invite code invalid or expired |
| `AUTH_010` | 401 | Invite code revoked |
| `AUTH_011` | 401 | Invite code use limit exceeded |
| `AUTH_012` | 429 | Too many failed attempts (rate limited) |
| `AUTH_013` | 400 | Device linking code expired |
| `AUTH_014` | 400 | Device linking denied by user |
| `AUTH_015` | 401 | Re-authentication code invalid or expired |
| `AUTH_016` | 429 | Too many re-auth code requests (rate limited) |

## Implementation Notes (Phase 4, Task 1)

### Password Hashing

AUTH.md specifies **Argon2id** (OWASP recommended) for password hashing. The initial implementation uses **PBKDF2-HMAC-SHA256 with 600,000 iterations via `ring::pbkdf2`** instead. Rationale:

- The `ring` crate is already in the workspace (TLS backend, HMAC signing, timing-safe comparisons)
- Adding `argon2` would introduce a new dependency for one function
- PBKDF2 with 600,000 iterations meets OWASP 2023 guidelines for PBKDF2-SHA256
- Migration path to Argon2id is straightforward: check hash prefix (`$argon2id$` vs hex-encoded PBKDF2) and verify accordingly
- This decision should be revisited before release if Argon2id is a hard requirement

### Session Token Generation

- 32 cryptographically random bytes via `rand` 0.9 (`OsRng`)
- Hex-encoded (64 chars) for the client-facing token
- SHA-256 hash stored in `user_sessions.token_hash` — raw token never stored
- Absolute timeout from `AuthConfig.session_absolute_timeout_days` applied at session creation

### SQL Queries

All auth service queries use **runtime `sqlx::query`** with `Row::get()` rather than compile-time `sqlx::query!` macros. This avoids requiring a running PostgreSQL instance with `DATABASE_URL` at build time. See BUILD_ORDER.md Phase 4 Task 1 for details.

### Auth Error Integration

`AuthError` (23 variants) integrates with the central `AppError` enum via `AppError::Auth(#[from] AuthError)`. The mapping function `auth_error_to_http()` in `server/src/error.rs` converts all 22 auth error codes to HTTP status codes per ERROR_HANDLING.md. The `Database` variant wraps `sqlx::Error` for internal DB failures.

### WebAuthn Crate

Uses `webauthn-rs` (by kanidm) — a mature, security-audited server-side Relying Party library. Originally `passkey-auth` was recommended during Task 1 (pure Rust, aligns with `ring`/`rustls` strategy), but research in Task 2 revealed that `passkey-auth` and `passkey-rs` are **client-side** libraries (implementing WebAuthn client/authenticator, not server-side Relying Party verification).

**Why `webauthn-rs`:**
- Server-side Relying Party implementation with safe, high-level API
- Security-audited by SUSE product security
- Supports all major authenticator types (YubiKey, TouchID, FaceID, Windows Hello, Android)
- Passkey flow API: `start_passkey_registration` → `finish_passkey_registration` → `start_passkey_authentication` → `finish_passkey_authentication`
- Feature `danger-allow-state-serialisation` enabled to persist challenge state in DB (safe since we store server-side, not in client cookies)
- `Webauthn` instance stored in `AppState`, constructed from `AuthConfig.rp_id` and `AuthConfig.rp_origin`

**Challenge state persistence:**
- Registration state (`PasskeyRegistration`) and authentication state (`PasskeyAuthentication`) are serialized to JSON and stored in a transient in-memory `DashMap` keyed by challenge ID with 5-minute TTL
- Each challenge is single-use — consumed on `finish` and removed from the map
- Alternative DB storage considered but in-memory is sufficient for single-instance deployment; multi-instance would require a shared store (Redis/PG) — deferred to future horizontal scaling work

# Household-owner management security

## Status and authority

**Qualified source:** `a2292e37e03cbf079a55626c83b76a4f4c30cfa3` on `main`. Later documentation-only checkpoints do not change the verified code. No release or PR merge.

Research and design review started October 24, 2026. The user confirmed the complete concrete backend contract below on October 24. The backend foundation is implemented and locally qualified; [BUILD_ORDER.md](../../BUILD_ORDER.md) records its source checkpoint. [IPTV](../design/IPTV.md) owns household/Dispatcharr product behavior; this document owns the shared owner-proof and credential-lifecycle boundary. [Authentication](../design/AUTH.md) owns ordinary account login. The existing Kids exit PIN remains a separate mechanism.

The repository [grill-me entry point](../../.agents/skills/grill-me/SKILL.md) loads [grilling](../../.agents/skills/grilling/SKILL.md). Its shared-understanding confirmation is required before implementing the interviewed plan. Continue on `main`, without a release or PR merge.

## Client follow-up — October 26, 2026

[Household management client](../design/HOUSEHOLD_MANAGEMENT_CLIENT.md) owns the next screen and session-adoption design. The user confirmed owner-security UI before Live TV forms, a dedicated standard-profile settings surface, browser passkeys/native password proof first, and an authoritative session-context read. Its full contract awaits confirmation. The backend trust, operator recovery and five-minute-grant rules below remain authoritative; no new client qualification is claimed here.

## Existing behavior and the trust gap

- One `users` account owns its `user_profiles`; there is no separate household identity table. Database role `owner` means server superuser. It does not identify the person designated to manage a household.
- `AuthenticatedUser` validates cookie/bearer sessions and current account/profile state. Session rows record no authenticating factor or enrollment provenance. A setup/password/passkey/invite/device session cannot later be distinguished as owner-held.
- Existing passkey registration and deletion require only an authenticated session. A shared viewing device can enroll a passkey. Existing account passkeys therefore cannot all be promoted to management factors automatically.
- Only first-run setup currently writes a password hash; invited accounts have no password. The unused change-password request type is not an implemented password-management or recovery flow. There is no existing operator authenticator command.
- Existing profile administration refuses Kids mode but accepts an ordinary standard-profile session. It includes profile creation/deletion, rating/library/feature restrictions and Kids PIN replacement. Protecting only a new IPTV route would leave those related mutations outside the proposed boundary.
- Session deletion is reusable for grant invalidation by foreign-key cascade. Profile switching already locks session state and clears Kids exit-unlock state. Management authority needs independent invalidation, including a switch away and back to the same profile.

Concrete source boundaries: `server/src/extractors.rs`; `server/src/domains/auth/{handlers,service,types}.rs`; `server/src/domains/profiles/{handlers,service}.rs`; `server/src/state.rs`; migrations `20260530040000_create_auth_domain.sql` and `20260712080000_create_profiles_and_ambient_channels.sql`.

## Official-source research — October 2026

URLs were discovered through MCP web search and opened October 24. Current published specifications are distinguished from older snapshots and installed-library behavior.

| Source | Recommendation and benefit | Cost or alternative |
|---|---|---|
| [W3C WebAuthn Level 3](https://www.w3.org/TR/webauthn-3/), Recommendation of August 25, 2026 | Use fresh server challenges, expected account/credential, origin/RP checks and required user verification for owner passkey proof. Separate proof purposes and consume challenges once. | An assertion proves the selected credential; it does not establish that a credential enrolled by a viewing device belongs to the household owner. January's Candidate Recommendation is not the current publication. |
| [OWASP authentication](https://cheatsheetseries.owasp.org/cheatsheets/Authentication_Cheat_Sheet.html), [session management](https://cheatsheetseries.owasp.org/cheatsheets/Session_Management_Cheat_Sheet.html) | Reauthenticate sensitive operations and rotate session identity when elevating privilege. Keep ordinary viewing and bounded management authority distinct. | Adds an explicit unlock flow and client handling for a replacement session. Reusing every signed-in session as proof would retain the trust gap. |
| [NIST SP 800-63B-4 authenticator events](https://pages.nist.gov/800-63-4/sp800-63b/events/) | Treat factor binding and recovery as sensitive lifecycle operations; record significant events, limit one-use handoffs and notify through an independent established channel. | Duskcue has no verified recovery-address/notification channel for every household. An operator handoff is an application-specific proposal, not a claim of NIST assurance-level compliance. |
| [WCAG 2.2 accessible authentication](https://www.w3.org/WAI/WCAG22/Understanding/accessible-authentication-minimum.html) | Later owner prompts must allow password-manager filling and paste, provide labeled inputs, keyboard/focus behavior and accessible errors. Passkey ceremonies should use the platform's accessible authentication. | Backend signature/HTTP tests provide no UI accessibility conformance evidence. Avoid transcription-only handoff controls. |
| [Installed WebAuthn API family](https://docs.rs/webauthn-rs/latest/webauthn_rs/struct.Webauthn.html) | Use account-bound authentication with an explicit list of eligible stored `Passkey` values and persist credential updates after verified assertions. | Current docs describe 0.5.5; installed 0.6.1-dev source was checked separately. Its supported account-bound path requires user verification. No dependency upgrade is inferred. |
| [OWASP cryptographic storage](https://cheatsheetseries.owasp.org/cheatsheets/Cryptographic_Storage_Cheat_Sheet.html), [ring AEAD](https://docs.rs/ring/latest/ring/aead/struct.LessSafeKey.html) | For the subsequent assignment increment, use strict authenticated encryption bound to household/record/field/version rather than plaintext-compatible secret helpers. | Current AES-GCM helpers use empty associated data. Contextual binding and fail-closed decoding require a focused extension and their own tests; this is separate from password hashing. |

## Recommendation stack and tradeoffs

Retain Rust/Axum, PostgreSQL, the installed WebAuthn verifier and existing PBKDF password verification. Compose small owner-security services around those components; no new dependency or parallel authentication framework is needed.

| Selected approach | Benefit | Cost |
|---|---|---|
| Explicit operator handoff plus fresh factor proof | Establishes owner-factor provenance despite shared viewing sessions and previously enrolled keys. Recovery remains possible without an old factor. | The server operator is the trust root and must deliver the secret securely; independent notifications remain future work. |
| Absolute five-minute grants, external token rotation and credential generations | Limits authority duration and rejects both old tokens and requests authenticated before elevation. Internal session bindings survive. | Consumers must adopt replacement tokens and unlock again after expiry or invalidation. |
| PostgreSQL transactions, authority revisions and scoped one-use challenges | Keeps sensitive writes, recovery and revocation under one serialized account boundary; works across server instances. | Adds short database locks, bounded challenge storage and additional queries. |
| Account-bound WebAuthn with required user verification | Uses the existing verifier for challenge, origin, RP and signature checks, with application checks for account handle and supported client context. | Requires a compatible passkey client. Software-authenticator evidence does not qualify physical devices or the future unlock UI. |
| Conditional activation and backend APIs first | Preserves existing unprotected households while completing the shared security boundary before Dispatcharr assignment. | Activation requires the documented API flow until an accessible client unlock screen is implemented. |

For the next increment, extend the existing ring AEAD service with household/record/field/version associated data, then build encrypted Dispatcharr assignment and administrator-defined delegation on this authority revision. Keep upstream feed networking disabled until destination authority and transport qualification are complete. WCAG authentication, labeling, focus and status requirements apply when implementing the later client flow.

## Design tree and decisions

Confirmed in this review:

1. Successful proof creates a five-minute server-side grant for the exact signed-in session. No automatic extension. Logout, profile switching, credential changes and delegation changes invalidate it.
2. Losing all trusted owner factors leaves management locked until an explicit server-operator recovery handoff. Viewing-device, invite and device-link codes do not count as owner proof. Recovery revokes prior management grants.

3. Use an operator-issued, account-bound, single-use ten-minute handoff plus fresh proof to designate the initial owner factor; previously enrolled passkeys receive no automatic owner trust.
4. Once owner protection is enabled, gate related existing profile/restriction/PIN and passkey mutations through the new owner boundary.
5. Complete the backend owner-security foundation in this increment. Encrypted Dispatcharr assignment/delegation follows in the next increment.
6. Activate owner protection only after the first operator-authorized factor is successfully proven. Other households retain their current behavior. Activation is explicit through the documented backend/operator flow; the interactive unlock screen follows separately.
7. Recovery may register a fresh replacement passkey under the operator handoff. Issuing recovery immediately revokes old owner trust and management grants. Protection stays enabled and management stays locked until proof succeeds; ordinary viewing access remains separate.

Shared understanding of the complete concrete contract below is confirmed. Continue implementation under the existing authorization.

## Confirmed concrete backend contract

### Operator, activation and factors

Add a focused `duskcue owner-handoff --user <account UUID>` command to the existing server binary, using its established database/bootstrap configuration. It issues a 256-bit random, account-bound, one-use token with a ten-minute expiry; only its hash is stored. The raw handoff is printed once to the operator's command output and is never included in application diagnostics or audit data. There is no HTTP route for a viewing/admin session to issue handoffs. Operators remain the explicit trust root and choose the secure handoff channel.

For an unprotected household, successful handoff plus fresh existing-password proof or account-bound passkey proof designates that selected factor and activates protection atomically. Handoff-authorized passkey registration also supports accounts without a usable existing factor. An unfinished/expired initial handoff does not silently activate protection. Issuing another handoff invalidates the previous one. For an already protected household, issuance is recovery: revoke owner designations, grants and challenges immediately while retaining protection.

Store passkey owner eligibility separately from ordinary login usability, defaulting every existing passkey to ineligible. Password eligibility is bound to a fingerprint of the existing stored password hash; changing that hash invalidates its eligibility and grants. Protected passkey enrollment requires a current owner grant and creates an owner-eligible key. Factor changes revoke management authority. Refuse removal of the last usable trusted factor; operator recovery remains available. No new password-setting/reset API, automatic trust of preexisting passkeys, additional management PIN or ordinary-login redesign is included.

### Session proof and mutation boundary

The `household_security` domain follows the five-file HTTP/service/error/types/module convention, with focused handoff, factor, challenge, grant and transaction helpers. Expose authenticated, no-store status/password/passkey-start/passkey-finish/lock APIs under `/api/v1/household-management`. Require a selected standard profile for proof. Secret-bearing requests do not implement `Debug` or `Serialize`; errors are fixed and request bodies are bounded.

Passkey start selects only the session account's eligible factor(s), or the exact handoff-authorized enrollment factor. Store server-side challenge state bound to account, session, purpose and authority revision, with a two-minute expiry, at most eight pending per account and 4,096 globally. Finish consumes it once, verifies challenge/origin/RP/user presence/user verification/signature through the installed library, rechecks the current factor/revision and persists verified counter/backup updates. A present assertion user handle must match the authenticated account's UUID bytes; omission is allowed for this account-bound ceremony. Client data is capped at 16 KiB; unsupported cross-origin/top-origin context is refused. A ceremony cannot be reused across sessions, purposes, revisions or recovery handoffs.

Successful proof rotates the external session token and credential generation, preserves the internal session UUID, viewing scope and expiry, and returns the correct cookie/bearer replacement. Transaction-time generation checks also refuse an in-flight request authenticated before rotation. Retaining the internal UUID preserves existing download/session bindings. The replacement session receives an absolute five-minute management grant without sliding renewal. No extra reusable owner credential is returned. Five failed proofs cause a fifteen-minute account-scoped management lockout; this applies in local mode too. Bounded proof execution retains capacity until its cryptographic work ends.

For protected households, profile create/update/delete and passkey registration/deletion must validate current grant, session, active standard profile, account status and authority revision inside the same transaction as their mutation. Lock/check/write ordering must linearize concurrent enrollment, expiry, revocation and mutation. Session deletion cascades grants; profile changes invalidate them even when switching away and back. Password/trusted-factor changes revoke authority. Future delegation must use the same revision invalidation boundary.

Unprotected households retain their established API behavior. Protected households use the new documented API unlock flow before existing protected mutations; this increment does not claim a new interactive client prompt. Viewing preferences, playback, history, ordinary profile selection and the existing Kids exit-PIN purpose retain their current contracts. Add safe, attributed audit events for handoff, enrollment/recovery, proof, lock and factor changes. Independent notification delivery remains a later qualification and no NIST assurance-level claim is made.

### Acceptance and deferred work

Use real production routes and disposable PostgreSQL to prove password verification, genuine software-authenticator registration and signed assertions, cookie/bearer rotation, protected and legacy behavior, activation/recovery, last-factor refusal, challenge/handoff replay and cross-scope refusals, expiry, switch-away-and-back, direct API attempts, concurrent revocation/mutation and safe diagnostics/audit. Registration uses none attestation: it exercises real parsing and credential enrollment without claiming signed hardware provenance. Run the existing Rust library and relevant profile/credential regression cases under unchanged serial resource guards; record baseline lint limitations honestly.

Encrypted Dispatcharr assignment/delegation, guide import, production Live TV, a client unlock screen, actual browser/hardware/Tauri owner ceremonies, independent notification delivery and general passkey-login repair remain separate increments. Standard legal headers on new files follow the user's standing exception; no explanatory code comments are introduced. Commit and push the completed implementation/documentation on `main`; no branch, release or PR merge.

## Implemented interfaces and storage

The five-file `household_security` domain composes focused operator, transaction, factor, challenge and grant services. Existing profile writes move to `profiles/management.rs`; existing credential enrollment/removal moves to `auth/service/passkeys.rs`. Existing readers, preferences and ordinary login retain their domain ownership. No new dependency is introduced.

| Method and path | Request and behavior |
|---|---|
| `GET /api/v1/household-management/status` | Selected standard profile; returns `protection_enabled`, current grant `expires_at`, `password_available` and `trusted_passkeys`. It grants no authority. |
| `POST /api/v1/household-management/password` | `{ password, handoff? }`. A valid handoff designates the freshly proven existing password; otherwise the password must already be eligible. Password bytes are capped at 1,024 and the body at 64 KiB. |
| `POST /api/v1/household-management/passkey/start` | `operation: unlock` selects eligible account keys; `enroll` additionally requires `handoff` and `passkey_id`; `register` requires `handoff` and a nonempty `name` of at most 200 characters. Returns an opaque challenge ID and WebAuthn `options`. |
| `POST /api/v1/household-management/passkey/finish` | `{ challenge_id, credential }` completes the exact server-bound ceremony. Failed cryptographic proof consumes the challenge and commits its bounded failure counter without partial factor/grant changes. |
| `DELETE /api/v1/household-management/grant` | Explicitly clears the current session's management grant and pending management ceremonies. Returns 204. |

Successful password/passkey proof returns `expires_at`. Bearer callers must replace their ordinary session token with the returned `session_token`. Cookie callers receive a replacement HttpOnly/SameSite=Strict cookie, Secure in exposed mode; no body token is returned to them. Absolute viewing-session expiry is retained. Requests already authenticated under an older credential generation are refused at the protected transaction boundary. The qualified shared consumer is documented in [Household management client](../design/HOUSEHOLD_MANAGEMENT_CLIENT.md); it handles retained cookie/bearer rotation and explicit uncertain-outcome recovery.

All management responses, including outer global-rate-limit errors, are no-store. Routed failures use fixed RFC 9457 problem details: `OWNER_001` 403 fresh proof required; `OWNER_002` 401 invalid/expired proof; `OWNER_003` 429 temporary lockout/capacity; `OWNER_004` 409 last-factor safeguard; `OWNER_005` 400 invalid request; `OWNER_006` 503 unavailable service/storage. Authentication, body-size and media-type errors retain their existing safe statuses. No submitted credential/parser error is reflected.

[Migration 20261024190000](../../server/migrations/20261024190000_household_owner_security.sql) adds `household_owner_state`, one current hashed handoff per account, one grant per session, bounded management challenges, default-false passkey owner eligibility and a session credential generation. Current account status, session expiry, selected profile, credential generation, authority revision and grant expiry are rechecked under consistent transaction locks. Profile changes clear authority even on switch-away-and-back. Password/status/deletion changes invalidate grants/challenges; changing the password also clears password eligibility. Successful factor changes revoke grants. The last-factor guard excludes malformed/oversized stored alternatives.

Two shared proof slots bound cryptographic execution; a canceled password caller cannot release a blocking verifier's slot before that verifier finishes. New management challenges expire in two minutes with eight per-account / 4,096 global slots; each stored state is limited to 64 KiB. Owner-eligible passkeys are capped at 64, checked at protected registration start and finish. Recovery can still register a replacement when more than 64 old viewing keys exist: its exclusion list is bounded and database credential uniqueness remains enforced. Existing registration additionally checks its issuing session/generation and authority revision at finish, preventing a ceremony begun before protection from bypassing activation. Existing ordinary-login discovery remains separate.

## Operator use and activation

After the upgraded server has applied migrations, run `duskcue owner-handoff --user <account UUID>` with the established database/bootstrap configuration. The command connects directly to that configured database; it starts no HTTP listener and does not run migrations or write an application log. Treat its one-time output as an enrollment/recovery secret. Supply it only in the documented authenticated request body over the deployment's protected channel, never in a URL or diagnostic output.

An unfinished initial handoff leaves protection disabled. Successful handoff plus fresh password/existing-passkey proof, or genuine new-passkey registration, activates it atomically. Unprotected households keep their existing behavior, including ordinary credential registration. Protected households require fresh owner grants for profile create/update/delete and passkey registration/removal. This includes the existing profile-name endpoint because it shares the protected update contract. Activation remains explicit in the [household management screen](../design/HOUSEHOLD_MANAGEMENT_CLIENT.md), using the same operator handoff and fresh proof.

For recovery, issuing a new handoff immediately clears trusted-factor designations and management authority while leaving protection enabled. The operator delivers the replacement handoff to the designated owner. That owner may register a replacement passkey through the bounded ceremony; the handoff alone never becomes a viewing session or ongoing login credential. Existing viewing credentials remain separate and are not automatically promoted again. Security events record fixed event names and actor/target attribution in the existing audit log; independent notification delivery remains unqualified.

## Random open PR applicability

GitHub MCP search on October 24 returned one open PR. Random selection from that set chose [PR #5](https://github.com/cloudbyday90/Duskcue/pull/5), immutable head `c018d77b7f09543e5960a571dc43e2f061cedf0a`; it remains unmerged. It requests PostCSS 8.5.23 and Nano ID 3.3.16. Both current client lockfiles contain PostCSS 8.5.30 and Nano ID 3.3.20. The requested patch is superseded, so no local downgrade or PR merge applies.

## Verification and next item

The final current-source production-route contract passes one test with zero ignored cases in **130.47 seconds**, resource `8da655d3-a8bb-4315-b7b7-45f13336ee86`. It uses immutable local PostgreSQL image `sha256:74935e72241653ca55e0414067e6d8763aceb8a810eb51b452253ec3dcfc4336`. Genuine none-attestation registration and signed P-256 assertions exercise the installed verifier; negative cases include wrong account handle, origin, RP, challenge, signature, missing user verification and unsupported iframe context. An omitted optional handle succeeds. Password/handoff expiry and replay, both transports, stable internal session bindings, stale in-flight generation refusal, profile/credential writes, legacy preferences/Kids exit PIN, 64-factor admission, recovery with 65 old viewing keys, challenge bounds and the actual operator command pass. A PostgreSQL-observed blocked mutation proves the concurrent revocation ordering. Fifty-seven fixed, attributed audit events and secret-free captured diagnostics pass. The owned database is removed. Receipt: `.cache/testing-memory/2026-10-25T00-08-04-691Z-1d7b790d-e796-43c9-a0e4-b2187ef79d34.jsonl`. The wrapper finishes zero without a memory stop, with minimum sampled host available memory 15,706,660,864 bytes; no resource reserve, cap or deadline changes.

| Check | Outcome and receipt |
|---|---|
| `node scripts/testing-memory/run.mjs cargo-test --features playback-qualification --lib -- --quiet` | **975 passed, seven existing ignores**, 48.64 seconds; `.cache/testing-memory/2026-10-25T00-12-06-788Z-80400154-833d-4102-81a4-4f4551d966bd.jsonl`. Default-feature integration and feature-enabled library compilation both succeed on the final source. |
| `cargo fmt --all -- --check`; `node scripts/check-copyright.mjs` | Pass; copyright covers 729 tracked source files. |
| Existing profile-selection and parent-unlock integration scripts | Both pass against the refactored service ownership. |
| Changed-document local links and Git whitespace | 691 local links across eleven documents pass; staged/unstaged whitespace checks pass. |
| `node scripts/testing-memory/run.mjs cargo-iptv-clippy` | **Strict check fails on the existing baseline:** 28 library diagnostics and 32 library-test diagnostics including duplicates. Receipt: `.cache/testing-memory/2026-10-25T00-00-45-604Z-4c015102-a9f9-4d9d-a1f5-694d2b1a9e95.jsonl`. Existing sites include downloads/playback/TV argument counts, metadata attributes, chunk iteration and the preexisting auth test-module ordering. No lint suppression/configuration change. |
| Same all-target Clippy invocation with `-W warnings` | Completes and reports no introduced owner-security warnings; additionally exposes two existing Tonight test casts. Receipt: `.cache/testing-memory/2026-10-24T23-52-15-529Z-fee10601-2550-42e2-83ac-45b82e4bd121.jsonl`. The existing `proc-macro-error2` future-compatibility notice remains. This diagnostic run does not turn the strict failure into a pass. |

The initial contract passed its password section, then failed on an error response lacking no-store. Its receipt is `.cache/testing-memory/2026-10-24T23-00-28-154Z-8b43157d-322d-4bc0-bd89-7f6caa0c6757.jsonl`. The response policy now wraps the global guard, and the fixture paces calls at 650 ms under unchanged global limits. The final contract explicitly depletes the global bucket and verifies no-store on the outer 429; native method refusals retain their correct media type. The first corrected run passed in 101.89 seconds at `.cache/testing-memory/2026-10-24T23-08-10-524Z-7f0f624d-22fd-411d-b0e0-5f91e99e897d.jsonl`; expanded runs culminate in the final result above. Internal session-row replacement was also corrected to token/generation rotation so existing download bindings remain intact. Introduced redundant conversion/export placement Clippy findings were corrected before the final check. Failed attempts remain evidence; no assertion or limiter threshold is relaxed.

The subsequent [encrypted assignment/delegation increment](../design/IPTV_ASSIGNMENT.md), source `898b9c2e5be7689e9acb78e2001abdb1b5b13127`, now uses this owner boundary and authority revision. Its shared multi-account lock/revocation helpers preserve the owner contract: the regression passes again in 131.51 seconds with 57 safe events and owned cleanup. The assignment authority owns that receipt and the next saved-destination/M3U/XMLTV sequence. The subsequent [household management client](../design/HOUSEHOLD_MANAGEMENT_CLIENT.md) records real browser/server passkey and isolated native password/keyring qualification. Physical-authenticator/assistive-technology observation, independent notifications, general passkey-login repair and production Live TV remain separate. Research/PR applicability above remains current to October 24; no release or PR merge.

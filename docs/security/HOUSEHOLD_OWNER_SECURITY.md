# Household-owner management security

## Status and authority

Research and design review started October 24, 2026. This is a review draft; no owner-management implementation or runtime qualification is claimed. The current review has two settled decisions and three pending choices below. [IPTV](../design/IPTV.md) owns household/Dispatcharr product behavior; this document owns the proposed shared owner-proof and credential-lifecycle boundary. [Authentication](../design/AUTH.md) owns ordinary account login. The existing Kids exit PIN remains a separate mechanism.

The repository [grill-me entry point](../../.agents/skills/grill-me/SKILL.md) loads [grilling](../../.agents/skills/grilling/SKILL.md). Its shared-understanding confirmation is required before implementing the interviewed plan. Continue on `main`, without a release or PR merge.

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

## Design tree and decisions

Confirmed in this review:

1. Successful proof creates a five-minute server-side grant for the exact signed-in session. No automatic extension. Logout, profile switching, credential changes and delegation changes invalidate it.
2. Losing all trusted owner factors leaves management locked until an explicit server-operator recovery handoff. Viewing-device, invite and device-link codes do not count as owner proof. Recovery revokes prior management grants.

Pending questions already presented to the user:

1. Use an operator-issued, account-bound, single-use ten-minute handoff plus fresh proof to designate the initial owner factor; previously enrolled passkeys receive no automatic owner trust.
2. Once owner protection is enabled, gate related existing profile/restriction/PIN and passkey mutations through the new owner boundary.
3. Complete the backend owner-security foundation in this increment, followed by encrypted Dispatcharr assignment/delegation in the next increment, or deliver them together.

The next review round must settle the dependent enrollment/recovery and activation contract from those answers, then confirm the complete scope. No pending answer is treated as approval.

## Proposed service boundaries and acceptance

Retain Rust/Axum, PostgreSQL, ring and the installed WebAuthn library. Prefer a focused household-management domain and separate proof, trusted-factor, grant, operator-handoff and transaction helpers over adding to the existing large authentication service. Reuse existing authentication and capabilities for identity; apply fresh grant checks inside the transaction that performs each protected mutation.

The candidate design binds challenges to account, session, purpose and authority revision, limits their lifetime/count, consumes them once and applies account-scoped proof throttling even in local mode. A successful privilege elevation rotates the viewing-session identity and returns the correct cookie/bearer replacement. Management state is server-side; the client does not receive a reusable owner credential. Exact schema, endpoint/error and activation contracts remain review work.

Required qualification includes genuine PBKDF verification and signed WebAuthn registration/assertion through the installed library; incorrect challenge/origin/RP/user-verification/signature refusals; cross-account/session/purpose and replay refusals; expiry and profile-switch invalidation; credential enrollment/deletion and last-factor safeguards; operator-handoff single consumption/recovery; direct API and concurrent revocation/mutation cases; and secret-free responses/diagnostics/audit events. Use disposable PostgreSQL and the existing serial guarded testing policy. Existing ordinary viewing behavior must retain its own regression checks.

A real software-authenticator fixture can use existing ring P-256 signing, base64 and serde dependencies. It exercises cryptographic validation without claiming actual browser, hardware, Tauri or UI accessibility qualification. No new cryptographic implementation or dependency is justified by the research.

## Random open PR applicability

GitHub MCP search on October 24 returned one open PR. Random selection from that set chose [PR #5](https://github.com/cloudbyday90/Duskcue/pull/5), immutable head `c018d77b7f09543e5960a571dc43e2f061cedf0a`; it remains unmerged. It requests PostCSS 8.5.23 and Nano ID 3.3.16. Both current client lockfiles contain PostCSS 8.5.30 and Nano ID 3.3.20. The requested patch is superseded, so no local downgrade or PR merge applies.

## Verification and next item

This checkpoint contains research documentation only. No source implementation or runtime tests are claimed. The research draft is published on `main` without marking milestone 3 complete. After the remaining decisions and shared-understanding confirmation, finalize this authority, implement the approved scope, run appropriate guarded qualification, update build/project/domain checkpoints, and commit/push all changes on `main` without a release.

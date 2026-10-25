# Encrypted IPTV household assignment and delegation

## Status and authority

Research started October 24, 2026 against `10b21c37a999fefca40fe11d97b79f9c1af623db` on `main`. This is the proposed next backend increment, following the qualified [household-owner security foundation](../security/HOUSEHOLD_OWNER_SECURITY.md). Product decisions and shared-understanding confirmation are pending. No assignment schema, endpoint or encryption extension is implemented by this research checkpoint. No release or PR merge.

[IPTV.md](IPTV.md) owns the player contract and milestone sequence. This document owns saved assignment, secret storage, administrator delegation and the proposed first profile-policy slice. [Household-owner security](../security/HOUSEHOLD_OWNER_SECURITY.md) continues to own factor trust, enrollment/recovery and five-minute grants. Existing settled decisions are carried forward rather than reopened.

## Existing constraints and source findings

- A `users` account is the household; its viewing profiles belong through `user_profiles.owner_user_id`. No second household identity model is needed.
- `live_tv` currently exposes only syntax validation of an administrator-supplied playlist/guide URL pair. It has no saved assignment, delegation, channel catalog or guide schema. URLs may have separate origins, opaque reverse-proxy paths and query credentials. Existing validation rejects URL userinfo, fragments, raw whitespace, controls and backslashes, and caps each URL at 4,096 UTF-8 bytes.
- The owner foundation already provides strict session/profile/generation checks and current-grant checks inside a transaction. Its legacy `authorize` helper intentionally allows existing unprotected households; new owner IPTV management must instead require an explicitly activated owner grant.
- `can_manage_users` is the existing administrator configuration permission. Its extractor checks a snapshot before service execution. Sensitive saved writes must also recheck the actor's current authority in the write transaction. Global server role `owner` does not substitute for household owner proof.
- Profile schemas currently have no Live TV fields, durable channel identifiers or programme-rating evidence. Shared household stream reservations are also not implemented. Persisting a cap cannot be described as enforcing concurrent playback limits.
- Existing `EncryptionKey` uses ring 0.17.14 AES-256-GCM with a 32-byte key, random twelve-byte nonce and sixteen-byte tag. It uses empty associated data. `decrypt_if_encrypted` accepts plaintext and converts decryption errors into empty strings; that compatibility behavior is unsuitable for new assignment secrets.
- Storage-key initialization already loads or persists a bootstrap key and shares it through `AppState`. There is no storage-key ring, key identifier or rewrapping workflow. Existing stream-signing key rotation is a different mechanism.
- Generic audit row snapshots and partial secret masking are unsuitable for feed URLs. Use explicit fixed-event audit data and presence/transport metadata, with no URL or ciphertext copied into audit records.

Source boundaries inspected: `server/src/domains/live_tv/`; `server/src/domains/household_security/`; `server/src/domains/auth/service.rs`; `server/src/extractors.rs`; `server/src/services/encryption.rs`; `server/src/domains/profiles/`; and the existing guarded production-route qualification helpers.

## Official-source research

URLs were discovered through MCP search and opened October 24, 2026. These findings describe retrieved official material, with application-specific recommendations identified separately. No inferred URL, Dispatcharr endpoint generation or upstream runtime qualification is used.

| Official source | Finding and recommended use | Cost or alternative |
|---|---|---|
| [Dispatcharr Channel Profiles](https://dispatcharr.github.io/Dispatcharr-Docs/channels/) | Consume copied profile M3U/EPG links. Preserve opaque paths and query strings. Export options include bypassing the proxy, so a saved feed origin cannot authorize every nested resource. | An admin API adapter would couple Duskcue to upstream management and duplicate responsibility; it is unnecessary for this player increment. |
| [Dispatcharr System](https://dispatcharr.github.io/Dispatcharr-Docs/system/), [export authentication guidance](https://dispatcharr.github.io/Dispatcharr-Docs/troubleshooting/) | XC user/profile restrictions and password-protected export URLs are distinct from raw profile endpoints. Treat each complete supplied URL as secret. A profile identifier or label does not establish upstream authentication. | Saving syntax-valid links does not prove authentication, membership, proxy mode or reachability. Those need actual qualification later. |
| [Dispatcharr v0.33.0 release](https://github.com/Dispatcharr/Dispatcharr/releases/tag/v0.33.0) | The retrieved October 24 release changes XC streaming network checks to `XC_API`, matching `get.php`/`xmltv.php`. Record this version-specific distinction from older general documentation. | New native HLS support does not expand Duskcue's agreed initial proxied MPEG-TS scope. |
| [OWASP cryptographic storage](https://cheatsheetseries.owasp.org/cheatsheets/Cryptographic_Storage_Cheat_Sheet.html), [ring AEAD](https://docs.rs/ring/latest/ring/aead/struct.LessSafeKey.html), [ring associated data](https://docs.rs/ring/latest/ring/aead/struct.Aad.html) | Retain authenticated AES-GCM and the installed key service. Add a strict envelope and bind household, assignment and typed feed field as associated data. Generate a fresh nonce on every write. | Context binding prevents substitution; it does not independently prevent restoring an old complete database snapshot. Losing the storage key prevents decryption. |
| [OWASP authorization](https://cheatsheetseries.owasp.org/cheatsheets/Authorization_Cheat_Sheet.html) | Deny ungranted actions and recheck account, session, capability, household ownership and delegation at the protected mutation. | Requires coordinated transaction locks and revocation tests, including the existing capability-update path. |
| [OWASP secrets management](https://cheatsheetseries.owasp.org/cheatsheets/Secrets_Management_Cheat_Sheet.html) | Keep raw URLs write-only, minimize secret copies and use fixed audit events without secret material. Preserve the established bootstrap-key ownership. | A dedicated external secret manager adds deployment complexity and is not proposed here. Storage-key rotation remains separate work. |
| [OWASP SSRF prevention](https://cheatsheetseries.owasp.org/cheatsheets/Server_Side_Request_Forgery_Prevention_Cheat_Sheet.html) | Saving configuration must not grant connection authority. Future networking needs explicit destinations, connection-time resolution checks and redirect/proxy control for feeds and nested resources. | Storage alone cannot qualify hostname/TLS behavior, embedded stream URLs or artwork destinations. |
| [HTTP Semantics, RFC 9110](https://www.rfc-editor.org/rfc/rfc9110.html) | Use conditional mutation with strong representation validators to prevent lost updates. Recheck the expected assignment revision under the transaction lock. | Clients must handle stale-version refusal and reread current metadata rather than silently overwriting another change. |
| [W3C WCAG 2.2](https://www.w3.org/TR/WCAG22/) | Later forms need labels, accessible authentication, keyboard/focus behavior, textual errors and announced results. Keep backend refusal codes stable enough for those clients. | Backend HTTP tests do not establish UI accessibility conformance. The interactive management screen is a separate increment. |

## Recommended stack

Retain Rust/Axum, PostgreSQL, installed `url`, ring AES-256-GCM, `AppState.encryption_key` and the qualified owner-security services. Extend encryption in a focused child module; keep assignment HTTP translation, authority checks, secret storage, delegation policy and profile policy in separate modules under the existing Live TV domain. Preserve the five-file domain pattern and existing validator ownership. No new dependency, singleton, Dispatcharr management adapter or authentication framework is recommended.

This stack reuses qualified components and keeps the trust boundary reviewable. Its costs are additional transaction checks, explicit secret-replacement handling and a consumer that already understands owner-grant/session rotation. The first increment should remain backend-only and perform no upstream DNS or HTTP.

## Settled product boundaries

The administrator configures a household's Dispatcharr connection and maximum allowance. The designated household owner can only narrow access within delegated controls. Viewing profiles never expand household authority. Source configuration is not owned by a viewing profile. The existing owner-proof grant is five minutes, session-bound and not renewed by management requests; delegation changes invalidate outstanding grants.

Keep the supplied playlist/guide URL pair opaque. New request types accept validated raw URLs only, never caller-provided encrypted envelopes or masked-placeholder values. Responses expose configured state, identifiers, revisions, policy and transport metadata; no raw, partially masked or decrypted feed URL is returned. Provide no HTTP secret-export/decrypt endpoint.

Saving an assignment does not enable production Live TV, prove Dispatcharr credentials or authorize any destination. Channel import, XMLTV/rating policy, common stream admission, production lifecycle and client journeys remain separate milestones. Existing ordinary capabilities remain part of future effective playback authority; an assignment does not silently grant new global capabilities.

## Open design frontier

The following independent decisions were presented to the user. Recommendations below are proposed, not confirmed behavior:

1. **Sensitive administrator proof:** require a current grant on the administrator's own account in addition to `can_manage_users` for source writes and delegation changes. Benefit: an ordinary admin viewing session cannot retarget feeds or alter delegation. Cost: the admin account must explicitly enroll an owner factor and unlock before these operations.
2. **Initial delegation:** offer independent, default-off permissions to manage standard-profile Live TV access and set a stricter household Live TV cap. Keep Kids/channel exceptions for the channel/guide milestone. Benefit: useful bounded owner controls without pretending missing guide evidence exists. Cost: saved caps still await shared runtime admission enforcement. The alternative presented is profile-access delegation only.
3. **Rotation and replacement:** preserve assignment identity/profile policy on explicit credential rotation; create a new identity and reset owner overrides/profile access on explicit replacement. Both revoke management grants. Benefit: routine secret rotation preserves identity while replacing a lineup requires deliberate access approval. Cost: administrators must distinguish the two actions. The alternative presented resets profile access on every credential change.

After those answers, finalize any dependent policy details, exact interfaces/schema and acceptance scope, then obtain the repository-required shared-understanding confirmation before source implementation. Existing legal-header authorization carries forward; no explanatory source comments are proposed.

## Proposed implementation sequence

### 1. Strict contextual secret storage

Add a small encryption child module that can use the existing private key without exporting raw key bytes. Use a bounded, versioned envelope with one recognized algorithm/version. Associated data must encode a fixed namespace, household UUID, assignment UUID, typed field and envelope version unambiguously. Keep envelope version separate from mutable policy revision, avoiding needless secret re-encryption on profile/cap changes.

Opening must reject plaintext, legacy `encrypted:` values, unknown versions, malformed/truncated payloads, wrong keys/context, tag failures and invalid UTF-8. No empty-string or plaintext fallback. Preserve exact validated URL bytes and generate a fresh nonce per seal. Retain compatibility behavior for existing unrelated callers rather than changing their stored data implicitly.

### 2. Assignment persistence and administrator authority

Propose one current assignment per household, with its own UUID/revision, encrypted playlist/guide fields, enabled allowance, optional Live TV cap and explicit delegation flags. Keep policy defaults restrictive. Deletion removes the saved secrets and dependent profile policy. An initial assignment can be configured before the target household activates owner protection; new owner management still requires activation and fresh proof.

Check current actor/target account state and actor session/profile/generation in the write transaction. Lock all involved account rows in deterministic UUID order. Re-resolve `can_manage_users` under that lock. Make existing capability replacement participate in the account-row locking protocol so a concurrent capability revocation cannot race the new write. Reuse a deliberate owner-service invalidation API for target-household grants/challenges rather than copying revocation SQL.

Use conditional creation/update/delete against the current representation and locked revision. Credentials and mutable policy should have separate write DTOs so an ordinary policy update never round-trips secret placeholders. Exact paths, DTOs, validator semantics and status mapping will be recorded after the design decisions are confirmed.

### 3. Owner delegation and profile policy

Use strict owner scope plus current-grant verification for every new owner mutation. An unprotected household must activate owner protection before these new operations; the legacy compatibility helper must not bypass that requirement. Check assignment, target profile ownership, administrator allowance and the exact delegated operation in the same transaction. Default new profile policy to disabled. Keep Kids blocked until channel/rating evidence and exceptions can be represented and qualified.

Optional Live TV caps add a ceiling within the existing shared household cap; they never replace it. Null means no additional ceiling at that layer, not unlimited playback. Positive bounded cap values and explicit enabled flags avoid overloading zero with conflicting meanings. Effective limits and profile policy must be computed from the current administrator allowance and narrower owner choices. Revoking management delegation must not silently broaden existing restrictions.

Use assignment-policy revisions for lost-update checks separately from owner-authority revisions. Ordinary owner policy writes must not extend a grant. Administrator delegation/source changes revoke affected management authority. Production playback enforcement, reservations and active-stream revocation are not claimed by saved-policy work.

### 4. Qualification and outcome documentation

Add meaningful unit cases for round trip, fresh ciphertext, every context mismatch, cross-household/assignment/field swaps, altered nonce/tag/payload/version, wrong key, legacy/plaintext refusal and byte bounds. Verify that associated-data encoding is unambiguous and that secret request types remain non-debuggable/non-serializable.

Use the real production router and guarded disposable PostgreSQL pattern for cookie/bearer access, cross-household refusal, fresh/stale grants, capability/delegation revocation, source rotation/replacement, profile ownership/Kids refusal, cap narrowing, stale revisions and transaction rollback. Inspect actual stored ciphertext and capture responses/audit/logs to prove no submitted URL, token or handoff disclosure. Observe a real blocked mutation/revocation race through PostgreSQL rather than using timing-only sleeps. Include invalid/oversized/media-type requests and no-store on outer error paths.

Run compilation, focused runtime, existing owner-security regression, relevant profile checks and library/lint verification serially through the established [resource workflow](../ci/TESTING_MEMORY.md). Preserve all limits and record strict Clippy baseline failures honestly. Do not count local unit/HTTP checks as hosted CI, browser, hardware-passkey, Dispatcharr or production playback qualification.

After implementation, update this authority with exact source/receipts/outcomes, cross-reference [database](DATABASE.md), [API security](../security/API_SECURITY.md), [project structure](PROJECT_STRUCTURE.md), [BUILD_ORDER.md](../../BUILD_ORDER.md) and [PROJECT.md](../../PROJECT.md), then commit/push all changes on `main` without a release or PR merge.

## Random open PR applicability

GitHub MCP search on October 24 returned one open PR; random selection from that candidate set chose [PR #5](https://github.com/cloudbyday90/Duskcue/pull/5). The fetched immutable head is `c018d77b7f09543e5960a571dc43e2f061cedf0a`, with no merge timestamp. It requests PostCSS 8.5.23 and Nano ID 3.3.16. Both current client lockfiles contain PostCSS 8.5.30 and Nano ID 3.3.20, verified directly during this research. The requested patch is superseded, so no local downgrade, patch, PR comment, closure or merge applies.

## Outcome and next item

This checkpoint contains official-source research, source investigation, recommendations/tradeoffs and a proposed implementation/qualification sequence. Product answers and final shared-understanding confirmation remain pending. No source implementation, migration or runtime test success is claimed yet.

Local verification passes 580 repository links across the four changed Markdown documents and Git whitespace checks. Source/schema are unchanged, so no application build or runtime test is needed for this documentation checkpoint. Those remain required for the subsequent implementation.

The immediate next item is to settle the three design choices and confirm the concrete assignment contract, then implement and qualify it. After that increment, evaluate saved destination authority and bounded M3U/XMLTV ingestion before connecting production feed networking or playback. The client owner unlock/management UI remains separately qualified work.

# Encrypted IPTV household assignment and delegation

## Status and authority

**Qualified source:** `898b9c2e5be7689e9acb78e2001abdb1b5b13127` on `main`. Later documentation-only checkpoints preserve the verified code. No release or PR merge.

Research started October 24, 2026 against `10b21c37a999fefca40fe11d97b79f9c1af623db` on `main`; research checkpoint `85122e0` records the source investigation. This is the qualified saved-configuration backend increment following the [household-owner security foundation](../security/HOUSEHOLD_OWNER_SECURITY.md). The user confirmed all three product choices below. The user confirmed shared understanding of the complete concrete contract. The backend assignment/delegation increment is implemented and locally qualified. No release or PR merge.

[IPTV.md](IPTV.md) owns the player contract and milestone sequence. This document owns saved assignment, secret storage, administrator delegation and the first profile-policy slice. [Household-owner security](../security/HOUSEHOLD_OWNER_SECURITY.md) continues to own factor trust, enrollment/recovery and five-minute grants. Existing settled decisions are carried forward rather than reopened.

## Pre-increment constraints and source findings

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
| [OWASP cryptographic storage](https://cheatsheetseries.owasp.org/cheatsheets/Cryptographic_Storage_Cheat_Sheet.html), [ring AEAD](https://docs.rs/ring/latest/ring/aead/struct.LessSafeKey.html), [ring associated data](https://docs.rs/ring/latest/ring/aead/struct.Aad.html) | Retain authenticated AES-GCM and the installed key service. Add a strict envelope and bind household, assignment and typed feed field as associated data. Generate a fresh nonce on every write. | Context binding prevents substitution; it does not prevent replay of older valid ciphertext in the same context or restoring a database snapshot. Losing the storage key prevents decryption. |
| [OWASP authorization](https://cheatsheetseries.owasp.org/cheatsheets/Authorization_Cheat_Sheet.html) | Deny ungranted actions and recheck account, session, capability, household ownership and delegation at the protected mutation. | Requires coordinated transaction locks and revocation tests, including the existing capability-update path. |
| [OWASP secrets management](https://cheatsheetseries.owasp.org/cheatsheets/Secrets_Management_Cheat_Sheet.html) | Keep raw URLs write-only, minimize secret copies and use fixed audit events without secret material. Preserve the established bootstrap-key ownership. | A dedicated external secret manager adds deployment complexity and is not proposed here. Storage-key rotation remains separate work. |
| [OWASP SSRF prevention](https://cheatsheetseries.owasp.org/cheatsheets/Server_Side_Request_Forgery_Prevention_Cheat_Sheet.html) | Saving configuration must not grant connection authority. Future networking needs explicit destinations, connection-time resolution checks and redirect/proxy control for feeds and nested resources. | Storage alone cannot qualify hostname/TLS behavior, embedded stream URLs or artwork destinations. |
| [HTTP Semantics, RFC 9110](https://www.rfc-editor.org/rfc/rfc9110.html) | Use conditional mutation with strong representation validators to prevent lost updates. Recheck the expected assignment revision under the transaction lock. | Clients must handle stale-version refusal and reread current metadata rather than silently overwriting another change. |
| [Additional HTTP Status Codes, RFC 6585](https://www.rfc-editor.org/rfc/rfc6585.html), [WHATWG Fetch](https://fetch.spec.whatwg.org/) | Return 428 when the required version condition is absent and expose `ETag` to allowed browser clients. The retrieved Fetch standard is dated October 19, 2026. | Every 428 must remain no-store. Header exposure does not expand the allowed-origin or credential policy. |
| [W3C WCAG 2.2](https://www.w3.org/TR/WCAG22/) | Later forms need labels, accessible authentication, keyboard/focus behavior, textual errors and announced results. Keep backend refusal codes stable enough for those clients. | Backend HTTP tests do not establish UI accessibility conformance. The interactive management screen is a separate increment. |

## Recommended stack

Retain Rust/Axum, PostgreSQL, installed `url`, ring AES-256-GCM, `AppState.encryption_key` and the qualified owner-security services. Extend encryption in a focused child module; keep assignment HTTP translation, authority checks, secret storage, delegation policy and profile policy in separate modules under the existing Live TV domain. Preserve the five-file domain pattern and existing validator ownership. No new dependency, singleton, Dispatcharr management adapter or authentication framework is recommended.

This stack reuses qualified components and keeps the trust boundary reviewable. Its costs are additional transaction checks, explicit secret-replacement handling and a consumer that already understands owner-grant/session rotation. The first increment should remain backend-only and perform no upstream DNS or HTTP.

## Settled product boundaries

The administrator configures a household's Dispatcharr connection and maximum allowance. The designated household owner can only narrow access within delegated controls. Viewing profiles never expand household authority. Source configuration is not owned by a viewing profile. The existing owner-proof grant is five minutes, session-bound and not renewed by management requests; delegation changes invalidate outstanding grants.

Keep the supplied playlist/guide URL pair opaque. New request types accept validated raw URLs only, never caller-provided encrypted envelopes or masked-placeholder values. Responses expose configured state, identifiers, revisions, policy and transport metadata; no raw, partially masked or decrypted feed URL is returned. Provide no HTTP secret-export/decrypt endpoint.

Saving an assignment does not enable production Live TV, prove Dispatcharr credentials or authorize any destination. Channel import, XMLTV/rating policy, common stream admission, production lifecycle and client journeys remain separate milestones. Existing ordinary capabilities remain part of future effective playback authority; an assignment does not silently grant new global capabilities.

## Confirmed product choices

The user accepted each recommended choice on October 24:

1. **Sensitive administrator proof:** require a current grant on the administrator's own account in addition to `can_manage_users` for source writes and delegation changes. Benefit: an ordinary admin viewing session cannot retarget feeds or alter delegation. Cost: the admin account must explicitly enroll an owner factor and unlock before these operations.
2. **Initial delegation:** offer independent, default-off permissions to manage standard-profile Live TV access and set a stricter household Live TV cap. Keep Kids/channel exceptions for the channel/guide milestone. Benefit: useful bounded owner controls without pretending missing guide evidence exists. Cost: saved caps still await shared runtime admission enforcement.
3. **Rotation and replacement:** preserve assignment identity/profile policy on explicit credential rotation; create a new identity and reset owner overrides/profile access on explicit replacement. Both revoke management grants. Benefit: routine secret rotation preserves identity while replacing a lineup requires deliberate access approval. Cost: administrators must distinguish the two actions.

These answers close the product frontier. The concrete contract below resolves the implementation details within those choices; the user has now confirmed shared understanding of its complete scope. Existing legal-header authorization carries forward; no explanatory source comments are proposed.

## Confirmed concrete backend contract

### Storage and secret boundaries

Add `live_tv_assignments` with a UUIDv7 primary key, unique household account, positive revision, encrypted playlist/guide fields, safe transport metadata, `enabled` default false, nullable `max_live_streams`, `allow_owner_profile_access` default false, `allow_owner_live_limit` default false and nullable `owner_max_live_streams`. Administrative configuration can exist before the target household activates owner protection. Administrator authority is proven on the administrator's account, independently of the target household.

Add `live_tv_profile_access`, keyed by assignment/profile, storing the household account and an enabled flag. Enforce assignment-household and profile-household equality with composite foreign keys; the required `user_profiles(owner_user_id,id)` uniqueness already exists. Missing policy means disabled. Deleting/replacing an assignment cascades its profile policy; deleting a profile cascades its setting. No channel or programme schema is introduced.

The strict encryption envelope is `duskcue-secret:v1:` plus standard Base64 of the random twelve-byte nonce, AES-256-GCM ciphertext and sixteen-byte tag. A URL is at most 4,096 UTF-8 bytes; the envelope is at most 5,518 bytes. Bound input before allocating decoded storage. Use unambiguous associated data containing a fixed assignment namespace, household UUID, assignment UUID, typed playlist/guide field and envelope version. The mutable policy revision is separate. Unknown versions, legacy envelopes, plaintext, malformed/oversized data, wrong context/key and failed authentication are errors. Keep existing compatibility helpers for unrelated consumers. No secret-export endpoint or upstream connection permit is added.

### HTTP interfaces

Use one administrator resource, `/api/v1/live-tv/households/{household_id}/assignment`, and one owner resource, `/api/v1/household-management/live-tv`. Both use bounded, strict DTOs and safe response types. Secret-bearing request types have no `Debug` or `Serialize`. Every mutation body is at most 16 KiB, and each raw URL uses the established validator without rebuilding its path/query.

| Method/resource | Contract |
|---|---|
| `GET` administrator resource | Current `can_manage_users`, selected standard profile and current session/generation; returns redacted metadata and a strong `ETag`. This read does not require a management grant and returns no secret. Missing account/assignment returns 404. |
| `PUT` administrator resource | Create only with `If-None-Match: *` and fresh administrator-account proof. Body: required `playlist_url`/`guide_url`; optional `enabled`, `max_live_streams`, `allow_owner_profile_access`, `allow_owner_live_limit`, with restrictive defaults above. Return 201 and redacted metadata/ETag. |
| `PATCH` administrator resource | Fresh administrator-account proof plus the current strong `If-Match`. Tagged `operation`: `configure` supplies all four administrative policy fields; `rotate_credentials` supplies both URLs; `replace_assignment` supplies both URLs. Return 200 and current metadata/ETag. |
| `DELETE` administrator resource | Fresh administrator-account proof plus current strong `If-Match`; remove assignment/secrets/profile policy, revoke target grants/challenges and return 204. |
| `GET` owner resource | Strict current owner grant on this session/account; returns redacted assignment/delegation/limit metadata and its own strong `ETag`. Missing assignment returns 404. |
| `GET` owner resource `/profiles` | Same strict owner authority. Keyset pagination by profile UUID, default limit 50, maximum 100; return profile identity/type, saved enabled state, eligibility, assignment revision and next cursor. Kids are ineligible and disabled. No unbounded profile array in the assignment resource. |
| `PATCH` owner resource | Strict owner authority plus the owner resource's current strong `If-Match`. Tagged `operation`: `set_profile_access` requires `profile_id` and `enabled`; `set_live_limit` requires `max_live_streams` as a positive integer or explicit null. Missing nullable fields are invalid. Each operation requires its independently delegated permission and an enabled household allowance. Return updated metadata/ETag. |

Redacted metadata includes assignment/household IDs, revision, configured state, feed transports and the relevant administrator/owner policy. `configured` means saved presence, not verified upstream credentials. Expose `network_checked: false` and `playback_available: false` for this configuration-only slice. No full or partially masked URL, hostname, path, query, token or ciphertext enters a response.

Generate strong validators from deterministic redacted representations using the existing SHA-256 ETag primitive; administrator and owner representations have their own validators. Recompute/check them under transaction locks. Creation refuses an existing assignment with 412. Updates/deletion require a version-specific `If-Match`: absence or bare wildcard gets 428, stale/weak nonmatching validators get 412 and malformed conditions get 400. Bound conditional headers at 1,024 bytes and support valid tag lists within that bound. Reuse conditional GET behavior after authentication. Expose `ETag` through CORS without widening allowed origins or credential rules.

All assignment/owner management responses, including outer limiter errors, are no-store. Use fixed RFC 9457 errors without submitted/parser/storage text: `IPTV_001` 403 disallowed operation, `IPTV_002` 404 unavailable assignment/profile, `IPTV_003` 412 stale condition, `IPTV_004` 428 version condition required, `IPTV_005` 400 malformed request, `IPTV_006` 422 invalid policy and `IPTV_007` 503 storage/service failure. Existing authentication/owner errors retain their established codes; URL field validation retains `VALID_001`, with existing 413/415 handling. A cross-household profile ID receives the same refusal as a missing profile. Do not relabel native empty/text errors as JSON problem bodies.

### Policy and lifecycle semantics

Live TV caps are nullable positive 32-bit integers: null adds no ceiling at that layer, and zero/negative values are invalid. The configured additional ceiling is the minimum of present administrator and owner values. Owner values cannot exceed an explicit administrator ceiling. Clearing the owner value removes only that additional restriction; shared household limits remain authoritative when runtime admission is implemented. Administrative narrowing immediately narrows the computed ceiling; later broadening does not erase a saved stricter owner choice.

Owners can change only their own standard-profile choices within an enabled household allowance and the corresponding delegated operation. New profiles default disabled. Kids mutations are refused until the later channel/rating milestone. Withdrawing delegation blocks further owner mutations and retains existing restrictive choices; it does not silently broaden access. Ordinary owner policy writes increment assignment revision and never extend the management grant.

Credential rotation retains the assignment UUID, administrative policy, owner limit and profile policy, reseals both supplied URLs and increments revision. Replacement creates a new UUID/revision 1, retains administrator allowance/delegation, clears the owner limit and removes all profile-access choices. Administrator create/configure/rotation/replacement/deletion revoke target-household grants/challenges in the same transaction. Same-account administration also revokes the acting grant as a consequence. None of these operations revokes ordinary viewing sessions or changes trusted factors.

### Transactions, modules and acceptance

Keep the five-file `live_tv` domain and add a focused assignment submodule with HTTP/types/error/service boundaries and separate authority, storage, policy and precondition helpers. Add the strict encryption child module and a deliberate shared owner revocation/multi-account transaction interface. Do not duplicate credential verification, factor trust or grant SQL in the assignment service.

Lock unique involved account rows in deterministic UUID order before profile/session/assignment locks. The actor must be active/nondeleted with current session/generation; administrator writes require current capability and fresh proof, including for global role `owner`. Targets must exist and be nondeleted; an inactive target may be configured or unassigned without gaining viewing/owner authority. Owner mutations require the existing strict active-account grant. Make capability replacement lock its subject account row, so request-start permission snapshots cannot race a later revocation. Preserve established lock/statement deadlines. Check target profile ownership/type and the conditional representation before writing, and commit fixed attributed audit events without secrets or row snapshots. Audit data records only the event and target household UUID, with actor/assignment attribution in the existing audit columns; household association survives assignment replacement/deletion.

Acceptance includes actual cookie/bearer proof and production-route PostgreSQL execution; cross-account/profile refusals; capability/delegation revocation observed through database locks; expired grants and stale generations; concurrent/conditional updates and replacement identity; cap narrowing and Kids refusal; profile/FK cascades; inspected ciphertext/context substitution and failure rollback; no secret disclosure through responses/audit/logs; no-store on outer errors; bounded pagination/requests/conditions; and CORS ETag exposure. Repeat the owner-security regression and appropriate library/build/lint checks serially under unchanged resource guards. Record existing strict Clippy findings rather than weakening lint policy.

This increment includes backend configuration/policy only. Client unlock/management UI, physical/browser/Tauri owner ceremonies, independent notifications, storage-key rotation, destination authorization/fetching, M3U/XMLTV imports, runtime stream-cap enforcement and production Live TV remain separate qualification. Commit/push verified source and outcomes on `main`, without a release or PR merge.

## Implementation structure and qualification

### 1. Strict contextual secret storage

Add a small encryption child module that can use the existing private key without exporting raw key bytes. Use a bounded, versioned envelope with one recognized algorithm/version. Associated data must encode a fixed namespace, household UUID, assignment UUID, typed field and envelope version unambiguously. Keep envelope version separate from mutable policy revision, avoiding needless secret re-encryption on profile/cap changes.

Opening must reject plaintext, legacy `encrypted:` values, unknown versions, malformed/truncated payloads, wrong keys/context, tag failures and invalid UTF-8. No empty-string or plaintext fallback. Preserve exact validated URL bytes and generate a fresh nonce per seal. Retain compatibility behavior for existing unrelated callers rather than changing their stored data implicitly.

### 2. Assignment persistence and administrator authority

Persist one current assignment per household, with its own UUID/revision, encrypted playlist/guide fields, enabled allowance, optional Live TV cap and explicit delegation flags. Keep policy defaults restrictive. Deletion removes the saved secrets and dependent profile policy. An initial assignment can be configured before the target household activates owner protection; new owner management still requires activation and fresh proof.

Check current actor/target account state and actor session/profile/generation in the write transaction. Lock all involved account rows in deterministic UUID order. Re-resolve `can_manage_users` under that lock. Make existing capability replacement participate in the account-row locking protocol so a concurrent capability revocation cannot race the new write. Reuse a deliberate owner-service invalidation API for target-household grants/challenges rather than copying revocation SQL.

Use the conditional creation/update/delete contract above against the current representation and locked revision. Credentials and mutable policy use separate tagged write variants so an ordinary policy update never round-trips secret placeholders.

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

## Verification and outcome

The confirmed backend scope is implemented. One current encrypted URL-pair assignment per household, fresh administrator-account proof plus current permission, default-off independent delegations, strict owner/profile checks, conditional writes and rotation/replacement semantics are wired into the real production router. No dependency, branch, release, PR merge or deployment is added. Existing compatibility encryption callers retain their contracts; new assignment secrets use only the strict codec.

The final source contract passes **one actual test with zero ignored cases in 109.81 seconds**, resource `28237785-5e92-49c7-8186-8e52cd5668e5`. Its immutable local PostgreSQL image is `sha256:74935e72241653ca55e0414067e6d8763aceb8a810eb51b452253ec3dcfc4336`. Real PBKDF proof, cookie/bearer behavior, global-owner proof refusal, current permission checks, bounded strong conditions, cross-household/profile/FK refusal, expiry/stale tokens, Kids refusal, narrowed/retained limits, paginated policy, rotation identity, replacement reset, failed-replacement rollback and deletion cascades pass. Actual PostgreSQL blocking observations cover capability-update participation, a pending administrator write across capability revocation and a pending owner mutation across an atomic delegation/grant revocation. The loopback listener observes no source connection during save. Seventeen audit events contain only fixed event/target-household data with actor/assignment attribution. Response/audit/log secret checks and outer limiter no-store pass. The owned database and verified temporary directory are removed.

Final receipt: `.cache/testing-memory/2026-10-25T01-53-44-520Z-e2fe48f4-fac5-4fc7-997d-19f581f6ed33.jsonl`. The wrapper finishes zero without a memory stop, with minimum sampled host available memory **15,930,908,672 bytes**. No resource reserve, cap, deadline, global limiter, assertion or unrelated workload is relaxed. The application pool remains four; the capability-lock case briefly uses one separate coordinator connection under the unchanged PostgreSQL maximum of sixteen.

| Check | Outcome and receipt |
|---|---|
| Existing owner-security production-route regression | **One pass, zero ignored**, 131.51 seconds, 57 safe events; `.cache/testing-memory/2026-10-25T01-39-33-346Z-d8d8f128-f91e-435f-97ab-bf86ae2c6339.jsonl`. Real signed passkey/password enrollment/recovery, operator command, session generations, related mutations, revocation and shared-fixture cleanup retain their contract. |
| `node scripts/testing-memory/run.mjs cargo-test --features playback-qualification --lib -- --quiet` | **977 passed, seven existing ignores**, 55.32 seconds; `.cache/testing-memory/2026-10-25T01-42-50-206Z-120ff9e0-c670-4ff8-b74e-b6c8de48c390.jsonl`. Includes genuine contextual encryption/alteration/UTF-8/bounds cases and strong-condition parsing. |
| Default-feature compilation | Initial guarded `cargo-check` passes; `.cache/testing-memory/2026-10-25T01-23-46-362Z-a780eb4a-a118-45e8-8718-bc91ba913090.jsonl`. The final default-feature production-route executable also compiles and passes. |
| Strict Clippy | Remains blocked by the existing **28 library / 32 library-test diagnostics including duplicates**, with no introduced-file finding; `.cache/testing-memory/2026-10-25T01-45-53-494Z-6bdf0edf-300e-451d-93ab-ee2c90412450.jsonl`. No source allowance or lint-policy change. |
| Final all-target diagnostic with `-W warnings` | Completes with **no introduced assignment/encryption/fixture warning**; existing library findings, two Tonight test casts and `proc-macro-error2` future-compatibility notice remain. `.cache/testing-memory/2026-10-25T01-51-27-591Z-36552ac5-2dd9-4a85-bf94-63bf417a77cc.jsonl`. This is not a strict-lint pass. |
| Format, copyright, profile/PIN contracts and documentation | Format and copyright pass (742 tracked source files); both existing profile-selection/parent-unlock scripts pass. All 749 local links across fourteen changed documents and Git whitespace checks pass before the source commit. |

The first attempted route contract failed after 22.10 seconds because a fixture expected uppercase `VALID_001` in the problem URI; the existing response contract lowercases it to `/errors/valid_001`. Receipt: `.cache/testing-memory/2026-10-25T01-31-35-259Z-fbc44ed2-9300-4cf4-b48b-217a56b836cf.jsonl`. The assertion was corrected to the exact established URI, with no status or input-limit relaxation. The next expanded run passed in 111.60 seconds at `.cache/testing-memory/2026-10-25T01-36-55-013Z-5608e619-159b-4ab8-9319-d0483c143569.jsonl`. Review then removed the unrelated validator-endpoint instance from assignment errors and retained household identity in audit events after source replacement/deletion. The final repeat above qualifies both corrections. An unused fixture-field warning was removed by using the bounded database coordinator; no lint suppression was added.

Research checkpoint `85122e0` and concrete-contract checkpoint `d99682a` preserve the source investigation, official-source alternatives, the three product answers and reviewable concrete contract. The user confirmed shared understanding after that contract checkpoint and before implementation. Their 580/564 link checks were documentation evidence, not application-runtime qualification. The source checkpoint is recorded in [BUILD_ORDER.md](../../BUILD_ORDER.md).

## Next item and qualification boundaries

Next, implement and qualify **[saved feed destination authority](../security/IPTV_DESTINATION_AUTHORITY.md)** after confirmation of its complete concrete contract. October 25 research records the user's confirmed explicit IP pins, public-root TLS verification and default-off qualification. Its approval privacy, generation fencing, same/changed-origin rotation, connection ownership and refusal tests extend this saved-configuration boundary. No source or networking change is claimed at that design checkpoint. Qualify that boundary before bounded M3U/XMLTV ingestion and production feed connections. Saved URLs and successful syntax/encryption checks are not networking permits or proof of Dispatcharr authentication/profile membership.

The client unlock/management UI and its WCAG authentication/focus/status execution, actual browser/hardware/Tauri owner ceremonies, independent notifications, storage-key rotation, general passkey-login repair, guide/current-programme policy, shared runtime stream admission and production Live TV remain separately qualified work. No such capability is inferred from this backend increment.

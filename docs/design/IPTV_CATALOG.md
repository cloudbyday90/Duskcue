# Profile-scoped IPTV catalog reads

## Status and scope

Research began October 25, 2026, against `b89e06a632e44628087c70b2195cf9963980a428` on `main`, following qualified [SSE root ownership](SSE_LIFECYCLE.md), source `7c1c23f`. This document records inspected source, MCP-discovered official guidance, alternatives and implementation prerequisites. The first three design choices are confirmed: selected-standard-profile normal-build reads, allowlisted metadata without upstream references, and snapshot/profile-bound continuation. The second review round is pending; the complete implementation contract has not yet received shared-understanding confirmation. No catalog source changes, release or PR merge are claimed.

[IPTV](IPTV.md) owns the product contract. [Imports](IPTV_IMPORTS.md) owns encrypted last-good snapshots and uncertain guide evidence. [Assignment](IPTV_ASSIGNMENT.md) owns household/profile access and management delegation. [Owner security](../security/HOUSEHOLD_OWNER_SECURITY.md) owns fresh management proof. Browsing must preserve those boundaries without treating an ordinary viewing request as a management operation.

## Findings in the existing code

### Viewing authority

- `AuthenticatedUser` supplies account, session, active profile and credential version after session validation. It is an extraction-time observation, so it cannot alone certify authority throughout a later catalog transaction.
- `Require<CanUseLiveTv>` resolves the existing capability and blocks Kids. It does not itself reject pending profile selection or fence a subsequent profile switch.
- `household_security::transactions::scope` checks the owned standard profile, exact session, selected-profile state, absolute expiry and credential version under account/profile/session locks. Its management proof helpers must not gate ordinary viewing reads.
- Do not directly reuse the management-specific `access::allowed(..., "can_use_live_tv")`: it assumes admin defaults for arbitrary capabilities, whereas the real authentication defaults omit `can_use_live_tv` for admin/member/guest. Existing semantics grant it through an explicit override or global-owner bypass. Reuse the real capability resolver on the locked connection; do not silently add an administrator viewing grant.
- `transactions::begin` reaches a helper that initializes `household_owner_state`. A catalog GET should instead use a focused lock-only authority path, avoiding new management-state initialization on reads. Existing session activity maintenance remains an authentication concern.
- Profile access is explicitly default off. Require the current assignment/profile policy; management delegation flags are not viewing grants. An administrator catalog request must not become a cross-household browse API.

### Source and snapshot authority

- `imports/status.rs::load` authenticates snapshot attribution/counts, but `current_source` only checks active household, enabled assignment, non-null destination envelopes and equal source generation. This is not complete viewer authorization.
- `feed/source.rs::decode` opens saved credentials and validates origin-bound destination approval, address count and selected approved address without networking. A catalog authority adapter can use the same validation while keeping URLs and addresses internal.
- A failed same-generation refresh deliberately retains the usable last-good snapshot. Every source-generation change makes old saved data unavailable until a fresh matching-generation import succeeds. The latest attempt need not be `succeeded` when an older valid pair remains current.
- The manual-refresh control governs refresh admission/cancellation. Disabling it retains data and does not itself revoke source/viewing authority. Avoid coupling catalog reads to that switch.
- Publication atomically replaces both feeds, updates the current snapshot pointer and deletes the prior snapshot and children. Page continuation must account for that replacement; old snapshots are not a historical archive.

### Identity and ordering

Duskcue-created snapshot/channel-entry/guide/programme rows use UUIDv7, enforced by the migration. Those row IDs are recreated on every successful import. They are snapshot-entry identities, not persistent favorites identities.

The separate internal 32-byte channel key is SHA-256 over assignment identity and the upstream Dispatcharr UUID. It survives refresh/credential rotation and changes scope on assignment replacement. The upstream UUID and guide-match key remain internal. Do not rename this hash as a UUID or derive a new UUID version for it.

`UNIQUE(snapshot_id, channel_key)` already supports bounded deterministic channel-key pagination. There is no persisted lineup ordinal; display names and numbers are encrypted. Key order cannot be described as Dispatcharr's curated lineup order. Persisting lineup order or independently stable public UUIDv7 channel identities would require additional importer/storage lifecycle work and explicit design choices.

### Profile-context continuation

There is no monotonic session profile-context revision. Switching profiles changes the active profile/selection/unlock fields and activity timestamp, but does not rotate the session or advance credential version. Therefore session/profile/version binding rejects an A-to-B mismatch, but an A-to-B-to-A round trip can revive the same tuple.

The activity timestamp is unsuitable as an epoch: ordinary authentication and parent unlock also update it, and transaction-start timestamps are not a monotonic context counter. The confirmed strict continuation choice therefore requires a dedicated session `profile_context_revision`, advanced at the database boundary when active profile or selection state changes. Bind it into paging context, including A-to-B-to-A refusal, and leave ordinary activity updates independent. Include that migration and its trigger/exhaustion/rollback qualification in the complete contract before implementation.

### Encrypted evidence and resource bounds

- The snapshot manifest authenticates attribution and counts, not a digest of every child. Each returned row needs its own authenticated decryption. Do not claim completeness against malicious database deletion/restoration.
- `imports/codec.rs` binds household, assignment, snapshot, source generation, attempt, sequence, import timestamp, row ID, kind/purpose and indexed guide/key/time/quality fields. A narrow typed reader must reconstruct the complete binding; generic decryption or manifest verification alone is insufficient.
- Metadata, stream and logo use separately sealed purposes. Metadata can be read without opening stream/logo references. Avoid exposing private parser models or arbitrary stored JSON through the HTTP layer.
- Metadata plaintext is bounded at 4,096 bytes, ciphertext at 5,518 bytes; imports allow up to 10,000 channels/guides and 250,000 programmes. Loading/decrypting an entire snapshot per request is unsuitable. Bound selected rows, decrypted/serialized output, query input, database time and request time.
- Existing programme indexing is `(guide_id, starts_at)`. Per-channel reads must first resolve that channel's same-snapshot guide relation. Distinct channels may intentionally share a guide. Nullable/tied times require explicit ordering and a separately defined treatment of unpositionable evidence.
- Guide metadata preserves language-tagged text variants, raw rating system/value pairs, raw/reduced-precision times and flags: uncertain=1, conflict=2, overlap=4, clump=8. Missing stops are not inferred. Those records are evidence, not a rating interpretation or playback grant.

## Official-source research

URLs were discovered through MCP search and retrieved on October 25, 2026. Canonical discovered URLs were used to verify document identity where opaque search references resolved to unrelated results. Product limits and API choices below are Duskcue recommendations, not mandates from these sources.

| Primary source | Application |
|---|---|
| [OWASP API1: object authorization](https://api-security.owasp.org/editions/2023/en/0xa1-broken-object-level-authorization/) | UUIDs are not access control. Check current principal, selected profile and household/object relation for every page and channel lookup. |
| [OWASP API3: property authorization](https://api-security.owasp.org/editions/2023/en/0xa3-broken-object-property-level-authorization/) | Explicit response DTOs should expose only allowed display/evidence fields. Do not serialize stored JSON or secret-bearing models wholesale. |
| [OWASP API4: resource consumption](https://api-security.owasp.org/editions/2023/en/0xa4-unrestricted-resource-consumption/) | Enforce record/page/input/output/time bounds and retain current rate limiting. A row-count limit alone does not cap serialized bytes. |
| [PostgreSQL 18 SELECT](https://www.postgresql.org/docs/18/sql-select.html) | LIMIT requires deterministic ordering; locking modes and SKIP LOCKED have distinct semantics. Inference: use a strict keyset and an authority/publication lock protocol, not skipping locked catalog rows. |
| [PostgreSQL 18 isolation](https://www.postgresql.org/docs/18/transaction-iso.html) | Repeatable Read provides a stable transaction view, but does not by itself mean the latest revocation was observed. Choose and qualify current-authority checks together with snapshot coherence. |
| [RFC 9110 conditional requests](https://www.rfc-editor.org/rfc/rfc9110.html) | Normal request checks precede HTTP preconditions. If conditional reads are introduced, validators must never bypass current authorization. A paging fence is not automatically an HTTP representation validator. |
| [RFC 9111 caching](https://www.rfc-editor.org/rfc/rfc9111.html) | no-store covers private/shared cache storage but is not a substitute for authorization or transport protection. Reuse the existing private Live TV response policy on success/refusals. |
| [XMLTV DTD](https://github.com/XMLTV/xmltv/blob/master/xmltv.dtd) | The format permits reduced date precision and optional stop times, language variants and clumped/overlapping listings. Known start/stop intervals are half-open. Preserve uncertainty; do not invent programme boundaries. The importer is a bounded evidence lane, not a claim of full DTD validation. |
| [RFC 9562 UUIDs](https://www.rfc-editor.org/rfc/rfc9562.html) | Continue UUIDv7 for Duskcue row identities. Timestamp layout and optional monotonicity do not establish persistent identity across new row creation or a curated playlist-order contract. |
| [W3C language tags](https://www.w3.org/International/articles/language-tags/index.en) | Keep language metadata attached to its text. Treat unknown upstream labels as raw evidence rather than silently claiming validated BCP 47 or choosing the server's language. |
| [W3C time/timezone guidance](https://www.w3.org/TR/timezone/) | Distinguish actual instants from incomplete/floating evidence and use explicit UTC instants where available. The retrieved publication is a Group Draft Note, not an endorsed W3C Recommendation; apply it as informative guidance. Do not invent a location/timezone from an offset. |
| [WCAG 2.2](https://www.w3.org/TR/wcag/) | Preserve text/language/time/state information needed by an accessible future guide. Keyboard/focus/status/contrast/zoom and assistive-technology execution belong to the client milestone; backend metadata cannot establish UI conformance. |

## Alternatives and recommended stack

| Choice | Pros | Cons | Recommendation |
|---|---|---|---|
| Selected standard-profile catalog with current transactional authority | Reuses household policy; no management proof merely to browse; prevents path-selected household/profile bypass. | Requires a narrow viewing adapter and revocation-race qualification. | Confirmed. |
| Reuse a management helper unchanged | Small patch. | Wrong admin capability defaults, management-state writes or fresh-proof requirements can change viewing behavior. | Reject as a direct reuse strategy. |
| Typed metadata projection without stream/logo references | Limits accidental credential disclosure and introduces no outbound fetch lifecycle. | No artwork/playback from this slice. | Confirmed. |
| Return stored JSON or upstream URLs | Fastest client hookup. | Expands the exposed contract and can disclose credentials or trigger unapproved browser networking. | Reject. |
| Snapshot-bound keyset continuation | Finite indexed pages; coherent snapshot and explicit refresh boundary. | Refresh can require a new read; profile-change history needs an epoch if strict invalidation is promised. | Confirmed. |
| Silently restart on a new snapshot | Fewer visible restart responses. | Can mix lineups/programmes or skip/duplicate entries across pages. | Not recommended. |
| Explicit bounds and existing PostgreSQL indexes | Avoids snapshot-sized decryption and new cache invalidation state. | Guide-window/null ordering and query-plan evidence still need qualification. | Prefer. |
| Computed now/next, stable favorites, curated ordering and logos together | Richer first client surface. | Adds identity/import/order/network/policy concerns before the read boundary is qualified. | Review as separate dependent scope choices. |

Recommended foundation: the existing Rust/Axum/PostgreSQL stack, ring-backed contextual authenticated encryption, UUIDv7 row IDs and bounded typed JSON. Use focused `live_tv/catalog/` HTTP/service/authority/cursor/projection modules and a narrow authenticated import reader. Reuse locked dependencies; no broker, background refresh owner or large singleton is needed for request-owned reads. Retain the existing explicit CORS policy and no-store response middleware.

## Design-review frontier

Round one is confirmed: normal-build selected-standard-profile scope; metadata-only allowlisted responses; and snapshot/profile-bound pagination with explicit stale continuation refusal. Normal reads retain current can_use_live_tv, explicit profile access, enabled assignment and approved matching-generation source/snapshot authority. They require no owner-management proof and grant no playback access.

Round two asks for the now-unblocked choices: snapshot-entry versus separately stable public channel identities; deterministic internal-key order versus imported curated ordering; and windowed per-channel evidence plus a separate unpositionable view versus computed now/next. Strict invalidation is settled by round one and requires the dedicated profile-context revision described above. Existing last-good retention, source-generation revocation, management-proof separation, Kids disabled, playback/UI/scheduling deferrals and UUIDv7 row strategy remain settled.

Before implementation, record the complete route/DTO/cursor/authority/limits/error/lifecycle contract and obtain the shared-understanding confirmation required by [the local grill-me skill](../../.agents/skills/grill-me/SKILL.md) and [its interview procedure](../../.agents/skills/grilling/SKILL.md). Do not infer design answers from elapsed time.

## Planned qualification

- Real cookie/bearer production routes with selected/unselected/foreign/Kids profiles; correct existing owner/admin/member/explicit capability behavior; no management proof required for authorized viewing reads.
- Current account/session/credential/profile/access/source checks with database-observed revocation, profile switch and publication races. Qualify profile round trips and the required profile-context revision.
- Authenticated manifest and row bindings, wrong-purpose/cross-household/snapshot/index tampering, no stream/logo/reference leakage or network fetches, safe errors/logs and no-store on all refusal paths.
- Empty snapshots, failed-refresh retention, fresh-generation refusal, shared guide relations, sparse language/time/rating evidence, overlap/conflict/clump and explicit unpositionable evidence behavior.
- Cursor/context/filter/snapshot/tie/null ordering, tampering and page/output/input bounds; maximum-sized representative snapshots and actual query-plan evidence before adding indexes.
- Request cancellation, database lock/statement deadlines, pool return and real root/SSE regression as affected. Run heavy commands serially through the unchanged resource guard; preserve reserves/jobs/threads/container CPU/PID/deadlines.

## Baseline outcome

The unchanged bounded-import suite passes **13 tests**, zero failures/ignores, 994 filtered cases, in **30.13 seconds**:

```powershell
node scripts/testing-memory/run.mjs cargo-test --features playback-qualification --lib domains::live_tv::feed::imports::tests
```

Receipt: `.cache/testing-memory/2026-10-25T22-24-12-538Z-17ac7968-f47d-4ff5-b966-61c0ce20ea14.jsonl`. Wrapper exit zero, no memory stop/interruption, minimum available host memory 21,001,924,608 bytes and peak owned private bytes 315,437,056. This proves the existing import/crypto/uncertainty baseline, not the proposed catalog API. The research checkpoint also passes whitespace checks and 599 local link targets across four Markdown documents. No new implementation or lint pass is implied.

## Random open PR applicability

Fresh MCP search returned one open PR. Uniform selection from that singleton chose [PR 5](https://github.com/cloudbyday90/Duskcue/pull/5), head `c018d77b7f09543e5960a571dc43e2f061cedf0a`, open and unmerged. MCP metadata, exact two-file list and both per-file patches were inspected. The PR requests PostCSS 8.5.23 and Nano ID 3.3.16 in the web/desktop lockfiles; both already resolve 8.5.30/3.3.20. No patch, downgrade, merge, comment or closure is applicable.

## Next step

Resolve the second design round, document the complete bounded read contract and confirm shared understanding, then implement and qualify it modularly on main. Client catalog UI, owner unlock, automatic refresh, Kids programme policy, shared playback admission and actual Dispatcharr/client playback remain separate increments. No release is part of this work.

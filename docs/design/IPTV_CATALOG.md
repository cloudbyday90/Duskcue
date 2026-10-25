# Profile-scoped IPTV catalog reads

## Status and scope

Research began October 25, 2026, against `b89e06a632e44628087c70b2195cf9963980a428` on `main`, following qualified [SSE root ownership](SSE_LIFECYCLE.md), source `7c1c23f`. This document records inspected source, MCP-discovered official guidance, alternatives and implementation prerequisites. All six product choices are confirmed: selected-standard-profile normal-build reads, allowlisted metadata, strict snapshot/profile continuation, windowed listings plus separate unpositionable evidence, UUIDv7 snapshot-entry IDs and deterministic internal-key ordering. The complete implementation contract below awaits shared-understanding confirmation. No catalog source changes, release or PR merge are claimed.

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
| [SQLx pool 0.9.0 documentation](https://docs.rs/sqlx/latest/sqlx/struct.Pool.html), [pool source](https://docs.rs/sqlx-core/latest/src/sqlx_core/pool/connection.rs.html), [transaction documentation](https://docs.rs/sqlx-core/latest/sqlx_core/transaction/struct.Transaction.html) | MCP pages identify the locked 0.9.0 version. Local pinned source confirms transaction Drop queues rollback and pool-owned return/close retains its permit. Await normal rollback and root pool closure; do not mistake Drop for error-reporting remote cleanup acknowledgment. |
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
| Windowed listings plus separately paginated unpositionable evidence | Delivers useful guide data without inventing missing intervals or certainty. | Consumers must handle evidence that cannot be placed on the timeline; no computed now/next yet. | Confirmed. |
| UUIDv7 snapshot-entry IDs and deterministic internal-key ordering | Reuses qualified storage and existing channel index; no persistent identity registry or lineup-schema rewrite. | Refresh changes public entry IDs; key order is not curated lineup order. | Confirmed; favorites identities and curated ordering follow separately. |
| Computed now/next, stable favorites, curated ordering and logos together | Richer first client surface. | Adds identity/import/order/network/policy concerns before the read boundary is qualified. | Deferred. |

Recommended foundation: the existing Rust/Axum/PostgreSQL stack, ring-backed contextual authenticated encryption, UUIDv7 row IDs and bounded typed JSON. Use focused `live_tv/catalog/` HTTP/service/authority/cursor/projection modules and a narrow authenticated import reader. Reuse locked dependencies; no broker, background refresh owner or large singleton is needed for request-owned reads. Retain the existing explicit CORS policy and no-store response middleware.

## Design-review frontier

Round one is confirmed: normal-build selected-standard-profile scope; metadata-only allowlisted responses; and snapshot/profile-bound pagination with explicit stale continuation refusal. Normal reads retain current can_use_live_tv, explicit profile access, enabled assignment and approved matching-generation source/snapshot authority. They require no owner-management proof and grant no playback access.

Round two is confirmed: existing UUIDv7 snapshot-entry identities, deterministic internal-key channel order, and windowed per-channel listings plus a separate paginated unpositionable view. Computed now/next, persistent public favorites identities and curated lineup-order storage are deferred. Strict invalidation is settled by round one and requires the dedicated profile-context revision described above. Existing last-good retention, source-generation revocation, management-proof separation, Kids disabled, playback/UI/scheduling deferrals and UUIDv7 row strategy remain settled.

Before implementation, record the complete route/DTO/cursor/authority/limits/error/lifecycle contract and obtain the shared-understanding confirmation required by [the local grill-me skill](../../.agents/skills/grill-me/SKILL.md) and [its interview procedure](../../.agents/skills/grilling/SKILL.md). Do not infer design answers from elapsed time.

## Concrete implementation contract for confirmation

### Scope and HTTP surface

Implement normal-build backend reads under the existing explicit household assignment/profile grants. Add no new global enablement switch and do not reuse the manual-refresh control as a viewing gate. All three routes derive household and selected profile from authentication; no household/profile selector is accepted from the request.

| GET route | Query and behavior |
|---|---|
| `/api/v1/live-tv/catalog/channels` | Optional `limit` and `cursor`. Read the current snapshot on the first page, then seek by the internal channel key in ascending order. |
| `/api/v1/live-tv/catalog/channels/{channel_id}/programmes` | Required `snapshot_id`, `from` and `to`; optional `limit` and `cursor`. Return complete known intervals intersecting the requested half-open window. |
| `/api/v1/live-tv/catalog/channels/{channel_id}/programmes/unpositionable` | Required `snapshot_id`; optional `limit` and `cursor`. Return records with either indexed time bound null, independently of any requested time window. |

Use the existing collection shape: `items`, `has_more` and an opaque `cursor` only when more items remain. Include a typed `context` on every successful page. Programme pages additionally identify `channel_id`, `view` (`window` or `unpositionable`), guide evidence and a normalized window or null. Responses are finite JSON, with no new conditional-read/cache optimization or generic body-collecting ETag layer.

`context` contains selected `profile_id`, decimal-string `profile_context_revision`, `snapshot_id`, `imported_at`, `observed_at`, imported `channel_count` and safe latest-refresh state/category/timestamps. It exposes no session credential, source URL, destination address, upstream UUID, guide-match key, internal channel key or encrypted envelope. The revision is an opaque decimal string to avoid client integer-precision loss. Latest-attempt metadata is distinct from the snapshot's own attribution.

An authorized valid empty lineup returns 200 and an empty final page. A channel without a guide relation returns empty programme pages with explicit absent guide evidence. Undeclared/ambiguous guide declarations remain distinguishable and do not prevent delivery of raw standard-profile metadata. No success asserts that programmes are current, ratings are understood, playback is allowed or any channel belongs in Kids mode.

### Current viewing authority and transaction boundary

Preserve shared cookie/bearer authentication, then perform catalog validation and bounded admission. A focused READ COMMITTED transaction uses explicit sequential statements to lock the household account, owned standard profile and exact session in that order. Validate active/not-deleted account, selected standard profile, no pending selection, current credential version, absolute/idle session validity and the real capability resolver's current `can_use_live_tv` semantics. Preserve the existing global-owner bypass; do not introduce an admin default. Do not call management-proof helpers or initialize `household_owner_state`.

Require current enabled assignment and explicit enabled profile access. Validate both saved URL/destination approval pairs, including origin, pins and stored counts, through the existing no-network decoder. Keep all decoded references internal. Require a current snapshot whose source generation matches that assignment, authenticate its manifest and reconstruct row receipts from the snapshot's own attempt/sequence/import timestamp. Never substitute the latest refresh attempt's attribution for retained data.

Retain the account/profile/session locks through bounded row selection, authenticated metadata projection, final time-sensitive session check and response materialization. Cooperating assignment/source/capability/publication writers share the account boundary. Profile/session-only switch, unlock, activity and revocation paths serialize through their respective locked rows; do not add a late account lock to those paths. Do not acquire another household or refresh-control lock from a catalog read. No SKIP LOCKED, stale authorization cache, replay transaction or automatic database retry is used.

The final authority check and materialization define the read's authorization point. A later revocation cannot recall a prepared or transmitted response. Each subsequent page must reauthorize and validate its complete context. Same-generation refresh failures retain valid last-good data with explicit timestamps/status; no new arbitrary guide-age expiry or freshness assertion is added. Assignment/source disable or revocation refuses reads, and generation changes require a successful new import before reuse.

### Snapshot and profile-context continuation

Add one migration with a positive BIGINT `user_sessions.profile_context_revision`, default one for existing/new application sessions, and a database BEFORE UPDATE trigger covering every session update. Advance it when active profile or profile-selection-required state changes; otherwise retain the old value even if an update supplies a different revision. Ordinary activity, parent unlock, device metadata and owner-proof expiry do not advance it. Existing grant/challenge invalidation triggers remain in force. Counter exhaustion rejects the context-changing transaction; it never wraps or resets.

No broad login response/model change is needed: the catalog adapter reads the current revision on its locked session. A-to-B, A-to-B-to-A and selection-required round trips invalidate old continuations. Logout, session expiry/deletion and credential rotation remain independently checked.

Use the existing contextual AES-GCM record encryption under a distinct catalog-cursor namespace, bound to household and exact signed-in session. Wrap its envelope in bounded URL-safe base64 for transport. The versioned plaintext also binds profile/revision, credential version, assignment/revision/source generation, snapshot, route kind, effective limit, channel/window where applicable, and the last strict sort tuple. Validate it against current server-derived context on every page; it is never a viewing credential or an authority source. No cursor registry or single-use proof semantics are introduced.

First channel reads start on the latest current snapshot. Programme requests explicitly name their snapshot. Current-authority refusal takes precedence over stale continuation. Once authority and current data are valid, any snapshot/profile/revision/query-context mismatch returns a fixed context-changed refusal; the client explicitly begins a new read. Do not silently restart, mix snapshots, retarget another channel/window or revive a cursor after switching away and back.

### Metadata projection and guide semantics

Expose channel entry UUIDv7, name, TVG name, group and opaque display-number hint. Preserve empty names and leading zeros; do not invent sorting or fallback names. Include typed guide evidence with `undeclared`, `ambiguous` and separate declaration display-name variants. Keep the declaration boundary instead of flattening conflicting declarations into asserted equivalence.

Programme entry UUIDv7 is snapshot-scoped. Return bounded title/subtitle/description/category language variants, raw rating system/value pairs, raw start/stop/clump-index strings, nullable normalized UTC instants, and explicit uncertain/conflict/overlap/clump indicators. Return all imported bounded variants rather than selecting the server's language. Missing/empty rating systems and values remain explicit. Unknown language labels are raw evidence, not validated HTML language attributes. Text is plain untrusted text; no HTML interpretation or client-fetch grant is implied by URL-like text.

Internal Deserialize-only projection types may ignore unexposed stored fields because the importer currently accepts some recognized XML fields in both declaration/programme contexts. Validate exposed field types/counts/bytes and emit Serialize-only response DTOs; do not use request-style unknown-field rejection on the entire stored JSON or serialize that JSON wholesale. Row structs and raw encrypted/import models remain non-serializable behind a narrow import-reader facade.

For the timed view, both normalized bounds must be present and `starts_at < to AND stops_at > from`. Preserve conflicting, overlapping and clumped entries without picking a winner. Order by `(starts_at, id)` ascending with a strict keyset. The unpositionable view is exactly `starts_at IS NULL OR stops_at IS NULL`, ordered by entry UUIDv7 ascending; it does not pretend those records belong to a selected window. Do not classify the lanes solely from flag bits, infer stops, normalize reduced precision, reinterpret rating systems or compute now/next.

### Fixed bounds, query plans and admission

| Resource | First-lane bound |
|---|---|
| Page size | Default 20, accepted range 1–100; fetch at most limit+1 for lookahead. |
| Timed window | Required explicit-offset RFC 3339 instants, at most microsecond precision without silent rounding; `from < to`, duration at most 24 hours. Normalize actual instants to UTC. |
| Raw query | At most 8,192 bytes; duplicate/unknown query fields and malformed/out-of-range values receive fixed errors. The unpositionable route accepts no time window. |
| Cursor | At most 2,048 transport bytes and 1,024 plaintext JSON bytes; strict version/shape and bounded decoded envelope. |
| Fetched ciphertext / decrypted metadata | At most 2 MiB / 1 MiB across the page, lookahead, distinct guide evidence and small manifest/authority/cursor records. Never load the whole snapshot. |
| Serialized response | At most 1 MiB, enforced before response transmission; no partial successful page on projection/integrity/bound failure. |
| Materialization concurrency | Four nonqueued operations per AppState, sharing one infallibly constructed admission service. Excess/closed admission uses the fixed unavailable response. The permit covers service materialization/release, not a permanently paused network client. |
| Work/deadlines | Three-second service-work budget including pool acquisition, with absolute-deadline checks between bounded CPU steps; LOCAL lock timeout 500 ms, statement timeout 1,500 ms and transaction timeout three seconds. Up to two further seconds for explicit normal-path transaction release. Existing authentication and HTTP transport budgets remain separate. |

Channel pagination reuses `UNIQUE(snapshot_id, channel_key)`. Programme paging needs a UUID tie-break and a useful unpositionable-ID path; add or extend narrowly scoped indexes based on actual EXPLAIN evidence while retaining guide-prefix support for foreign-key cleanup. Index additions must be qualified against import publication cost. Use bounded ID-first selection before loading ciphertext where it prevents unnecessary metadata work.

LIMIT bounds returned/decrypted rows, not necessarily examined rows: the residual stop-time predicate can still scan a large history. Qualify representative 250,000-entry single-guide historical-empty-window, dense-overlap, tied-start and null-heavy sets under the existing PostgreSQL fixture limits. Record actual EXPLAIN ANALYZE/BUFFERS, successful first/last pages and deadline refusals. Do not raise the statement/request budgets to hide an unsuitable query plan.

### Error and privacy contract

| Outcome | Mapping |
|---|---|
| Shared authentication failure or session revoked/expired during recheck | Existing authentication 401 behavior; no management-proof challenge. |
| Malformed query/path/cursor, invalid window/limit or excessive input | 400 `IPTV_CATALOG_001`, fixed request-invalid text. |
| Current viewing capability/profile/selection/access/assignment/source consent absent or disabled | 403 `IPTV_CATALOG_002`, fixed access-denied text. |
| Channel entry absent from the authorized current snapshot | 404 `IPTV_CATALOG_003`, fixed entry-unavailable text. |
| Valid current authority/data but stale snapshot/profile/paging context | 409 `IPTV_CATALOG_004`, fixed restart-read text. |
| No matching-generation snapshot, missing key, crypto/manifest/shape/bound/database/deadline/release failure, or busy/closing admission | 503 `IPTV_CATALOG_005`, fixed unavailable text and Retry-After 5. |
| Existing outer rate limiting | Preserve its existing 429 and Retry-After behavior. |

All application-route successes/refusals remain no-store through the existing Live TV private response policy, including extractor and outer limiter failures. Preserve explicit-origin credentialed CORS and ordinary JSON/problem media types. Never echo a cursor, imported text, credential, ciphertext, destination, raw SQL or raw crypto error in refusal bodies/logs. Add no successful-read audit trail, viewing history, stream reservation or playback side effect. Conditional-read optimization and client caching remain deferred; authorization never depends on a validator.

### Request ownership and shutdown

Keep reads owned by the HTTP request: no spawned catalog query/materialization/background refresh task. Immediately wrap an acquired pooled connection in a small default-discard lease before another await. The lease owns explicit manual BEGIN/READ COMMITTED setup, data queries, representation construction and ROLLBACK; do not rely on ordinary Pool::begin/Transaction Drop for an unacknowledged transaction start. A separate acknowledged SET TRANSACTION ISOLATION LEVEL READ COMMITTED precedes catalog data reads. Never read under an unchecked inherited isolation level.

After constructing/checking the finite representation and final authority, explicitly await ROLLBACK on success and ordinary error. Mark the lease clean only after its acknowledgment, with no further database work; only then may normal pool reuse and a data response occur. If the future is dropped at any earlier point or release is uncertain, the lease sets close_on_drop and leaves SQLx owning the close path. Never detach/leak the connection, return plaintext before release, or turn an uncertain release into success.

Pinned-source inspection found that PostgreSQL transaction depth advances after BEGIN acknowledgment, while start_rollback depends on positive depth. That makes executed-but-unacknowledged top-level BEGIN a specific source-analysis concern for ordinary Drop reuse; it is not a reproduced upstream vulnerability claim. Qualify this reader with actual lost/delayed BEGIN acknowledgment and caller cancellation, ensuring the uncertain connection cannot be reused. Investigation of unrelated existing transaction callers is separate.

SQLx pool-owned close retains its permit and may finish during awaited pool closure. Its internal close timeout/error suppression and lack of remote termination acknowledgment remain library limits; close_on_drop is a fallback, not an error-reporting cleanup certificate. Qualify real backend/lock disappearance, bounded cleanup and pool recovery without claiming stronger guarantees from Drop alone.

Close new catalog admission before root HTTP drain. Existing retained HTTP/SSE/root/import work still uses one shared thirty-second drain; only then await up to sixty seconds of pool closure, including pool-owned fallback. Preserve nonzero deadline/failure and continuing second-signal behavior. No extra sequential root timeout, management owner, process-global singleton, source fetch or scheduler is introduced.

### Implementation layout and deferrals

Keep the domain five-file HTTP pattern under focused `live_tv/catalog/` modules, with small authority, cursor, limits, admission/representation helpers as needed. Add a narrow typed `feed/imports/reader/` facade that reuses contextual manifest/row verification and exposes no stream/logo/raw model fields. Share the actual auth capability resolver on the existing connection without changing role defaults. Add the profile-context migration and only justified paging indexes, wire both AppState constructors/router/root admission closure, and retain the already approved legal headers without explanatory source comments.

Reuse locked Rust/Axum/Tokio/SQLx/ring/serde/base64/chrono/UUID dependencies; do not change Cargo.lock or SDK versions for these APIs. No release, deployment, PR merge, CHANGELOG update or separate branch. Keep client UI/recovery/owner unlock, automatic refresh, computed now/next, persistent favorites identities, curated lineup storage, logos, Kids rating enforcement, shared playback admission and actual Dispatcharr/client playback qualification separate.

## Planned qualification

- Real cookie/bearer production routes with selected/unselected/foreign/Kids profiles; correct existing owner/admin/member/explicit capability behavior; no management proof required for authorized viewing reads.
- Current account/session/credential/profile/access/source checks with database-observed revocation, profile switch and publication races. Qualify profile round trips and the required profile-context revision.
- Authenticated manifest and row bindings, wrong-purpose/cross-household/snapshot/index tampering, no stream/logo/reference leakage or network fetches, safe errors/logs and no-store on all refusal paths.
- Empty snapshots, failed-refresh retention, fresh-generation refusal, shared guide relations, sparse language/time/rating evidence, overlap/conflict/clump and explicit unpositionable evidence behavior.
- Cursor/context/filter/snapshot/tie/null ordering, tampering and page/output/input bounds; maximum-sized representative snapshots and actual query-plan evidence before adding indexes.
- Actual lost/delayed BEGIN acknowledgment, request cancellation, database lock/statement deadlines, clean-only-after-rollback pool reuse and real root/SSE regression as affected. Run heavy commands serially through the unchanged resource guard; preserve reserves/jobs/threads/container CPU/PID/deadlines.

## Baseline outcome

The unchanged bounded-import suite passes **13 tests**, zero failures/ignores, 994 filtered cases, in **30.13 seconds**:

```powershell
node scripts/testing-memory/run.mjs cargo-test --features playback-qualification --lib domains::live_tv::feed::imports::tests
```

Receipt: `.cache/testing-memory/2026-10-25T22-24-12-538Z-17ac7968-f47d-4ff5-b966-61c0ce20ea14.jsonl`. Wrapper exit zero, no memory stop/interruption, minimum available host memory 21,001,924,608 bytes and peak owned private bytes 315,437,056. This proves the existing import/crypto/uncertainty baseline, not the proposed catalog API. The research checkpoint also passes whitespace checks and 599 local link targets across four Markdown documents. No new implementation or lint pass is implied.

## Complete-contract checkpoint

All six product choices were confirmed after research checkpoint `c193a8d`. The route, DTO, strict profile-context revision, opaque paging, source/row authentication, fixed bounds, default-discard lease and qualification contract above is now concrete and awaits shared-understanding confirmation. Whitespace checks and **724 local link targets across eleven changed Markdown documents** pass. Source, manifests and Cargo.lock remain unchanged; the existing thirteen-case import baseline remains the only execution claim for this catalog work. No new catalog build, strict lint, route or runtime result is implied.

## Random open PR applicability

Fresh MCP search returned one open PR. Uniform selection from that singleton chose [PR 5](https://github.com/cloudbyday90/Duskcue/pull/5), head `c018d77b7f09543e5960a571dc43e2f061cedf0a`, open and unmerged. MCP metadata, exact two-file list and both per-file patches were inspected. The PR requests PostCSS 8.5.23 and Nano ID 3.3.16 in the web/desktop lockfiles; both already resolve 8.5.30/3.3.20. No patch, downgrade, merge, comment or closure is applicable.

## Next step

All six product choices are settled. Confirm shared understanding of the complete contract above, then implement and qualify it modularly on main. Client catalog UI, owner unlock, automatic refresh, Kids programme policy, shared playback admission and actual Dispatcharr/client playback remain separate increments. No release is part of this work.

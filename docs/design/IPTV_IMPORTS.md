# Bounded IPTV imports and last-good snapshots

## Status and scope

Research started October 25, 2026, against `22422191ee285773392e7f43b2e0dd53bdc65ca6` on `main`; checkpoint `3b02c8bc416707789de4a929d2486143ea72808e` records the initial investigation. The user has now confirmed all nine product decisions below. This document records the concrete implementation contract for shared-understanding review. No import implementation, schema migration, dependency update, build or runtime qualification is claimed yet. No release or PR merge.

[IPTV](IPTV.md) owns player behavior and milestones. [Saved-feed destination authority](../security/IPTV_DESTINATION_AUTHORITY.md), qualified at source `30511af8e2347426446b3c5ff3db08ad863e01ed`, owns encrypted source approval and controlled HTTP/TLS. [Assignment/delegation](IPTV_ASSIGNMENT.md) and [owner security](../security/HOUSEHOLD_OWNER_SECURITY.md) retain their existing boundaries. This document owns bounded import semantics, identity, publication, retention and the next qualification contract.

The user confirmed:

- Complete parsing, persisted last-good snapshots and controlled qualification behind the default-off feature first. Production refresh endpoints, scheduling and client catalog UI follow root-lifecycle integration.
- Failed or malformed refreshes retain last-good data with timestamp/status. Assignment deletion/replacement removes its catalog; disabled or revoked authority makes saved data unavailable for viewing.
- Treat both feeds as untrusted metadata. Fetch no stream/logo/playlist/XML reference and keep Kids disabled until programme-rating policy is qualified separately.
- Use the verified Dispatcharr proxy-UUID identity lane, including reverse-proxy prefixes. Guide keys do not identify playlist channels. Unsupported/duplicate channel identities reject the refresh.
- Publish playlist and guide as one coherent pair after both pass parsing and the source generation is rechecked transactionally.
- Preserve incomplete times and raw rating systems, coalesce exact duplicates without merging channels and mark conflicting/overlapping evidence as ambiguous. Do not invent end times or rating meanings.
- Accept UTF-8 M3U/XMLTV and bounded gzip-file XMLTV; reject DTD/custom-entity processing and other encodings. HTTP content coding remains absent/identity only.
- Publish structurally valid empty lineups. Malformed/error responses are failures, not empty success.
- Any source-generation change retains encrypted last-good data but makes it unavailable until a fresh matching-generation import succeeds. Reapproval alone cannot revive old derived credentials.

The product interview is complete. Shared-understanding confirmation of the complete contract below remains required before source changes; settled decisions are not reopened.

## Existing-source findings

The current `FeedOwner` returns bounded `Vec<u8>`. Its private fence contains household, assignment and source generation, but the result does not carry that fence into a publication transaction. A separate check before writing would leave a race. Playlist and guide loads also occur independently; a paired import needs one coherent admitted generation.

Administrator mutations cooperate through account-row locks followed by the assignment lock. Publication must use that same ordering and recheck source identity/generation inside its write transaction. Rotation/approval/configuration and account status/deletion transitions advance source generation. Owner profile/cap choices advance only the general representation revision. Deleting/replacing an assignment changes identity and cascades existing assignment-owned policy.

There is no channel/guide/import-run schema, last-good pointer or imported-data retention mechanism. The 64 KiB response body limit is explicitly a controlled network fixture bound, unsuitable as a production guide limit. There is no importer, decompressor or owned parser worker to reuse.

The generic scheduler tracks its polling loop, while executor wrappers spawn separate tasks and cancellation drops handler futures without separately joining their resources. Its config/run metadata are ordinary JSONB, and generic audit snapshots do not comprehensively redact nested URLs. This does not qualify it as the owner of an import pipeline or as a location for source/derived credentials. Root/scheduler integration stays outside this increment as confirmed.

Existing NFO parsing and metadata/archive decompression are not bounded-feed precedents: the former uses lossy UTF-8 and drops attribute errors; the latter reads whole inflated data without an import-specific ceiling. New services must not copy those assumptions.

## Official-source research

URLs were discovered through MCP search/GitHub services and retrieved October 25, 2026. Release-tagged Dispatcharr source and installed library inspection supplement current official documentation. Application recommendations and proposed limits are distinguished from format requirements; source inspection is not actual Dispatcharr interoperability qualification.

| Source | Relevant finding | Implication |
|---|---|---|
| [Dispatcharr v0.33.0 release](https://github.com/Dispatcharr/Dispatcharr/releases/tag/v0.33.0), [tagged M3U exporter](https://github.com/Dispatcharr/Dispatcharr/blob/v0.33.0/apps/output/views.py) | Standard proxy URIs carry the persisted Channel UUID. EXTINF has no separate UUID attribute. `tvg-id` is configurable and may instead be a channel number or guide identifier. XC/direct outputs have different URI identities. | Separate channel identity from guide matching. A proxy-UUID extractor is a version-qualified Dispatcharr adapter, not a generic M3U identity standard or an endpoint generator. |
| [Dispatcharr channel model](https://github.com/Dispatcharr/Dispatcharr/blob/v0.33.0/apps/channels/models.py), [EPG model](https://github.com/Dispatcharr/Dispatcharr/blob/v0.33.0/apps/epg/models.py) | Channel UUID is persisted/unique; TVG-ID is not globally unique. Profile membership is separate from the Channel row. | Inference: profile exclusion/readdition or export-query rotation does not itself replace an existing Channel UUID. Channel deletion/recreation can change it; no preservation guarantee is inferred across recreation. |
| [Dispatcharr XMLTV exporter](https://github.com/Dispatcharr/Dispatcharr/blob/v0.33.0/apps/output/epg.py), [stream/cache helper](https://github.com/Dispatcharr/Dispatcharr/blob/v0.33.0/apps/output/streaming_chunk_cache.py) | Application output is UTF-8, without a DOCTYPE or gzip conversion/content coding. Programme times are complete UTC timestamps. Duplicate chosen guide keys can emit repeated channel/programme data. | Strict UTF-8/no-DTD processing is compatible with this inspected exporter. Reverse proxies may change wire coding; that requires separate qualification. Shared guide keys must not merge playlist channels. |
| [XMLTV DTD](https://github.com/XMLTV/xmltv/blob/master/xmltv.dtd) | Partial timestamps and missing stops are permitted; absent timezone means UTC. Same-slot clumps can be valid. Rating schemes are optional free text, and review-star ratings are separate. | Preserve incomplete time/rating evidence rather than invent durations or a universal age scale. Neither XML parsing nor an upstream rating label grants Kids access. |
| [Official Kodi IPTV Simple](https://github.com/kodi-pvr/pvr.iptvsimple) | Its documented IPTV examples include distinct channels sharing `tvg-id`; TVG-ID is optional. | A guide identifier is not a safe primary channel identity. Generic IPTV interoperability is broader than the initial Dispatcharr-specific lane. |
| [W3C XML 1.0](https://www.w3.org/TR/xml/), [OWASP XXE prevention](https://cheatsheetseries.owasp.org/cheatsheets/XML_External_Entity_Prevention_Cheat_Sheet.html) | XML well-formedness, legal characters, declarations and entity handling require explicit policy. External resource denial alone does not bound internal expansion. | Use a strict nonvalidating subset, reject DTD/custom-entity processing, enforce application limits and require one complete root. Never log raw parser diagnostics. |
| [W3C XML internationalization](https://www.w3.org/TR/xml-i18n-bp/), [security/privacy review guidance](https://www.w3.org/TR/security-privacy-questionnaire/) | Preserve language distinctions and review unnecessary data exposure/ambient authority. | Keep bounded language metadata and plain text; do not interpret imported text as HTML or expose source/derived URLs. Future client accessibility execution remains separate. |
| [quick-xml releases](https://github.com/tafia/quick-xml/releases), [installed 0.41 documentation](https://docs.rs/crate/quick-xml/0.41.0) | Installed 0.41 has no application depth/event/output budget. Current 0.42 adds UTF-8 validation and several namespace/Serde safeguards, with breaking string APIs. | Prefer an isolated new-import dependency on 0.42 rather than migrating existing NFO/plist consumers incidentally. Library upgrades do not replace root/depth/field/record/byte checks. |
| [GZIP format, RFC 1952](https://www.rfc-editor.org/rfc/rfc1952.html), [flate2 decoder](https://docs.rs/flate2/latest/flate2/bufread/struct.MultiGzDecoder.html), [flate2 releases](https://github.com/rust-lang/flate2-rs/releases) | Gzip can contain concatenated members. Full consumption is necessary for footer/trailing-data validation; output bytes alone do not bound member processing. Current flate2 1.1.10 tightens EOF handling and changes the miniz backend. | Bound compressed/inflated bytes, member count, work and time independently. Review the patch/backend change and run compression regressions. Installed gzip already validates CRC/ISIZE; no claim is made that 1.1.9 necessarily accepts truncated gzip. |
| [Tokio blocking tasks](https://docs.rs/tokio/latest/tokio/task/fn.spawn_blocking.html) | A started blocking task cannot be stopped by aborting/dropping its handle. | Retain import admission and task ownership until the parser actually joins; cancellation must be cooperative and bounded. |
| [PostgreSQL application consistency](https://www.postgresql.org/docs/current/applevel-consistency.html), [PostgreSQL 18 SELECT](https://www.postgresql.org/docs/18/sql-select.html) | Unlocked reads do not prevent a later conflicting write. Explicit row-lock ordering and transaction isolation matter. | Recheck the opaque source receipt under account/assignment locks in the same transaction that publishes the new last-good pointer. |

The inspected XMLTV DTD blob is `305d1c634f5389c47159e90b76a664a1c2dd5043`. The Dispatcharr findings use tag `v0.33.0` rather than moving main. No installed Dispatcharr instance or actual household export was available.

## Alternatives, pros and cons

| Choice | Pros | Cons | Recommendation |
|---|---|---|---|
| Proxy UUID identity scoped to assignment | Stable across ordinary export-query changes; independent of shared guide keys; no heuristic channel merges. | First lane excludes generic, XC and direct-provider formats; exporter changes need qualification. | Confirmed. |
| Names, guide IDs or full-URL identity | Broad initial acceptance. | Names/guide IDs collide; credential-bearing paths/queries change and may leak through identity/debug fields. | Do not use as silent fallback. |
| Atomic playlist/guide publication | Readers never observe a mixed partial refresh; failed input preserves a coherent last-good pair. | A malformed guide delays an otherwise valid lineup update. | Confirmed. |
| Independent feed publication | Guide and channel refreshes can advance independently. | Requires explicit cross-version matching, stale-reference and permission semantics. | Not selected for this increment. |
| Strict UTF-8, no DTD/custom resources | Compatible with the inspected exporter; smaller parser/trust surface. | Some valid XMLTV from other producers needs transcoding or inert-DOCTYPE compatibility later. | Confirmed, including bounded gzip-file guides. |
| Per-record encrypted metadata/resources | Derived URLs and untrusted text avoid plaintext/audit exposure; reuse contextual AES-GCM. | Adds encoding, encryption and aggregate-record ceilings. | Recommended storage boundary. |
| One retained last-good snapshot | Bounded history; refresh failure changes status, not valid catalog data. | No historical rollback UI. | Recommended; preserve stable channel identity as assignment plus upstream UUID, independently of snapshot-row identity. |
| Owned bounded parse/publication pipeline | Cancellation and capacity cover work after fetch completion; source changes cannot publish stale data. | Requires new receipt, parser ownership and transaction qualification. | Required before production refresh integration. |

## Recommended stack

Retain Rust/Axum, PostgreSQL, existing owner-security transactions, contextual ring AES-GCM, URL parsing and the qualified Hyper/Tokio-Rustls destination boundary. Add a narrowly featured, optional renamed quick-xml 0.42 dependency for the new importer while retaining existing 0.41 consumers. Prefer the current flate2 1.1.10 patch for a qualified gzip lane, documenting its miniz backend change and checking other existing compression consumers. These are proposals; no dependency has been changed at this checkpoint.

Use separate M3U parsing, XMLTV structure/fields, timestamp evidence, compression/limits, secret encoding, source receipt, publication and operation-owner services. Preserve the Live TV domain's existing five-file HTTP boundary. Introduce no global singleton, Dispatcharr management client, scheduler secret payload, parser network resolver or production endpoint.

## Concrete implementation contract

### Entry points and source receipt

Add an import owner/handle and metadata-only status service behind `playback-qualification`. Its trusted in-process start selects a household and an index within each already approved playlist/guide pin set. It accepts no caller URL, replacement IP, TLS name, headers, parser limits or trust roots. Reuse the public-root TLS path and existing fixed-fixture CA constructor; do not add arbitrary CA configuration or a verification bypass.

Admit both saved feeds at the same household/assignment/source generation before any connection. Reject missing approval, inactive/deleted households, disabled assignments, mismatched source generations and out-of-range pins. A private, non-debug/non-serializable receipt retains that identity through fetch, parse and publication. Preserve the earlier 64 KiB/15-second network-fixture lane; import limits are a separate internal purpose, not a silent widening of the old `FeedOwner.start` contract.

Claim a fresh UUIDv7 attempt and advance a positive sequence under account-first, assignment-second locks. The latest attempt marker is durable. A newer claim supersedes an older attempt, even when both use the same source generation or come from distinct owner instances. Conditional status updates must never let a late older attempt overwrite the newer attempt's state. Arithmetic overflow refuses the claim without networking or partial publication.

### M3U identity and supported metadata

Require a UTF-8 playlist, optionally with one UTF-8 BOM, beginning with `#EXTM3U`. Support LF/CRLF and complete EXTINF/URI pairs with quote-aware attribute parsing. Validate attribute syntax and reject duplicate attributes, orphan URIs and incomplete entries. Bound unknown tags/attributes but do not execute player options or follow header guide links. HLS media/master constructs are outside this channel-list lane.

Each stream reference must be an absolute HTTP/HTTPS URI with the verified final path components `proxy/ts/stream/{canonical UUID}`, allowing a reverse-proxy prefix and optional final slash. Preserve the supplied URI, including query credentials, as an encrypted reference; never generate a replacement endpoint. Reject missing/unsupported identities and two entries with the same normalized upstream UUID. Recognizing a URI shape is not proof of upstream authenticity, authorization, proxy mode or playable MPEG-TS content.

Channel identity is assignment plus upstream UUID. Represent it internally with a stable, assignment-scoped SHA-256 channel key over a versioned fixed prefix and the two binary UUIDs. Names, guide IDs, URL origins, prefixes, paths and query credentials do not enter that key. Preserve the raw upstream UUID only inside encrypted metadata. The key is neither authentication nor an egress grant. Snapshot-entry UUIDv7 row identities remain distinct from this stable channel identity, so remove/readd does not require an unbounded tombstone registry. Explicit assignment replacement changes the identity scope.

Retain bounded display name, group, channel-number hint, TVG name and optional guide-match key. Treat guide keys as opaque, case-sensitive data; do not match by channel name or silently synthesize keys. Store an optional logo only as an encrypted opaque reference. No stream/logo reference is fetched, rendered or returned to ordinary clients.

### XMLTV structure and evidence

Require one complete XML 1.0 `tv` document in UTF-8, with optional BOM/declaration. Enable library structural checks and separately verify root completion, balanced depth, legal XML characters, attribute errors/duplicates and allowed trailing content. Use the non-namespace XMLTV element/attribute subset; `xml:lang`/`xml:space` may be recognized, but unsupported namespace extensions do not enable a resolver. Unknown bounded metadata is skipped structurally, not interpreted as HTML, executable instructions or a source override.

Reject DOCTYPE/custom entity processing and unsupported encoding declarations. Permit only the five predefined XML entities and valid numeric references, checking resolved characters too. Combine supported text/CDATA/reference events before applying field whitespace policy, preserving meaningful spaces and language labels. Never discard malformed-attribute errors or log library error/debug strings containing source text.

Keep guide entries only for nonempty keys referenced by the imported lineup, while input limits count unmatched/duplicate records too. Missing channel declarations can produce an explicit undeclared guide record; absence of a guide match stays unavailable. Distinct playlist channels may reference the same guide record. Coalesce exact duplicates of supported normalized metadata, not distinct channels. Preserve a bounded set of distinct declarations/display-name variants and mark contradictory declarations as ambiguous.

Programme metadata includes bounded language-tagged titles, subtitles/descriptions, categories, raw start/stop, clump information and parental rating system/value pairs. Review-star ratings and unsupported credit/artwork/link fields do not become parental classifications. No rating scale or missing system is invented.

Retain missing/reduced-precision/malformed or unsupported time evidence with an unavailable-state flag. Normalize an instant only from a complete fourteen-digit timestamp and valid numeric offset, or the XMLTV-defined UTC default when the zone is absent. Named/unknown zones remain explicit rather than being interpreted through host-local time or Chrono `%Z`. Missing stops are not inferred from adjacent programmes. Invalid intervals, conflicting same-slot records, overlaps and clumps remain explicitly uncertain; they do not authorize Kids access. Exact duplicates are removed before ambiguity detection so repeated exports do not create a false overlap by themselves.

A valid empty M3U plus valid XMLTV publishes an empty catalog. Ignored unmatched guide data does not preserve removed channels. Error pages, malformed XML/M3U and exhausted bounds are not empty success.

### Fixed initial qualification limits

These are supported-lane ceilings, not universal production feed limits or exact allocator/RSS claims. Exhaustion rejects the whole candidate; it does not truncate metadata or publish a prefix. Numerical engineering limits are reviewable here rather than exposed as new operator settings.

| Resource | Bound |
|---|---|
| Playlist body / line / EXTINF attributes | 8 MiB / 16 KiB / 32 |
| Guide wire body / aggregate decoded XML | 64 MiB / 128 MiB |
| Gzip file members | Eight; consume every admitted member/footer, check decoder CRC/ISIZE/completion and reject trailing non-member bytes |
| XML depth / attributes per element / event bytes | 32 / 64 / 64 KiB |
| XML events / declarations / input programmes | 8,000,000 / 20,000 / 250,000, including ignored and duplicate records |
| Distinct playlist channels / referenced guide keys | 10,000 each |
| Guide key / language label / supported text field | 512 / 64 / 4,096 UTF-8 bytes, subject to tighter aggregate-record bounds |
| Language values per text field / category, rating or distinct declaration entries | Four / eight; all remain subject to the metadata-record ceiling |
| Metadata JSON record / individual stream or logo reference | 4,096 bytes each; existing strict envelope ceiling 5,518 bytes |
| Total normalized plaintext payload / total encrypted payload | 96 MiB / 144 MiB |
| Publication batch | At most 250 rows and 1 MiB conservatively counted encoded payload/parameter overhead; validate equal array lengths |
| Concurrent imports | One per retained import owner, no admission queue; one owner must be retained by each future application root |

Gzip applies to the guide resource payload, detected from its bytes, not its filename or MIME label. It does not enable HTTP `Content-Encoding: gzip`. M3U stays uncompressed UTF-8. Check compressed/decoded bytes, member count, work/cancellation and complete input independently; consuming only a valid XML prefix is insufficient.

Retain the existing 32 KiB HTTP response-head budget, 64-header cap, five-second connect/TLS/header/body-idle limits, no redirects/proxies/retries/pooling, and exact peer/TLS-name checks. Allow at most 60 seconds for each import fetch, 60 seconds for parser/encoding work and 180 seconds for the entire operation before commit. Phases share the remaining overall budget; their ceilings are not additive extensions. Poll authority/attempt checks at the existing target 200 ms interval with one-second check deadlines. These controls do not replace worker joining or database cleanup.

### Schema and encrypted storage

Add a focused migration with the following records; add no channel/guide/profile delivery endpoint:

- `live_tv_import_state`: one row per assignment, composite household/assignment foreign key, latest UUIDv7 attempt, positive sequence, fixed state/error category and timestamps, plus nullable current-snapshot pointer. A stale attempt may not update this row conditionally as if it were latest.
- `live_tv_catalog_snapshots`: UUIDv7 identity, household/assignment/source-generation/attempt attribution, counts, receipt timestamps and a bounded authenticated manifest of that attribution/count metadata. Enforce at most one committed snapshot per assignment with a deferred uniqueness constraint, allowing replacement within one transaction.
- `live_tv_catalog_guides`: UUIDv7 row identity, snapshot linkage and encrypted bounded guide metadata. Raw guide keys/names are not plaintext database identifiers.
- `live_tv_catalog_channels`: UUIDv7 entry identity, snapshot linkage, fixed-size stable channel key, optional same-snapshot guide reference, encrypted metadata and separate encrypted stream/logo references. Enforce per-snapshot channel-key uniqueness.
- `live_tv_catalog_programmes`: UUIDv7 entry identity, same-snapshot guide reference, nullable reliable UTC bounds, explicit quality flags and encrypted bounded metadata. Composite foreign keys prevent guide references crossing snapshot/household boundaries.

All snapshot children cascade on snapshot removal; assignment deletion/replacement cascades state and catalog. Use existing strict contextual AES-GCM, never compatibility/plaintext decryption. Bind household, assignment, snapshot, its source generation/attempt attribution, record kind, row identity and field purpose. Bind indexed channel/guide/time/quality columns through a versioned row-binding digest in associated data, so editing a join, channel key, source epoch or timing flag cannot silently relabel an encrypted record. Verify the manifest before reporting authenticated current-source attribution; that manifest contains no URLs or programme text. This is context/tamper protection, not a promise against restoration of a complete older database/key snapshot. Keep requests/internal plaintext objects non-debug/non-serializable to external clients; database rows have no Serialize. Internal payload serialization exists only for bounded authenticated storage.

Use fixed system-attributed import audit events with identities, categories and counts only. Do not invent a human initiator for a server operation. No raw inputs, labels, guide keys, URLs, ciphertext snapshots or parser/SQL diagnostics enter audit, generic scheduler JSON or error bodies. Qualification status exposes only identities/counts/timestamps/categories and whether source attribution is still current; it grants no viewing/profile permission. Existing assignment `network_checked: false` / `playback_available: false` metadata remains unchanged.

### Atomic publication, failures and cleanup

Fetch/parse/normalize/encrypt outside a write transaction. Do not persist partially parsed staging records or cache raw feed files. Then borrow a transaction from a separately retained pooled connection, follow the existing account-first/assignment-second locking order and recheck active account, enabled assignment, source identity/generation and latest attempt under those locks.

Insert the entire bounded candidate in checked batches, update the last-good pointer and latest-success metadata, and delete the old snapshot in the same transaction. Readers cannot observe a partial pair or mixed snapshot. Failed parsing/networking, stale authority, supersession or a confirmed pre-commit rollback preserve the prior snapshot and update only the matching attempt's safe status. Data retained after a source change remains unavailable until its snapshot generation matches freshly admitted source authority again.

Keep the established five-second lock and ten-second statement limits. Add a per-transaction twenty-second PostgreSQL transaction timeout, without changing global database settings, and a twenty-second application publication budget; batches do not reset either whole-publication budget. A small shared lock-initialization helper may be factored from owner transactions so the importer can retain connection ownership without duplicating the lock protocol. Existing interactive proof/grant semantics stay unchanged and receive regression coverage.

Before commit, cancellation/timeout must explicitly await rollback or close the retained connection; dropping a query/transaction alone is not cleanup evidence. SQLx drop queues rollback and a dropped pooled connection schedules background cleanup. Publication/cleanup can therefore outlast a cancelled caller. Preserve ownership and admission through that work; cleanup failure is not reported as completed rollback or successful import.

Treat COMMIT as a separate decision phase. Once it begins, do not drop its future merely because a caller cancels. Await its bounded outcome. An acknowledged commit is published; a lost response/transport/deadline can be indeterminate, not an ordinary failed refresh. Close/retire the owned connection and reconcile using the candidate/attempt identity under the same account/assignment locks when possible. A found receipt proves commit; an immediate absent read does not prove rollback. If replacement/deletion or a later publication removed evidence, report superseded/indeterminate current state rather than invent historical certainty. Do not automatically replay network fetches or publication after an uncertain outcome.

Retain only one committed last-good snapshot and the latest attempt summary, with no historical rollback archive or unbounded run log. Publication deletion is transactional; no persistent candidate-stage janitor is introduced. Valid empty publication uses the same atomic replacement path. A safe status read separates retained data, failed/stale attempt state and current source attribution; ordinary client catalog reads remain deferred.

### Operation ownership and qualification

Hold import admission through both network reads, cooperative CPU work, transaction completion and cleanup. Run parsing/encoding on a retained blocking worker with byte/event/member/deadline/cancellation checks. Never treat aborting/dropping its JoinHandle as proof that work stopped. On cancellation or a stale source/attempt, signal the worker and join it before releasing its lease. Close admission before owner shutdown. Use a thirty-second cleanup/join deadline; if it cannot complete, surface cleanup failure and retain/quarantine outstanding ownership rather than admit replacement work as if idle.

Split the implementation into focused source-receipt/attempt, M3U, XML structure/fields, time evidence, limits/compression, encryption, storage/publication and owner services under the existing Live TV feature boundary. Reuse the qualified connection driver and fixed-fixture TLS path. Preserve the old transport/network-fixture contracts. No source networking is registered at startup, no scheduler executor is attached and no UI/read/playback endpoint is added.

Acceptance requires:

1. Genuine proxy-format fixtures, reverse-proxy prefixes, URI/query rotation and remove/readd preserve channel keys; different assignments and recreated UUIDs do not. Shared guide IDs never merge channels. Unsupported/duplicate identities and malformed/partial playlists preserve last-good data; valid empties replace it.
2. UTF-8/BOM/language/text/CDATA/entity handling, one-root/completion checks, malformed attributes/comments/names, DTD/XXE/entity-bomb and encoding refusals, depth/event/record/text/output limits. No external file, image, stream, XML or guide reference is opened.
3. Plain/gzip guide parity, member and compressed/decoded byte bounds, complete EOF/footer checks, truncation/corruption/trailing-data refusal and cooperative cancellation. Verify valid data through existing flate2 consumers affected by the patch; retain 0.41 NFO/plist API compatibility.
4. Reliable numeric/omitted-zone UTC handling, sparse/unknown/missing/invalid time evidence, language preservation, duplicate coalescing, conflict/overlap/clump flags and raw rating systems. No manufactured programme end, age classification or Kids grant.
5. Real owned PostgreSQL persistence, contextual encryption/substitution/index-binding refusal, composite-key isolation, atomic old/new visibility, failed-publication rollback, source-generation and latest-attempt races, pointer/count consistency, bounded retention and assignment cascade/reset.
6. Cancellation before connect, during HTTP/TLS/read and actual worker execution, abandoned callers, lease retention through worker/database cleanup, closed owner admission and shutdown. Qualify pre-commit rollback and indeterminate/reconciliation classification without claiming rollback from a lost acknowledgment.
7. Input-budget boundary and representative scale/resource checks, secret-free trace/audit/error capture and zero redirect/proxy/nested-resource trap connections. Reuse immutable local PostgreSQL/fixed TLS fixtures; no real household credentials or Dispatcharr instance is required for this controlled slice.
8. Serial memory-admitted builds/tests, existing destination/assignment/owner regressions, library/workflow tests, formatting/copyright and strict/all-target lint accounting. Preserve reserves, job/CPU/PID limits and existing diagnostic baselines. Record exact qualified source, passes, failed attempts and remaining limits, then commit/push all changes on main without a release.

## Publication source clarification

Installed SQLx 0.9 transaction commit/rollback consume the transaction, and Drop only queues rollback. PostgreSQL statement and lock limits are not whole-transaction bounds; disconnecting during commit can still leave a committed transaction. The retained connection, whole-publication budget and explicit indeterminate state above follow those facts. [SQLx transaction documentation](https://docs.rs/sqlx-core/latest/sqlx_core/transaction/struct.Transaction.html), [PostgreSQL timeouts](https://www.postgresql.org/docs/18/runtime-config-client.html), [PostgreSQL protocol flow](https://www.postgresql.org/docs/18/protocol-flow.html).

## Random open PR applicability

Fresh MCP GitHub search on October 25 returned one open PR. A uniform random selection from that complete one-item set selected [PR #5](https://github.com/cloudbyday90/Duskcue/pull/5), immutable head `c018d77b7f09543e5960a571dc43e2f061cedf0a`, not merged. It proposes PostCSS 8.5.23 and Nano ID 3.3.16. Both current lockfiles resolve PostCSS 8.5.30 and Nano ID 3.3.20. Its change remains superseded; no local downgrade, merge, closure or comment is appropriate.

## Checkpoint outcome and next item

Official-source research, installed-code constraints and all nine product confirmations are complete. The exact input/resource/storage/publication/ownership contract is now reviewable above. Shared-understanding confirmation of that complete contract precedes source work. There is no new runtime test result at this contract checkpoint.

Concrete-contract checkpoint verification: whitespace checks and **589 local target links across five changed Markdown documents** pass, excluding inline/fenced code examples. This checks target existence, not remote availability or Markdown anchors. No heavy build/test or dependency operation ran, and no source file changed. This documentation-only checkpoint stays on main without a release; source implementation awaits the pending final scope confirmation.

After import/storage qualification, the next item is retained root ownership and controlled administrator-triggered refresh, followed by separately qualified scheduling and profile-scoped catalog reads. Production playback, shared viewer admission, Kids programme enforcement, image/stream fetching and actual Dispatcharr/web/Tauri journeys remain their own milestones.

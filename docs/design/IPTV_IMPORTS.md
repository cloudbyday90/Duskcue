# Bounded IPTV imports and last-good snapshots

## Status and scope

Research checkpoint, October 25, 2026, against `22422191ee285773392e7f43b2e0dd53bdc65ca6` on `main`. This document records source findings, confirmed boundaries and proposed implementation choices. No import implementation, schema migration, dependency update, build or runtime qualification is claimed at this checkpoint. No release or PR merge.

[IPTV](IPTV.md) owns player behavior and milestones. [Saved-feed destination authority](../security/IPTV_DESTINATION_AUTHORITY.md), qualified at source `30511af8e2347426446b3c5ff3db08ad863e01ed`, owns encrypted source approval and controlled HTTP/TLS. [Assignment/delegation](IPTV_ASSIGNMENT.md) and [owner security](../security/HOUSEHOLD_OWNER_SECURITY.md) retain their existing boundaries. This document owns bounded import semantics, identity, publication, retention and the next qualification contract.

The user confirmed:

- Complete parsing, persisted last-good snapshots and controlled qualification behind the default-off feature first. Production refresh endpoints, scheduling and client catalog UI follow root-lifecycle integration.
- Failed or malformed refreshes retain last-good data with timestamp/status. Assignment deletion/replacement removes its catalog; disabled or revoked authority makes saved data unavailable for viewing.
- Treat both feeds as untrusted metadata. Fetch no stream/logo/playlist/XML reference and keep Kids disabled until programme-rating policy is qualified separately.

The next interview round is pending: the verified Dispatcharr proxy-UUID identity lane, atomic publication of the feed pair and explicit preservation of incomplete/ambiguous XMLTV evidence. Proposed choices below are recommendations, not confirmed implementation authority. The repository review requires shared-understanding confirmation before source changes.

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
| Proxy UUID identity scoped to assignment | Stable across ordinary export-query changes; independent of shared guide keys; no heuristic channel merges. | First lane excludes generic, XC and direct-provider formats; exporter changes need qualification. | Pending product confirmation. |
| Names, guide IDs or full-URL identity | Broad initial acceptance. | Names/guide IDs collide; credential-bearing paths/queries change and may leak through identity/debug fields. | Do not use as silent fallback. |
| Atomic playlist/guide publication | Readers never observe a mixed partial refresh; failed input preserves a coherent last-good pair. | A malformed guide delays an otherwise valid lineup update. | Pending product confirmation. |
| Independent feed publication | Guide and channel refreshes can advance independently. | Requires explicit cross-version matching, stale-reference and permission semantics. | Defer unless selected. |
| Strict UTF-8, no DTD/custom resources | Compatible with the inspected exporter; smaller parser/trust surface. | Some valid XMLTV from other producers needs transcoding or inert-DOCTYPE compatibility later. | Propose this bounded first lane. |
| Per-record encrypted metadata/resources | Derived URLs and untrusted text avoid plaintext/audit exposure; reuse contextual AES-GCM. | Adds encoding, encryption and aggregate-record ceilings. | Recommended storage boundary. |
| One retained last-good snapshot | Bounded history; refresh failure changes status, not valid catalog data. | No historical rollback UI. | Recommended; preserve stable channel identity as assignment plus upstream UUID, independently of snapshot-row identity. |
| Owned bounded parse/publication pipeline | Cancellation and capacity cover work after fetch completion; source changes cannot publish stale data. | Requires new receipt, parser ownership and transaction qualification. | Required before production refresh integration. |

## Recommended stack

Retain Rust/Axum, PostgreSQL, existing owner-security transactions, contextual ring AES-GCM, URL parsing and the qualified Hyper/Tokio-Rustls destination boundary. Add a narrowly featured, optional renamed quick-xml 0.42 dependency for the new importer while retaining existing 0.41 consumers. Prefer the current flate2 1.1.10 patch for a qualified gzip lane, documenting its miniz backend change and checking other existing compression consumers. These are proposals; no dependency has been changed at this checkpoint.

Use separate M3U parsing, XMLTV structure/fields, timestamp evidence, compression/limits, secret encoding, source receipt, publication and operation-owner services. Preserve the Live TV domain's existing five-file HTTP boundary. Introduce no global singleton, Dispatcharr management client, scheduler secret payload, parser network resolver or production endpoint.

## Proposed contract areas to finalize

The following areas need a concrete shared-understanding contract after the pending identity/publication/evidence answers:

1. **Input lane:** a version-qualified Dispatcharr proxy-UUID M3U dialect, bounded UTF-8 XMLTV, and a deliberate distinction between gzip resource payloads and HTTP content coding. No source URL inference, nested fetches, automatic redirects/proxies/DNS fallback or server-certificate bypass.
2. **Structure and limits:** validate full UTF-8 and XML 1.0 characters; check one complete root, depth, event/attribute/text/record sizes and aggregate output. Handle text/CDATA/predefined/numeric references without losing spaces or enabling custom entity resolution. Set realistic finite limits independently from the previous 64 KiB network fixture ceiling.
3. **Guide evidence:** preserve supported raw time precision/zone uncertainty and rating systems; normalize reliable timestamps to UTC without using host-local defaults. Exact duplicate handling must preserve distinct channel identities; conflicting/clumped/overlapping evidence must not become an invented safe programme.
4. **Identity and storage:** channel identity is assignment plus upstream UUID, while encrypted snapshot entries have their own record identity. Store guide-match keys and descriptive text inside bounded encrypted metadata; encrypt stream/logo references separately under household/assignment/snapshot/record/purpose context. No raw source content enters audit/errors or generic scheduler JSON.
5. **Receipt/publication:** admit both saved feeds at one source generation, retain an opaque receipt through parse, and recheck it under existing locks in the publication transaction. A durable attempt identity must prevent an older completed refresh from overwriting a newer attempt. Define commit-outcome handling separately from pre-commit parsing/network failure.
6. **Retention and visibility:** retain one last-good pair on failure, remove it on assignment deletion/replacement, and prevent authority changes from reviving old derived credentials without a fresh qualified import. A structurally valid empty lineup needs explicit documented treatment; malformed/error pages cannot masquerade as empty success.
7. **Ownership:** retain bounded import admission through networking, cooperative parser completion, publication and cleanup. Joining a cancelled caller future is not proof that a blocking parser stopped. Keep root/scheduler integration outside this confirmed slice.
8. **Qualification:** adversarial format/encoding/compression/time fixtures; real encrypted PostgreSQL publication, rollback, source/attempt races, replacement/deletion, bounded retention, opaque output and log capture; connected-peer cancellation and parser abandonment/shutdown. Run all heavy work serially through the existing memory guard, followed by appropriate assignment/owner/library/lint regressions.

These recommendations are not yet a confirmed implementation contract. After the remaining decisions, document the exact limits/interfaces/schema, acceptance criteria and deferred work here, obtain the required shared-understanding confirmation, then implement and qualify on main under the user's existing commit/push authorization.

## Random open PR applicability

Fresh MCP GitHub search on October 25 returned one open PR. A uniform random selection from that complete one-item set selected [PR #5](https://github.com/cloudbyday90/Duskcue/pull/5), immutable head `c018d77b7f09543e5960a571dc43e2f061cedf0a`, not merged. It proposes PostCSS 8.5.23 and Nano ID 3.3.16. Both current lockfiles resolve PostCSS 8.5.30 and Nano ID 3.3.20. Its change remains superseded; no local downgrade, merge, closure or comment is appropriate.

## Checkpoint outcome and next item

Official-source research, installed-code constraints, the initial three product confirmations and this reviewable recommendation document are complete. Remaining product answers and complete-contract confirmation precede source work. There is no new runtime test result at this research checkpoint.

Checkpoint verification: whitespace checks and **589 local target links across five changed Markdown documents** pass, excluding inline/fenced code examples. This checks target existence, not remote availability or Markdown anchors. No heavy build/test or dependency operation ran, and no source file changed. The checkout remains on main; the documentation checkpoint is committed and pushed without a release.

After import/storage qualification, the next item is retained root ownership and controlled administrator-triggered refresh, followed by separately qualified scheduling and profile-scoped catalog reads. Production playback, shared viewer admission, Kids programme enforcement, image/stream fetching and actual Dispatcharr/web/Tauri journeys remain their own milestones.

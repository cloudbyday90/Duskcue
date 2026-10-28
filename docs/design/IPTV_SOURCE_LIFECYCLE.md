# Local, recording and provider source lifecycle

## Status and boundary

Product requirements confirmed October 28, 2026 during [shared client migration](VOD_CLIENT_MIGRATION.md) review. These rules concern future provider VOD and recording integration; they do not implement a connector, DVR, source matcher or production playback. Existing [IPTV](IPTV.md), [Xtream](IPTV_XTREAM.md), local-library and playback permissions retain their separate boundaries.

The user's “Live TV > Recording > VOD > Local Media” describes progression toward a final local copy, not highest-first playback priority. A normal movie/episode Play action prefers an eligible ready local library source, then an eligible completed recording, then an explicitly owner-approved provider VOD. Watch live remains a separate explicit action at the current broadcast point.

## Confirmed product rules

| Decision | Behavior |
|---|---|
| Local priority | The household owner can opt a specific movie/show into provider fallback. Once its matching local media is ready, new Play actions use local media and the provider fallback expires. |
| Active viewers | Existing authorized provider playback may finish. A newly ready local copy does not silently retarget its output, file, timeline or history. Actual permission/source revocation still takes effect. |
| TV granularity | Show-level opt-in covers missing episodes. Each provider episode is replaced independently when its matching local episode becomes ready; remaining missing episodes can retain their approved fallback. |
| Provider reactivation | An expired provider fallback does not reactivate when local media disappears or becomes unavailable. A fresh household-owner opt-in is required, within administrator/profile permission. |
| Source order | Ready Local → completed Recording → owner-approved provider VOD for normal on-demand Play. Watch live is separate. The order never grants access to a source the viewer cannot use. |
| Recording replacement | Local replacement expires the recording fallback choice. It does not itself delete recording files/history; separate recording-retention policy owns those decisions. |
| Independent controls | Administrator provider Movies and Series switches remain independent/default off. They do not disable local libraries, recordings or Live TV, and an owner opt-in cannot bypass them. |

“Expire” here is a source-selection/admission transition. It is not upstream deletion, an automatic recording deletion instruction, or a replacement of an already admitted output URL. Manual recording access and later reactivation/retention details require the future DVR contract.

## Existing repository evidence

Read-only inspection against `903e670` found no recording domain/provenance, unified cross-catalog title resolver, provider/local matcher, per-title opt-in, fallback expiry or local-ready event. [Core media schema](../../server/migrations/20260530030000_create_core_media_tables.sql) owns items within a library; external-ID indexes are not unique and the same title can have separate item IDs across libraries. Series/season/episode relationships exist, but a show itself is not the playable movie/episode unit.

[Media matching](../../server/src/services/media_matching.rs) distinguishes explicit sidecar/NFO/provider-ID identification from filename-based automatic matching. That is not a qualified matcher for untrusted provider metadata. Edition, episode numbering, duplicate libraries and ambiguous matches must be resolved explicitly before a readiness event can expire another source.

The scanner's stored healthy flag is not a durable acquisition/readiness state. [Current source checks](../../server/src/domains/playback/source_freshness.rs) and [file identity](../../server/src/domains/playback/direct.rs) verify current filesystem and probe attribution for the selected file; the identity includes a metadata hash, not a full-content proof. Future local-ready publication must establish trusted identity, accessible stable media and current playback eligibility. Discovering a filename, an incomplete download or a matching display title is insufficient.

The [qualified reconfiguration contract](VOD_RECONFIGURATION.md) remains within one admitted item/file. Provider/recording-to-local transitions cannot masquerade as a same-file seek or recovery. New source/file selection requires a new Start after the old viewer's release, unless a separately reviewed future handoff contract establishes timeline/edition/track equivalence.

## Recommended design direction

Keep canonical title/episode identity and household source preference separate from immutable source-specific files and admitted playback. Model an owner fallback grant, its eligible source, the verified local-ready observation and an irreversible superseded/expired outcome. Retain enough bounded evidence to explain the transition to authorized management users without exposing inaccessible libraries or provider credentials to viewers.

Resolve source preference on the backend under current household/profile/library/rating/source permissions. Recheck it when admitting new playback. A local-ready event changes future selection; it must not rewrite active source attribution or invalidate an authorized viewer merely to promote a preferred source. Duplicate/stale events must not re-enable fallback, double-expire grants or delete data.

Use focused identity, readiness, preference, eligibility and transition services with transactional publication and UUIDv7 Duskcue records. Reuse the current Rust/SQLx/PostgreSQL and owned playback stack. No frontend provider credentials, title/year-only takeover, implicit recording classification or cross-file resume-time equivalence is proposed.

## Future design frontier and qualification

Before implementation, define the trusted cross-catalog mapping/edition/episode identity, local-ready publication and revocation lifecycle, profile-versus-household visibility, owner-proof/API controls, recording provenance, manual recording access/reactivation, whether recording arrival before local readiness merely outranks or expires provider fallback, source-specific import/storage/network authority and retention schema. These are future domain decisions; this document confirms the product direction, not their unreviewed APIs.

Qualification must cover duplicate/ambiguous identities, partial downloads, changed/missing files, inaccessible libraries, partial series, repeated/out-of-order readiness events, owner/admin/profile restrictions, no automatic provider reactivation, recording-retention independence, new-start priority and uninterrupted authorized active playback. Existing playback tests do not qualify this future lifecycle.

## Outcome

L1–L5 are recorded and linked to the client migration. No application code, provider/recording ingestion, playback enablement, deletion, dependency update or new runtime result is claimed by this documentation checkpoint. Supplied private IPTV feeds are unnecessary and remain ignored.

# Xtream Codes sources

## Status and confirmed scope

Research checkpoint, October 26, 2026, against `34de1637be7f6a4efdac7e0c5f3e394e2f0491f9`. The user requested Xtream Codes-compatible access to Dispatcharr or a direct IPTV provider, and confirmed **Live TV and EPG first**. Provider credentials and requests stay on the backend under existing household administrator controls. Movies/series and catch-up remain later increments.

The user subsequently clarified that Dispatcharr is preferred and approved continuing without Xtream. The active increment remains the existing Dispatcharr M3U/XMLTV catalog UI. Xtream is an optional deferred connector, not a prerequisite for using Dispatcharr. The Live TV/EPG scope above remains the boundary if that connector is resumed.

There is currently no Xtream connector in Duskcue. This document records facts, alternatives and a proposed design frontier; it does not claim source validation, provider login, import, playback or arbitrary-provider compatibility. The separately reviewed [client catalog](IPTV_CLIENT_CATALOG.md) should remain source-neutral and consume the existing [catalog APIs](IPTV_CATALOG.md).

[IPTV](IPTV.md) owns the player boundary. [Assignment](IPTV_ASSIGNMENT.md), [owner security](../security/HOUSEHOLD_OWNER_SECURITY.md), [destination authority](../security/IPTV_DESTINATION_AUTHORITY.md), [imports](IPTV_IMPORTS.md) and [manual refresh](IPTV_REFRESH.md) remain the foundations. Direct provider access adds a player input; it does not imply provider-account administration or channel aggregation.

## Official evidence

URLs were discovered and retrieved through MCP on October 26. GitHub MCP release metadata identifies **Dispatcharr v0.33.0**, published October 24, as the latest non-prerelease in the retrieved release collection. This is a version-qualified reference implementation, not a universal Xtream protocol specification.

| Primary source | Finding and consequence |
|---|---|
| [Dispatcharr XC output documentation](https://dispatcharr.github.io/Dispatcharr-Docs/troubleshooting/) | XC clients use a server URL, the configured user's username and a separately configured XC password. The documentation also exposes authenticated playlist/XMLTV exports. A display/profile label is not authentication. |
| [Dispatcharr v0.33.0](https://github.com/Dispatcharr/Dispatcharr/releases/tag/v0.33.0), [tagged output implementation](https://github.com/Dispatcharr/Dispatcharr/blob/v0.33.0/apps/output/views.py) | The API provides account information, live categories, live streams and guide operations. Live entries use numeric channel IDs; guide matching is a separate field. They cannot substitute for the current M3U adapter's proxy UUID. Other actions expose movies/series, supporting the need for the confirmed scope boundary. |
| [Dispatcharr M3U/EPG manager](https://dispatcharr.github.io/Dispatcharr-Docs/m3u-epg-manager/) | Dispatcharr distinguishes its own upstream XC imports and shared provider/server limits. Duskcue connecting to Dispatcharr's XC output is a separate role from managing Dispatcharr's provider accounts. |
| [OWASP SSRF prevention](https://cheatsheetseries.owasp.org/cheatsheets/Server_Side_Request_Forgery_Prevention_Cheat_Sheet.html) | Reuse explicit origin/IP authorization and controlled connections. An API response, advertised server address or stream/logo URL must not become a new networking permit. |
| [OWASP unsafe API consumption](https://api-security.owasp.org/editions/2023/en/0xaa-unsafe-consumption-of-apis/) | Treat provider responses as untrusted: bounded bodies/counts/types, safe errors, validated identities and controlled publication. A successful login is not permission to trust every returned property or destination. |
| [OWASP REST security](https://cheatsheetseries.owasp.org/cheatsheets/REST_Security_Cheat_Sheet.html) | Protect credentials in transport and diagnostics. XC interoperability may require path/query credentials upstream; keep those constructed requests server-side and omit full URLs, credential fields and raw provider errors from Duskcue responses/logs. Upstream logging behavior cannot be certified by Duskcue. |

The inspected tagged output file has Git blob SHA `d81d852e744d6a78538dd34fd5abd01257c98890`. No direct provider credentials, live account or real-provider behavior were tested. Dispatcharr behavior alone does not establish every provider's optional fields, ID types, EPG mapping, authentication quirks or transport support.

## Current implementation constraints

- Household assignment stores exactly one encrypted playlist URL and one encrypted guide URL. Its strict DTOs and schema have no connector discriminator or structured XC credentials.
- Current full-URL feeds can carry path/query secrets, but this does not implement login/account discovery, derived API requests or JSON listing parsing.
- Encryption separates URL and destination purposes and binds household/assignment ownership. New credential fields need their own typed, authenticated storage contexts; they must not masquerade as existing URL records.
- Saved authority approves specific origins and literal IPs, preserving normal TLS identity verification. It grants no DNS/proxy/redirect fallback and no authority over returned stream/logo/CDN locations.
- The current M3U importer accepts only qualified Dispatcharr proxy-UUID identities. Its private key is an assignment-scoped hash of that UUID; `tvg-id` is only a guide-match key. XC/direct numeric IDs are explicitly excluded from that adapter.
- UUIDv7 snapshot-entry IDs and encrypted catalog storage are already independent of the upstream UUID format. A new qualified producer can reuse that boundary without changing the client protocol.
- Imports currently parse one M3U/XMLTV pair and atomically publish a fenced last-good snapshot. A new JSON/API lane needs its own bounded acquisition/normalization path and must preserve retained task/database ownership, source-generation checks and atomic publication.
- Rotation, approval/revocation and account state advance source generation. Existing data cannot revive until a new matching-generation import. Source replacement creates a new assignment identity and resets owner choices; this distinction must remain explicit for connector/account changes.

## Alternatives and recommended stack

| Choice | Pros | Cons | Recommendation |
|---|---|---|---|
| Backend XC adapter into common catalog snapshots | One household permission model and UI; provider secrets remain server-side; deliberate identity/egress qualification. | Adds typed credentials, API parsing and import lifecycle work. | Recommended. |
| Generate playlist/XMLTV URLs from credentials only | Reuses much of the existing feed engine. | Still needs a qualified direct/XC identity adapter and does not establish account/API behavior. Must not be presented as full API support. | Possible narrow compatibility lane, not the selected architecture by assumption. |
| Let web/Tauri call providers directly | Less backend integration initially. | Exposes provider credentials, duplicates authority and allows an uncontrolled network path. | Reject. |
| Replace Dispatcharr management inside Duskcue | Could centralize provider curation. | Expands far beyond a player; conflicts with the existing product boundary. | Out of scope. |
| Live TV/EPG first | Focuses qualification on current catalog and permissions. | Provider movies/series/catch-up unavailable in this increment. | Confirmed. |

Recommended foundation: the existing Rust/Axum, SQLx/PostgreSQL, contextual AES-GCM and owned Hyper/Tokio-Rustls stack. Keep protocol configuration, credential protection, destination authority, acquisition, normalization, identity and publication services modular. Preserve the common catalog DTOs, UUIDv7 Duskcue identities and request-owned source-neutral client. Reuse locked dependencies where they suffice.

## Proposed design frontier

The complete backend contract is not yet confirmed. Resolve the following before source changes:

1. Preserve one active household assignment with an explicit source kind: existing Dispatcharr feed pair or Xtream-compatible input. Additional-provider aggregation is a separate feature. Define connector/account changes as explicit replacement, distinct from deliberate same-account password rotation.
2. Use separately encrypted server-base URL, username and password fields with fixed input limits, versioned purpose binding, write-only responses, current administrator permission/fresh proof and conditional updates. Keep household-owner delegation confined to the existing controls.
3. Authorize a fixed metadata operation set against an explicitly approved origin/IP set, retaining reverse-proxy prefixes, normal TLS checks, cancellation and secret-safe diagnostics. Do not trust advertised server addresses, returned direct-source fields or redirects as authority.
4. Prefer bounded account/category/live-listing requests plus bulk guide ingestion over unbounded per-channel guide fanout. Exact endpoint behavior, counts, byte/time budgets, optional fields and XMLTV/API guide association still need a concrete qualified contract.
5. Scope native channel identity to the assignment and connector/account context. Provider IDs remain private; Duskcue rows remain UUIDv7. Do not label a provider's numeric ID as a UUIDv7 or use an EPG match key as channel identity. Preserve exact identifier representation without JavaScript-number conversion.
6. Normalize only allowlisted metadata into coherent encrypted last-good snapshots. Authenticate receipt/index bindings and recheck current source/initiating authority before publication. Retain old data on failed refresh but refuse stale generations. Never infer programme ends or rating meanings.
7. Define protected stream-reference storage without enabling playback or silently granting networking to returned destinations. The current client milestone is read-only; real direct-provider transport and shared playback admission require their existing lifecycle gates.
8. Qualify migration/backwards compatibility, actual HTTP/TLS, credential encoding, malformed/oversized JSON, duplicate/colliding identities, guide matching, revocation/publication/cancellation/commit faults and secret-free output. Reference-implementation fixtures and real-provider qualification must be reported separately.

Sequencing is settled: continue the Dispatcharr-first client UI and defer Xtream. The common catalog boundary permits a future connector without coupling the UI to provider credentials. Resume the backend design frontier only when Xtream is explicitly prioritized; do not expand this checkpoint into provider management, VOD or playback.

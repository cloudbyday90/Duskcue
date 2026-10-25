# IPTV saved-feed destination authority

## Status and ownership

Research and design checkpoint, October 25, 2026, against `26a4a178adb81ee7e2db767fda5371d1e6cea85e` on `main`. The user confirmed explicit IP pins, public-root TLS verification and controlled qualification behind the default-off `playback-qualification` feature. Confirmation of shared understanding of the complete contract below is pending. This checkpoint changes documentation only; it does not implement or qualify networking. No release or PR merge.

[IPTV](../design/IPTV.md) owns the player milestones. [Assignment and delegation](../design/IPTV_ASSIGNMENT.md) owns the already qualified encrypted URL pair, administrator authority and owner controls. [Owner security](HOUSEHOLD_OWNER_SECURITY.md) owns fresh proof and grants. This document owns destination approval, stale-source fencing and the first saved-feed connection qualification. It does not change the settled household, Kids, stream-cap or initial playback decisions.

## Findings from the existing source

The saved assignment has encrypted playlist/guide URLs and a general revision, but no approved destinations or network generation. Reads deliberately return no URL, hostname, path, query, ciphertext or partial secret. Rotation currently accepts a different origin while preserving assignment identity; source approval therefore needs explicit origin comparison rather than assuming every rotation is the same destination.

The existing experimental MPEG-TS source accepts literal IPs and has redirect/proxy restrictions. Its evidence includes real HTTP and cancellation during a stalled TLS handshake, not a successful hostname-authenticated HTTPS fetch. It is not a finite-feed importer. Its selected blocked-address set is not a complete special-address classifier. Preserve that qualified experiment and implement the saved-feed boundary separately.

Inspected installed reqwest 0.12.28 uses its default resolver for names without an override. Its system lookup can start blocking work; dropping a caller is insufficient evidence that the lookup stopped. Its client also delegates connection driving to executor tasks without exposing a per-request join handle. These are ownership costs for this particular qualification, not a general claim that reqwest is unsuitable for HTTP.

The locked stack already contains Hyper 1.10.1, hyper-util 0.1.20, Rustls 0.23.46, Tokio-Rustls 0.26.4, webpki-roots 1.0.7 and Tokio 1.52.3. Latest online API documentation can describe newer releases; implementation must use and inspect the installed locked interfaces. The local registry did not contain the exact locked Rustls source during this research, so no exact-version Rustls implementation review is claimed.

## Official-source research

URLs below were discovered with MCP search and retrieved on October 25, 2026. Local dependency inspection complements these primary sources. Recommendations and numerical qualification limits are Duskcue design decisions, not requirements imposed by the cited standards.

| Source | Relevant finding | Application and limitation |
|---|---|---|
| [OWASP SSRF prevention](https://cheatsheetseries.owasp.org/cheatsheets/Server_Side_Request_Forgery_Prevention_Cheat_Sheet.html) | Validate destinations with an allowlist and prevent redirects from bypassing it; consider IPv4, IPv6 and DNS changes. | Bind approval to a saved feed and exact origin, connect to a literal approved address, and never follow redirects. Explicit LAN approval is necessary for a self-hosted Dispatcharr deployment. |
| [WHATWG URL](https://url.spec.whatwg.org/), [HTTP semantics, RFC 9110](https://www.rfc-editor.org/rfc/rfc9110.html) | HTTP origin identity includes scheme, host and effective port. | Use the existing URL parser and canonical origin equality. A shared hostname alone cannot authorize another port or downgrade HTTPS to HTTP. |
| [TLS service identity, RFC 9525](https://www.rfc-editor.org/rfc/rfc9525.html) | Reference identity comes from the intended service, not an intermediate resolution result. DNS names and literal IPs have distinct certificate matching rules. | Keep the saved hostname for SNI and certificate verification when connecting to a pinned IP. Literal-IP URLs require matching IP identities. |
| [Rustls](https://docs.rs/rustls/latest/rustls/), [webpki-roots](https://docs.rs/webpki-roots/latest/webpki_roots/index.html) | Rustls supports an explicit root store; webpki-roots compiles in Mozilla trust anchors. Compiled roots require software updates to refresh. | Reuse the installed root distribution and existing ring provider rather than introducing another crypto backend or ambient OS trust. Root updates remain dependency maintenance; this qualification does not establish live certificate-revocation checking. |
| [Tokio blocking tasks](https://docs.rs/tokio/latest/tokio/task/fn.spawn_blocking.html) | A blocking task that has started cannot be aborted through its join handle. | Avoid system DNS entirely in this increment. A timeout around a system lookup would not establish owned cancellation. |
| [Hyper client guide](https://hyper.rs/guides/1/client/basic/), [HTTP/1 connection](https://docs.rs/hyper/latest/hyper/client/conn/http1/struct.Connection.html) | The handshake returns request and connection objects; the connection must be polled. Dropping the connection closes its underlying IO. | Poll the connection and request/body work within the same tracked operation. Retain ownership through cancellation instead of discarding a spawned driver handle. |
| [HTTP/1.1, RFC 9112](https://www.rfc-editor.org/rfc/rfc9112.html) | Invalid or ambiguous message framing requires careful handling; incomplete bodies are not successful messages. | Use Hyper's parser, reject conflicting transfer/length framing, bound headers and body, and fail on truncation. Do not write an HTTP parser. |
| [IANA IPv4](https://www.iana.org/assignments/iana-ipv4-special-registry), [IANA IPv6](https://www.iana.org/assignments/iana-ipv6-special-registry) | Special-purpose categories differ in destination and forwarding properties; registry entries do not guarantee routability. | Maintain an explicit conservative address policy. Deliberately allowed local networks are application policy, not a claim that those addresses are globally routable. |
| [W3C security/privacy questionnaire](https://www.w3.org/TR/security-privacy-questionnaire/) | Review ambient authority, information exposure and cross-origin effects. This is review guidance, not an application certification. | Do not forward browser credentials or leak secret origins in diagnostics. A feed's nested URLs receive no inherited authority. Browser same-origin controls do not constrain server egress. |
| [W3C WCAG 2.2](https://www.w3.org/TR/WCAG22/) | Authentication and error/status interactions need accessible client behavior. | Preserve stable refusal codes for a later accessible administration/unlock UI. This backend increment makes no UI conformance claim. |

## Alternatives and recommendation stack

| Option | Pros | Cons | Decision |
|---|---|---|---|
| Administrator IP pins | No DNS rebinding or unresolved-name fallback; explicit local-network consent; deterministic tests. | Address changes need administrator updates; no automatic discovery or failover. | First increment, confirmed by the user. |
| Owned asynchronous DNS | Easier deployment when addresses change; could qualify dynamic discovery. | Adds resolver policy, background-task ownership, cancellation and answer-validation work. | Separate later increment. |
| Reqwest with overrides and a deny-by-default resolver | Reuses a high-level client already in the project. | Requires proving fallback refusal and lifecycle ownership through an executor-managed connection driver. | Retain for existing consumers; do not base this controlled feed operation on it. |
| Direct Hyper HTTP/1 over owned Tokio TCP and Tokio-Rustls | No resolver, proxy, pool or redirect machinery; explicit connection lifetime; existing protocol parser and TLS stack. | More request/body/deadline code and more ownership qualification. | Recommended for this bounded saved-feed operation. |
| Existing public WebPKI roots | Existing trusted-root distribution and ordinary certificate/name validation. | Private/self-signed deployments need a later explicit trust-store design. | First increment, confirmed by the user; no insecure verifier option. |

Retain Rust/Axum, PostgreSQL, contextual ring AES-GCM, the existing owner-security transaction services, URL parsing, Tokio and Tokio-Rustls. Add three narrowly featured direct dependency declarations for already locked packages: `hyper` with `client,http1`, `hyper-util` with `tokio`, and `webpki-roots`. This is a manifest/dependency-edge change, not a claim of zero dependency changes; no package-version upgrade is intended. Preserve the existing Rust/Cargo 1.99.1 local toolchain and the separately pinned Docker compiler. Use small policy, persistence, admission, connection, body and operation-owner modules; no global client singleton.

## Concrete contract for shared-understanding confirmation

### Administrator approval and confidentiality

Extend the existing administrator assignment PATCH resource with tagged `approve_destination` and `revoke_destination` operations. Approval specifies `feed` (`playlist` or `guide`), `expected_origin`, and between one and eight distinct literal `addresses`. Revocation specifies the feed. Both operations require current `can_manage_users`, fresh proof on the administrator's own account and the existing strong `If-Match` condition, checked under the shared account/assignment locks. Household-owner delegations cannot approve destinations.

The expected origin must equal the origin of the decrypted current saved URL. Accept HTTP/HTTPS origins only, with a host and nonzero effective port, no userinfo, no query/fragment and no path other than the optional root slash. Continue existing strict URL length/whitespace/control/backslash validation. Address entries are bare IP literals, never hostnames, CIDRs, port overrides, zone identifiers or wildcards. For a literal-IP feed, pins must agree with that literal; hostname feeds can use deliberately approved local or public addresses. The feed's effective port supplies the connection port.

Store origin and normalized address approval encrypted with a versioned payload and distinct household/assignment/feed/purpose context. Do not reuse the URL encryption context for approval data. Bound the serialized payload and reject unknown fields, unsupported versions, duplicates, invalid envelopes, changed contexts and mismatched origins. A malformed approval fails closed; reads must not decrypt it merely to display metadata.

Read responses may expose per-feed approval presence, approved-address count and a source generation. They must not expose the origin, address values, URL, path, query, tokens, ciphertext or masked secrets. Treat caller-controlled origin/address strings as sensitive throughout errors and audit. Preserve fixed audit events, no-store responses, existing conditional-write refusals and transactional grant revocation. No network call occurs in an administrator write.

### Address policy

Explicit pins allow deliberate RFC 1918, loopback, shared-address-space and IPv6 unique-local destinations, alongside ordinary public unicast. Normalize IPv4-mapped IPv6 addresses for policy and peer comparison, while retaining the URL's original canonical host for origin/TLS identity. Approval is an egress decision made by an authorized administrator; it is not evidence that a service is Dispatcharr or a substitute for an operator firewall.

Hard refusal remains even for explicitly submitted pins:

- IPv4: `0.0.0.0/8`, `169.254.0.0/16`, `224.0.0.0/4`, `240.0.0.0/4`, `192.0.0.0/24`, `192.0.2.0/24`, `192.88.99.0/24`, `198.18.0.0/15`, `198.51.100.0/24`, `203.0.113.0/24`, plus the existing special-service exclusions `100.100.100.200` and `168.63.129.16`.
- IPv6: permit only loopback, unique-local and `2000::/3` public-unicast candidates; exclude `2001::/23`, `2001:db8::/32`, `2002::/16`, `3fff::/20` and the existing special-service exclusion `fd00:ec2::254`. All other ranges, including unspecified, link-local, multicast and translation prefixes, are refused.

This deliberately conservative list can reject some usable special services. It is not an exhaustive cloud-metadata inventory and does not detect metadata services reached through an administrator-approved intermediary. Test the listed boundaries and mapped forms explicitly; review the policy when IANA assignments or deployment needs change.

### Rotation, invalidation and stale results

Add a positive `source_generation` separate from the general assignment revision. Administrator configuration, credential rotation and approval/revocation advance it. Owner profile/cap choices still advance the general revision but do not change source authority. Household activation/deactivation and deletion-state transitions must advance source generation transactionally, including disable/re-enable between checks. Deleting/replacing an assignment invalidates work through its identity as well as its generation. Overflow refuses the mutation.

Rotation preserves an approval only when that feed retains the same canonical origin. A changed origin clears that feed's approval. Every rotation invalidates already admitted work, even when credentials change on the same origin. If prior URL decryption fails during a repair rotation, clear approval rather than guessing. Explicit assignment replacement creates a new identity, clears destination approvals and retains the already agreed reset semantics for owner choices.

An opaque source snapshot binds household, assignment, feed and source generation to the decrypted saved URL and approved addresses. It has no debug/serialization interface and cannot be constructed from an arbitrary caller URL. Network admission requires an active, nondeleted household, enabled assignment, current generation and valid approval matching the saved origin. Check before connecting, before sending the request, during operation and before accepting a result. A mismatch or database-check failure cancels the operation; stale bytes must never become a successful result.

Periodic checks target a 200 ms interval; query latency is additional, bounded by a one-second check deadline. A request already sent cannot be unsent. Revocation therefore fences acceptance and requests bounded cancellation; it does not claim atomicity between a database commit and a remote TCP peer. No database transaction remains open for the duration of an upstream read. Shared SQLx pool lifetime is distinct from the owned upstream connection lifetime.

### Controlled HTTP/HTTPS operation

Expose the saved-source operation only behind `playback-qualification`, with no production fetch endpoint, scheduler registration or application-startup owner. A qualification caller selects a saved household/feed and an index within its approved pin set; it never supplies a replacement URL, address or TLS name. One invocation attempts one selected address. There is no fallback to another address or application retry.

Use an owned Tokio TCP stream to that literal socket address and verify the observed peer. For HTTPS, use ordinary Rustls chain, time and service-name verification with the public WebPKI roots; the saved URL hostname supplies SNI/reference identity. Literal-IP URLs use IP certificate identity. Never rewrite the URL hostname to the pin, disable verification, install OS roots or fall back to HTTP. The controlled fixture path may trust one fixed checked-in test CA only within qualification; it is not arbitrary runtime private-CA configuration.

Use Hyper HTTP/1 with one GET, an empty body, the saved path/query and a Host header derived from the saved origin. Send `Accept-Encoding: identity` and close the connection after the operation. Do not forward browser cookies, bearer tokens, authorization headers, referer or arbitrary headers. Do not consult proxy environment variables, system DNS or a resolver. Do not follow redirects, process authentication challenges, upgrade protocols, pool connections or reconnect automatically.

Accept status 200 only and absent/identity content encoding. Enforce a 32 KiB parser buffer, 64 response headers and a 64 KiB body ceiling, independently of advertised content length. Reject ambiguous length/transfer framing, unsupported transfer coding, truncation and parser/body errors. Correct HTTP chunking is permitted. The body remains opaque bounded bytes inside the qualification process; it is not parsed as M3U/XMLTV, published to clients or persisted as a last-good catalog. The 64 KiB ceiling is a fixture qualification bound, not a proposed production XMLTV limit.

Connection plus TLS handshake: five seconds. Response headers: five seconds. Body idle interval: five seconds. Whole operation: fifteen seconds, including authority checks. Bound total concurrent operations to two, with one per assignment and immediate refusal instead of an unbounded queue. These limits are independent of future household playback caps.

Poll the HTTP connection driver and request/body work within the same tracked operation, without an unjoined driver task. Handle a completed driver without dropping an already delivered complete response. Stop, caller abandonment and owner shutdown cancel the operation and close owned TCP/TLS/body resources. The owner closes admission before waiting, joins its tasks within a ten-second cleanup deadline and releases capacity only after cleanup. A failed cleanup is a failed qualification, not reported success. Qualification retains the owner until cleanup; root production shutdown wiring remains a separate enablement prerequisite.

Feed approval does not authorize stream URLs, logos, XML entities, guide references or redirects, even on another origin at an approved IP. No nested-resource fetch API or XML parser is introduced. Later imports must establish purpose-specific rules and decompression/XML protections before accessing such resources.

## Implementation and acceptance plan

1. Add a focused schema migration, encrypted approval codec and pure origin/address policy. Extend assignment operations and safe metadata while preserving the existing HTTP/service/type/error boundaries.
2. Add the separate source generation and invalidation rules, including account-state transitions. Reuse existing transaction locking, conditional requests, proof checks and audit services. Introduce no second owner or household model.
3. Add feature-gated saved-source admission, owned HTTP/TLS connection and bounded-body operation services. Keep connection ownership separate from the experimental MPEG-TS worker and finite-feed parsing.
4. Generate fixed test-only CA/leaf material without modifying machine certificate stores. Use real local HTTP/TLS peers and existing owned PostgreSQL support, with no real upstream credentials.
5. Qualify real administrator cookie/bearer routes, encrypted context substitution/corruption, cross-household/feed/origin refusal, conditional races, policy boundaries, same/changed-origin rotation, replacement, revocation and disable/re-enable. Assert no upstream activity from management writes.
6. Qualify successful hostname SNI/chain/name verification, IP identities, wrong-name/untrusted/expired certificates, wrong pins, redirect and proxy traps with zero trap hits, framing/byte/header/time limits and refusal without DNS. Exercise cancellation during connection/TLS/headers/body, abandoned callers, capacity recovery and owner shutdown with peer-observed closure. Run bounded trace capture against secret origins, paths, queries, request/response headers and fixture bodies; never weaken capture to suppress a leak.
7. Run appropriate assignment/owner regressions, library tests, formatting, copyright and compiler/lint checks through the serial memory-admitted wrapper. Preserve resource limits and existing diagnostic baselines; record failures and limitations as well as passes. Update this authority and related MDs with exact qualified source and receipts, then commit/push on `main` without a release.

No implementation, test pass, successful TLS session or production enablement is claimed at this design checkpoint. Additional independent security review and adversarial connection tests remain implementation work.

## Documentation checkpoint verification

The changed Markdown documents pass `git diff --check`. A local target-existence check, excluding inline and fenced code examples, verifies 605 links across six documents. It does not verify remote URL availability or Markdown anchor semantics. No heavy build or runtime test was needed or run for this documentation-only checkpoint. Application qualification remains the acceptance plan above.

## Random open PR applicability

MCP GitHub search on October 25 returned one open PR. A random selection from that complete one-item set selected [PR #5](https://github.com/cloudbyday90/Duskcue/pull/5), head `c018d77b7f09543e5960a571dc43e2f061cedf0a`. It proposes PostCSS 8.5.23 and Nano ID 3.3.16. Both current client lockfiles already resolve PostCSS 8.5.30 and Nano ID 3.3.20. Its change is superseded; applying it would downgrade dependencies. No local PR patch, merge, closure or comment is appropriate.

## Deferred work and next item

After this authority and its HTTP/TLS lifecycle are implemented and qualified, the next item is bounded M3U/XMLTV import: stable channel identity, independent guide matching, parsing/decompression limits, last-good snapshots and owned manual/scheduled refresh using the qualified authority. That increment needs its own concrete contract and research.

Automatic DNS, private-CA administration, production fetch APIs, nested-resource fetching, root lifecycle integration, shared playback admission, Kids programme enforcement, storage-key rotation, interactive owner unlock, client accessibility execution and real Dispatcharr/web/Tauri journeys remain separately scoped. Saved approval does not enable playback or change `network_checked: false` / `playback_available: false` metadata.

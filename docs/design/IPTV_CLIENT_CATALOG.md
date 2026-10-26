# Shared web and Tauri IPTV catalog

## Status and authority

Research checkpoint, October 26, 2026, against `34de1637be7f6a4efdac7e0c5f3e394e2f0491f9` on main. The [backend catalog](IPTV_CATALOG.md), verified source `451fb8b`, is already qualified. This document owns the next client increment; it does not claim a new screen, endpoint, native journey or accessibility result.

The user confirmed three client choices: Tonight's shell with a paginated channel browser and selected-channel programme list; fixed six-hour windows with explicit Earlier/Later/Return to current time actions and device-zone display; preferred interface-language text with all supplied variants available in disclosures. Remaining discoverability, recovery and in-memory navigation choices are under review. The user explored [Xtream Codes input](IPTV_XTREAM.md), then confirmed a preference for the existing Dispatcharr M3U/XMLTV path. Xtream is deferred as an optional future connector, with Live TV/EPG first if implemented. It is not a prerequisite for this client.

[IPTV](IPTV.md) owns product scope. [UI foundations](../branding/UI_FOUNDATIONS.md), [client accessibility](CLIENT_ACCESSIBILITY_INPUT.md), [internationalization](I18N.md), [API security](../security/API_SECURITY.md) and [guarded testing](../ci/TESTING_MEMORY.md) remain authoritative. Existing owner/admin management, Kids restrictions and playback are not expanded by a metadata screen.

## Inspected client facts

- Tauri builds the shared web client; a second catalog implementation is unnecessary. There is currently no Live TV route, navigation entry or client catalog adapter.
- The shell owns selected-profile readiness, exposes its active-profile context, cancels profile-scoped requests and remounts scoped children after local/cross-tab profile changes. Profile switches currently return to Home except for the existing media-detail preservation rule. Preserve that behavior.
- `lib/api/core.js` already uses web cookies or selected-server bearer authentication and checks profile scope after response-body parsing. Reuse it without exposing credentials to catalog components.
- Cached account capabilities and profile-list records cannot establish current Live TV entitlement. The profile response has no Live TV access field, and session revalidation does not reload current capabilities. Every catalog response remains authoritative.
- The existing `createCursorPager` ignores catalog context, keeps prior pages after append errors and exposes unbounded `loadAll()`. It cannot directly provide strict snapshot/profile fencing, bounded screen retention or explicit restart behavior.
- Both the JavaScript desktop allowlist and native incoming-route parser omit `/live-tv`. Adding a client destination requires deliberate route integration and tests, rather than accepting arbitrary paths.
- Existing gallery components provide polite status, distinct empty/error states, focusable busy controls and semantic CSS tokens. Reuse these patterns through small catalog-specific components.
- English messages belong in `messages/en.json`. Regenerate the checked-in message-availability map and retain `messageLocale` for actual English fallback language. Seven preview locales remain review-gated; this increment must not activate them.
- There is no existing guide-language selector, general raw-language-label helper or programme time-range/uncertain-time presenter. The audio/subtitle language helper accepts a narrower code set and is not a general XMLTV language validator.

## Official research, retrieved through MCP

The following URLs were discovered through MCP search and then opened on October 26. Product choices and fixed budgets are recommendations, not requirements imposed by these sources.

| Primary source | Application |
|---|---|
| [WCAG 2.2](https://www.w3.org/TR/wcag/) | Use keyboard operation, visible/unobscured focus, reflow, labels, contrast and programmatic status as acceptance criteria. A DOM/geometry test does not establish full conformance or spoken assistive-technology behavior. |
| [WAI grid pattern](https://www.w3.org/WAI/ARIA/apg/patterns/grid/), [keyboard guidance](https://www.w3.org/WAI/ARIA/apg/practices/keyboard-interface/) | An interactive grid needs explicit focus/navigation code. A list with native controls is the recommended first presentation for the current per-channel API; a multi-channel timeline would require its own design and qualification. |
| [W3C language declarations](https://www.w3.org/International/questions/qa-html-language-declarations.en) | Mark actual text language, including fallback UI strings. Preserve raw upstream labels separately; syntactic locale handling must not be described as verifying their meaning or registry membership. |
| [W3C bidirectional authoring guidance](https://www.w3.org/TR/i18n-html-tech-bidi/) | Isolate unknown-direction imported text with native bidirectional markup. This retrieved document is informative draft/note guidance, not a W3C Recommendation. |
| [ECMA-402, thirteenth edition, June 2026](https://402.ecma-international.org/13.0/) | Reuse standard locale/date/time formatting and report the resolved device time zone. Intl formatting is not an XMLTV date parser or automatic translation service. |
| [W3C time/timezone guidance](https://www.w3.org/TR/timezone/) | Distinguish instants from incomplete/floating evidence. Six elapsed hours avoid assuming a local calendar day always has twenty-four hours. The retrieved publication is a Group Draft Note, not an endorsed Recommendation. |
| [Fetch Living Standard](https://fetch.spec.whatwg.org/) | Reuse cancellation and explicitly request no-store. Keep request ownership through bounded body parsing; aborting a fetch and discarding obsolete results are complementary safeguards. |
| [Svelte effect lifecycle](https://svelte.dev/docs/svelte/$effect), [SvelteKit accessibility](https://svelte.dev/docs/kit/accessibility) | Dispose request/listener work on component teardown and provide descriptive route titles. Current online Kit docs include version-three changes; the installed version-two declarations remain the API compatibility boundary. |
| [Tauri testing](https://v2.tauri.app/develop/tests/), [webview versions](https://v2.tauri.app/reference/webview-versions/) | Mock runtime tests do not execute native webview libraries. Use the existing isolated actual-WebView2 qualification harness for desktop evidence and record the runtime version. |

The checked-in lock and installed package both identify SvelteKit **2.70.2**; the manifest permits it through `^2.69.1`. Locked Svelte is **5.56.4**. Installed navigation declarations still expose version-two options. No framework migration or dependency addition is justified by this screen.

## Alternatives and recommended stack

| Approach | Pros | Cons | Recommendation |
|---|---|---|---|
| Shared channel browser and selected-channel list | Fits current APIs; responsive native controls; one implementation for web and desktop. | Does not provide a multi-channel timeline. | Confirmed. |
| Full EPG grid now | More channels visible together. | Requires new aggregation, window coordination, keyboard model and rendering/resource qualification. | Later increment. |
| Focused catalog controller over the shared API transport | Checks context, bounds retention and rejects obsolete responses; fixed error behavior. | Adds small controller/decoder modules and adversarial tests. | Recommended. |
| Reuse the generic pager unchanged | Less initial code. | Context-blind retention, repeated cursor retry and unlimited load-all conflict with the catalog contract. | Unsuitable unchanged. |
| Explicit six-hour elapsed windows | Predictable finite requests; device-zone presentation without calendar arithmetic. | Users navigate to another window explicitly; no moving guide. | Confirmed. |
| Source-neutral Duskcue catalog API | Credentials stay on the backend; the UI can consume qualified M3U or future Xtream imports uniformly. | Xtream ingestion still needs its own backend work. | Recommended. |
| Direct browser/provider API access | Fewer server adapter steps. | Exposes provider credentials and creates a second network/authority path outside household controls. | Reject. |

Recommended stack: locked Svelte 5/SvelteKit 2, Paraglide and existing semantic CSS; focused ES-module API/decoder/controller/presentation services; native HTML lists, buttons and disclosures; Intl date/language support; the current cookie/bearer API core and Tauri shell. Retain Vitest, Playwright and the existing isolated native harness. Add no state framework, persistent cache, broker or large singleton.

## Proposed client contract under review

- A source-neutral `/live-tv` destination in the existing shell. Discoverability for selected standard profiles is proposed independently of stale cached capabilities; current GET results authorize content. Kids has no navigation entry or content surface.
- Initial channel pages do not silently select a channel. Explicit channel selection loads its guide. A chosen six-hour window survives channel selection within the current view; Return to current time captures a new instant. No polling or automatic window movement.
- Scheduled and incomplete-time views use the two existing per-channel APIs. Missing/undeclared/ambiguous guide evidence, overlap/conflict/clump flags, raw rating labels and raw times remain distinguishable. No now/next computation, inferred end, rating classification, playback action or source-management mutation.
- Proposed retention is one current channel page and one current programme page, twenty entries each, plus at most sixty-four cursor-history entries per lane. Previous/Next/First replace visible pages. There is no load-all, infinite append or persisted cursor/metadata/selection state.
- Each page must match selected profile, opaque decimal-string profile revision and current snapshot. Programme channel/view/window must also match the request. The client does not invent or increment the database revision. Obsolete responses cannot publish after cancellation, channel/view/window changes, profile/server/auth changes or disposal.
- Propose a focused bounded-JSON option inside the shared request lifetime: one MiB for success/error bodies, explicit no-store and fixed safe catalog errors. Do not use raw-response escape paths that end shared scope ownership before parsing. Existing callers retain their behavior when the option is absent.
- Propose a ten-second client operation deadline, at most one channel and one guide operation concurrently, bounded decoded shapes and no automatic retries. Server work/release/admission limits remain unchanged.
- Any catalog request failure is proposed to clear rendered catalog data. A 409 requires explicit Reload; 429/503 respect valid Retry-After before manual retry. A 401 uses current-context session revalidation without expiring a newer session or converting a network error into logout. Fixed localized messages omit raw error details, cursors and imported text.
- Keep screen state in memory only. Dispose on route exit/profile/server changes and clear on page-history restoration. Do not use local/session storage, IndexedDB, URL queries, route snapshots or service-worker storage for catalog data.
- Prefer interface-language text, then untagged or first supplied text, retaining every bounded variant in disclosures. Display imported text as text, not HTML or links. Keep raw language labels separate from a conservative well-formed locale subset used for attributes; unknown language does not inherit a false UI-language claim.
- Show localized programme instants with dates and a device-zone label. Raw/reduced-precision/missing times stay explicit. Formatting limitations must not invent a timestamp. Preserve exact server time strings for context comparison/details rather than reducing protocol precision to JavaScript milliseconds.
- Use persistent polite status, visible loading/empty/error states, native focus behavior and semantic headings. Channel selection can move focus to the guide heading; page controls retain focus; clearing removed content must restore a stable logical target without stealing focus from navigation outside the view. Responsive layout, disclosures and metadata must work at 320 CSS pixels, zoom and RTL.

These proposed details are not implementation approval. Dispatcharr-first sequencing is settled. Final shared understanding must confirm the remaining client discoverability/recovery/state proposal before implementation.

## Planned qualification and current baseline

Service units should cover body/shape limits, context mismatch, duplicate/stuck cursors, bounded page history, stale success/error/body completion, cancellation/deadline, Retry-After, current-context auth recovery, language fallback and daylight-saving/time formatting. Browser journeys should exercise actual production components with deterministic API fixtures: standard/Kids/forbidden/empty/unavailable states, strict 409 restart, profile/server changes, malformed data, mixed text/bidi, disclosures, keyboard/focus, narrow reflow, zoom, contrast and no unexpected outbound requests. Actual Tauri qualification must use a newly built isolated application identity/WebView2 directory and prove selected-origin bearer routing and cleanup. Fixtures are not live-provider or screen-reader speech qualification.

Before source changes, the unchanged relevant baseline passes **44 unit cases across five files**, plus the existing **37 Node pretest cases**, with zero skips/failures. Command: `node scripts/testing-memory/run.mjs web-unit tests/unit/api-core.test.ts tests/unit/scoped-resource.test.ts tests/unit/cursor-pager.test.ts tests/unit/desktop-auth.test.ts tests/unit/message-locale.test.ts`. Receipt: `.cache/testing-memory/2026-10-26T10-31-37-763Z-7070f8a4-18b4-49ba-a0b3-5ba5189d5406.jsonl`; exit zero, no memory stop/interruption, minimum available host memory 21,945,049,088 bytes, peak owned private bytes 439,341,056.

The unchanged web check also reports **zero errors and zero warnings**; receipt `.cache/testing-memory/2026-10-26T10-43-23-348Z-07e046a5-be2b-4bf8-b7c8-66dfbc91b86b.jsonl`. These are baseline results, not a new catalog UI pass. Run subsequent heavy checks serially through the existing guard without raising reserves, jobs, workers, fixture limits or production budgets.

## Random open PR

Fresh GitHub MCP search returns one open PR. Uniform selection from that singleton chooses [PR 5](https://github.com/cloudbyday90/Duskcue/pull/5), still open/unmerged at head `c018d77b7f09543e5960a571dc43e2f061cedf0a`. Metadata, exact filenames and both patches were inspected. Requested PostCSS 8.5.23/Nano ID 3.3.16 are superseded by 8.5.30/3.3.20 in both local lockfiles. No local patch applies; no PR mutation, merge or downgrade is performed.

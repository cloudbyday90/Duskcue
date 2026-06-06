# Name & Branding Direction

## Overview

This document defines the baseline naming and branding direction for the product so the repo can make UI, copy, icon, and marketing decisions from one consistent product identity instead of treating the application as a generic Plex alternative. It complements:

- [PROJECT.md](../../PROJECT.md) - top-level product scope, architecture, and open-question tracking
- [UI_FOUNDATIONS.md](UI_FOUNDATIONS.md) - baseline client look and feel, navigation language, and reusable UI surfaces
- [NAME_CONCEPTS.md](NAME_CONCEPTS.md) - broader exploratory set of candidate names, palettes, and visual territories
- [AUTH.md](../design/AUTH.md) - onboarding and household-user model that shapes product tone
- [SECURITY.md](../security/SECURITY.md) - local-first and optional remote-access posture that shapes trust expectations

The design goal is to choose a name and brand direction that feel trustworthy, household-friendly, and content-first for a self-hosted media platform, not technical for its own sake and not derivative of the products it is competing with.

## Goals

1. Define what kind of name fits the product and what kinds of names do not.
2. Keep the brand aligned with a personal and family self-hosted product, not an enterprise control plane.
3. Give the UI and copy system a stable tone before implementation begins.
4. Reduce the risk of settling on a name that is awkward on TV, mobile, desktop, and the web.
5. Preserve room for a final legal and trademark check without blocking current product-direction work.

## Official Research Findings (May 2026)

### Microsoft guidance for UI language and terminology

- Microsoft recommends scannable, concise, task-focused language instead of decorative or overly clever phrasing.
- Microsoft recommends plain language, culturally neutral wording, and terminology that remains consistent across the product.
- Microsoft recommends using one term for one concept throughout an experience rather than switching labels stylistically.
- Microsoft recommends sentence case and restraint in emphasis, which supports a calm, readable product voice.

### Apple guidance for hierarchy, harmony, and consistency

- Apple states that interfaces should establish clear hierarchy so important content is immediately recognizable.
- Apple emphasizes harmony between interface elements, system experiences, and devices.
- Apple emphasizes consistency with platform conventions so the product can adapt across different displays and sizes without feeling fragmented.

### Android TV guidance for content-first household products

- Android TV guidance states that TV products should put content front and center and reduce friction to finding something to watch.
- Android TV guidance highlights that TV is a communal device, which favors a household-friendly and trustworthy tone over ironic or highly technical branding.
- Android TV guidance favors legibility, clear focus, and fast recognition from a distance, which means the product name and labels should be easy to read and say quickly.

## Naming Criteria

The final product name should meet all of these criteria:

1. **Plainly pronounceable** - a new user should know how to say it on first read.
2. **Easy to hear and repeat** - it should survive spoken recommendations in a household context.
3. **Short enough for launcher surfaces** - prefer compact names, but do not force a rigid letter count if that makes the result less pronounceable or more awkward.
4. **Not clone-adjacent** - avoid sounding like Plex, Jellyfin, Emby, Kodi, Jellyseerr, Overseerr, or the Arr ecosystem.
5. **Warm but not cutesy** - the tone should feel trusted and calm, not jokey.
6. **Broad enough for the full product** - the name must fit server, web, desktop, mobile, and TV clients.
7. **Visually clean** - it should look good in sentence case, all-lowercase URLs, and launcher/icon contexts.

## Naming Anti-Patterns

Reject names that fall into these patterns:

1. **Infrastructure names** - names that sound like storage software, a transcoder, or a CI tool.
2. **Clone-signal suffixes** - `-plex`, `-flix`, `-fin`, `-arr`, and similar mimicry.
3. **Forced startup spellings** - dropped vowels, numerals, or punctuation-dependent names.
4. **Overly broad descriptive names** - generic terms like `Duskcue` or `Open Streaming Platform` that are clear but not memorable.
5. **Aggressive or hyper-technical names** - security, ops, or hacker-coded names that do not fit a family-facing viewing product.

## Current Naming Methodology Update (June 2026)

Recent online guidance on brand identity and product naming was consistent on the main evaluation method even when the examples and framing varied.

Use this process for future naming rounds:

1. **Start with strategy, not syllables** - define tone, audience, and competitive posture before worrying about exact length.
2. **Favor distinctiveness over literal description** - names that are too descriptive are easier to explain once but harder to own and remember.
3. **Bias toward spoken ease** - if a name is awkward to say, hear, or repeat, it is weaker even if it looks good on-screen.
4. **Check stretch across surfaces** - the same name must survive TV launchers, mobile UI, URLs, verbal recommendations, and icon labels.
5. **Screen before attachment** - do quick public-web, GitHub, linguistic, and early trademark sanity checks before treating a candidate as a front-runner.
6. **Test recall, not just taste** - the stronger candidate is usually the one people can repeat accurately after one exposure, not the cleverest one.
7. **Keep the system coherent** - name, icon, voice, and UI expression should reinforce one another instead of solving separate brand problems.

Practical consequence for this project: do not force coined names into an 8 to 10 letter range. If a shorter 4 to 7 letter form is clearer, easier to pronounce, and more memorable, prefer the shorter form.

## Design Options

| Option | Pros | Cons | Verdict |
|---|---|---|---|
| Descriptive technical name | Immediately understandable | Generic, forgettable, wrong emotional tone, weak icon/story potential | Reject |
| Hard-edged infrastructure brand | Distinct from mainstream streamers | Feels like admin software, not a viewing product | Reject |
| Warm cinematic household brand | Matches home viewing, supports a memorable UI personality, works across clients | Requires discipline to avoid sounding precious | Preferred |
| Clone-adjacent pun or suffix name | Fast recognition of category | Derivative, weak long-term identity, higher confusion and legal risk | Reject |

## Recommended Naming Direction

### Preferred brand territory

The product should live in a **warm cinematic household** naming territory.

That means the name should evoke one or more of these ideas:

- light
- screen or stage
- gathering or home
- curation or presentation
- evening viewing rather than technical infrastructure

### Voice and tone

The brand voice should be:

- calm
- direct
- modern
- trustworthy
- lightly cinematic

The brand voice should not be:

- snarky
- aggressively geeky
- enterprise-corporate
- nostalgic to the point of parody

### Working shortlist

The first-screened names did not produce a clean public-name shortlist.

| Candidate | Screened state | Why it still matters |
|---|---|---|
| Bluehour | Conditional alternate only | Strongest surviving tonal candidate, but active `bluehour.io` and exact-name namespace friction still keep it from being a clean lead |
| Gloamline | Conditional alternate only | Strongest screened compound so far, with very light collision signal and a clearer brand feel than earlier compound attempts |
| Duskcue | Conditional alternate only | Cleaner second-wave compound survivor, though still slightly production-coded |
| Lantern | Reference only | Excellent tonal fit, but materially occupied by existing software/app usage |
| Nocturne | Reference only | Useful tone signal, but too crowded in software and culture to lead publicly |
| Afterglow | Reference only | Strong visual territory, but deeper screening found active consumer-brand and app collisions |
| Hollis | Conditional alternate only | Best result from the surname-like correction round, but still not clean enough to promote as a public lead |
| Tavin | Conditional alternate only | Better than most screened transformed forms, but exact-name app usage already weakens it |
| Titlerow | Reference only | Clean namespace, but reads more like a UI shelf label than the parent brand |

Marquee, Backlot, and Halo should not stay on the public-name shortlist after the quick screen. Hearthlight is cleaner than those names, but an exact-name app already occupies the clearest consumer search path, so it also stays out of the active shortlist.

Treat the screened set as evidence, not as a resolved shortlist. The deeper Avren audit weakened that path materially, so there is not currently a single clean lead. Bluehour is the strongest tonal survivor, Gloamline is the strongest screened compound alternate, Duskcue is a cleaner secondary compound alternate, Hollis and Tavin are only conditional backups, and compounds should stay confined to the stricter atmosphere-plus-editorial lane.

### Naming approach correction

The first twelve quick screens point to a broader problem than individual candidate failure.

The current convention has leaned too heavily on clean real-word nouns in cinematic, evening, light, and household territory. Those words sound right for the product, but they are repeatedly already occupied by software brands, creator platforms, accessory brands, or media properties.

Practical consequence for the next round:

1. Do not default to broad dictionary nouns.
2. Build distinctiveness into generation earlier instead of screening it only after taste-based shortlisting.
3. Favor names that still speak clearly, but come from less publicly occupied word classes than the first twelve screened names.
4. Explicitly test surname-like and rarer-word classes instead of only direct media, light, and household metaphors.
5. After the Round C screen, shift again toward more brand-neutral transformed forms rather than leaning too hard on surname-like candidates.
6. After the Round D screen, keep the transformed-form direction but make it plainer and less designerly so spoken ease stays ahead of stylization.
7. If compounds are revisited, use them only in a stricter editorial-atmospheric lane rather than returning to house, home, room, screen, or frame compounds.
8. Prefer transformed or compound candidates that are easy to repeat in conversation without explanation, even when raw namespace signals look clean.
9. If a name starts reading more like a UI label than a parent brand, demote it even when the namespace is unusually clean.

Reelhouse is no longer a recommended lead public candidate. The quick-screen pass found active media-adjacent usage plus enough broader public usage to make confusion risk too high for the current shortlist. Keep it as a useful conceptual reference, not as a preferred launch name.

### Current recommendation

Do not lock the final product name yet.

Use this document to narrow the project into the preferred naming territory first, then perform trademark, package, and domain checks on the top candidates before selecting the final name. Use the quick-screen risk snapshot in [NAME_CONCEPTS.md](NAME_CONCEPTS.md) to avoid spending time on names that already look materially crowded.

For the working compare-and-cut set, use [NAME_CONCEPTS.md](NAME_CONCEPTS.md), which now records screened outcomes for the first twelve names, the Round C, D, E, and F screens, the follow-up compound screen, the tighter Avren and Bluehour audits, and the current Bluehour-versus-Titlerow preference check.

If a working name is needed immediately for prototypes, use a neutral internal codename. Do not treat any of the first-screened names as an assumed public placeholder yet.

## Brand Expression Rules

1. Prefer sentence case in product UI.
2. Keep product copy plain and benefit-first rather than slogan-heavy.
3. Avoid putting the product name in every heading or control label.
4. Keep iconography simple enough to survive TV launcher tiles and mobile app icons.
5. Let artwork and content carry emotional weight; the brand should frame the experience, not overpower it.

## Pros vs Cons

### Pros

- Gives the repo a real naming direction without pretending the legal selection work is already done.
- Aligns the brand with the actual product: personal, self-hosted, content-first, and household-friendly.
- Reduces the risk that the UI and copy drift into admin-tool language.
- Creates a stable filter for future name candidates instead of debating every new idea from scratch.

### Cons

- The final name still depends on later availability and trademark checks.
- A warm household direction can drift into softness if the visual system is not kept disciplined.
- Any shortlist can create attachment before validation work is complete.

## Final Recommendation Stack

1. Use a warm cinematic household brand direction.
2. Reject clone-adjacent and infrastructure-sounding names.
3. Keep the final name short, pronounceable, and visually clean across all client surfaces.
4. Use a neutral internal codename for prototypes until a screened candidate survives deeper review.
5. Do the final trademark, package, and domain checks before locking the public product name.

## Three More High-Value Design Areas

1. Define the app-icon direction so the name and symbol system evolve together.
2. Decide whether the server and client products share one public name or need a subtle server/client labeling scheme.
3. Define the product glossary for user-facing terms such as library, collection, continue watching, downloads, and admin settings.

## Official Sources

- Microsoft Learn: Recommendations for writing user interface content - https://learn.microsoft.com/en-us/power-platform/well-architected/experience-optimization/user-interface-content
- Microsoft Learn: Recommendations for following design standards - https://learn.microsoft.com/en-us/power-platform/well-architected/experience-optimization/design-standards
- Apple Human Interface Guidelines - https://developer.apple.com/design/human-interface-guidelines
- Android Developers: Design for TV - https://developer.android.com/design/ui/tv/guides/foundations/design-for-tv
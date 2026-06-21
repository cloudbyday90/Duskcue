# Image Formats

## Overview

This document is the authoritative design for image format strategy across Duskcue — which formats the server ingests, what it generates internally, and what it delivers to clients. It unifies policy decisions that were previously scattered across [POSTER_MANAGEMENT.md](POSTER_MANAGEMENT.md), [STORYBOARDS.md](STORYBOARDS.md), and [METADATA_OVERLAYS.md](METADATA_OVERLAYS.md).

The decision documented here: **WebP as the primary delivery format for all server-generated image content.** Store the original from upstream providers (TMDb/Fanart.tv) untouched as a compositing base; serve WebP to clients. AVIF is researched and rejected for primary use (encoding cost prohibitive on NAS hardware); JPEG XL is researched and rejected (browser support inadequate).

## Scope

**Covers:**

- Format choice for every category of image Duskcue produces or delivers (artwork, thumbnails, storyboards, overlays, user uploads)
- Ingest-vs-delivery conversion strategy (when to transcode, what to keep)
- Browser/TV/mobile platform support matrix
- Rust image-encoding ecosystem and crate choices
- Per-category quality and sizing policy
- Original-artwork preservation (clean art)

**Does NOT cover:**

- Where artwork is stored on disk — see [POSTER_MANAGEMENT.md](POSTER_MANAGEMENT.md) and [CACHE_STORAGE.md](../operations/CACHE_STORAGE.md)
- Artwork selection logic (which TMDb poster wins) — see [POSTER_MANAGEMENT.md](POSTER_MANAGEMENT.md)
- Overlay compositing algorithm — see [METADATA_OVERLAYS.md](METADATA_OVERLAYS.md)
- Storyboard timeline logic — see [STORYBOARDS.md](STORYBOARDS.md)

## Decision — WebP as Primary Delivery Format

**Duskcue standardizes on WebP for all server-generated image content delivery.** Originals from upstream providers are preserved untouched on disk; WebP variants are generated for client delivery.

### Why WebP (and Not AVIF)

| Concern | WebP | AVIF | Verdict |
|---|---|---|---|
| Compression vs JPEG | 25–35% smaller | 40–60% smaller | AVIF wins on bytes |
| Encode speed (1080p image) | ~90ms | 1–4 seconds (libaom); up to 48s at max quality | **WebP wins by 10–50×** |
| Peak RAM during encode | ~200 MB | ~2.5 GB (libaom) | **WebP wins** — critical for NAS/SBC hardware |
| Browser support (2026) | Universal since 2020 (Safari 14+) | ~96% global since 2022 (Safari 16.4+) | Both effectively universal; WebP slightly safer |
| Samsung Tizen TV support | Tizen 2.4+ (2016, all models) | Tizen 6.5+ (2022, Chromium M85+) | **WebP wins** — covers 6 more model years |
| LG webOS TV support | All Chromium-based versions | webOS 6.x+ (2020, Chromium 85+) | **WebP wins** — covers older TVs |
| Lossless mode | Yes | Yes | Tie |
| Alpha (transparency) | Yes | Yes | Tie |
| HDR / wide gamut | No | Yes | AVIF wins (but Duskcue artwork is SDR — TMDb serves SDR) |
| Rust encoder maturity | `webp` crate (libwebp bindings) — fast, production-grade | `ravif` crate (pure Rust) — slow, threading-required | **WebP wins** |
| Compression at high quality (q90+) | ~10–15% smaller than JPEG | Marginal further gain over WebP | AVIF wins by ~10% |
| Consistency with existing decisions | `overlay_image_format: "webp"` already chosen (POSTER_MANAGEMENT.md) | Would introduce second format | **WebP wins** — consistency |

**The decisive factor is encode cost on target hardware.** Duskcue deployments are commonly NAS devices (Synology, QNAP), SBCs (Raspberry Pi 5), or low-power mini-PCs (Intel N100). On these devices, AVIF encoding for a single 1080p image takes 4–10 seconds and ~2.5 GB peak RAM. A 1000-item library migration (Phase 14 scenario) with 5 images per item = 50,000 encodes = 28+ hours of CPU and constant memory pressure. WebP encodes the same library in under 10 minutes total with negligible memory impact.

The AVIF compression advantage (an additional ~20% over WebP) is not worth the operational cost on Duskcue's target hardware. Revisit if Duskcue adds a hardware-accelerated AVIF encode path (Intel QSV / GPU) in a future phase.

### Why Not JPEG XL

JPEG XL offers AVIF-class compression with fast encoding and the killer feature of lossless JPEG transcoding (20% smaller JPEGs without re-encoding). As of June 2026:

- ❌ **Browser support is Safari-only on the open web** (Chrome dropped the experimental flag in 2022; Firefox has it behind `about:config`)
- ❌ **TV support is zero** — no Tizen or webOS version ships JPEG XL
- ❌ **Rust ecosystem is immature** — no production-grade encoder

Chrome's stated position is that they're "evaluating" re-adding JPEG XL. The Chrome Canary 145 (early 2026) shipped a Rust-based JXL decoder as an experiment, suggesting possible stable support in H2 2026. **Until Chrome ships JXL stable, it cannot be a Duskcue delivery format.** Revisit if/when Chrome reverses course.

JPEG XL remains the right choice for archival/backup scenarios (lossless JPEG reduction), but Duskcue doesn't have an archival use case.

### Why Not Serve Originals Directly (No Transcoding)

TMDb serves JPEGs (occasionally PNG for logos). JPEG is universal but 25–35% larger than WebP at equivalent quality. For a 1000-item library with 5 images each at ~200 KB per JPEG, that's ~1 MB total artwork. WebP cuts it to ~700 KB. On a gigabit LAN this is negligible, but on remote/streaming-mode deployments and mobile clients, the savings matter.

Additionally, generating WebP variants gives Duskcue:
- **Control over sizing** — generate `w185`/`w342`/`w500`/`original` variants matching TMDb's size catalog, serve the right one per client context
- **Strip metadata** — drop EXIF/thumbnail chunks from upstream images (privacy hygiene)
- **Consistent format** — every image served is WebP regardless of upstream source format (some Fanart.tv art is PNG, some TMDb is JPEG)

## Per-Category Policy

| Image Category | Source | Stored Format | Delivery Format | Sizing Policy |
|---|---|---|---|---|
| **Movie/TV poster** | TMDb original JPEG | JPEG (original, untouched) | WebP | Variants: w185 (card thumb), w342 (card), w500 (detail hero), original (full-quality) |
| **Backdrop** | TMDb original JPEG | JPEG (original, untouched) | WebP | Variants: w300 (card), w780 (detail hero), w1280 (fullscreen), original |
| **Logo (clearart)** | TMDb PNG / Fanart.tv PNG | PNG (original, untouched — alpha required) | WebP (lossless, alpha preserved) | Single size: original |
| **Season poster** | TMDb original JPEG | JPEG (original, untouched) | WebP | Same variants as movie poster |
| **Episode still** | TMDb original JPEG | JPEG (original, untouched) | WebP | Variants: w185 (card), w300 (detail), original |
| **Storyboard sprite** | FFmpeg-extracted frames | N/A (generated directly) | WebP | Per [STORYBOARDS.md](STORYBOARDS.md) — sprite sheet dimensions |
| **Overlay composite** | Source artwork + overlay layers | N/A (composited at apply time) | WebP | Same sizing as source artwork |
| **User upload** | Admin-uploaded JPEG/PNG/WebP | Original preserved in `/data/metadata/artwork/uploads/` | WebP | Same variants as equivalent TMDb artwork |
| **Collection poster** | Generated via overlay engine | N/A | WebP | Per collection poster config |

### Sizing Rationale

The `w185`/`w342`/`w500`/`original` sizing mirrors TMDb's own size catalog. This is intentional:

1. **Predictable** — admins familiar with TMDb's sizing understand the variants
2. **Cache-friendly** — same set of variants for every poster regardless of source
3. **Adequate for UI** — w185 is sized for grid cards (~200px display), w342 for larger cards, w500 for detail pages, original for fullscreen hero
4. **No wasted bandwidth** — clients request the variant matching their display size, not the original

The web client uses `srcset` to declare these variants, letting the browser pick the right one for the viewport/DPR:

```html
<img
  src="/api/v1/items/{id}/artwork/poster?size=w342"
  srcset="
    /api/v1/items/{id}/artwork/poster?size=w185 185w,
    /api/v1/items/{id}/artwork/poster?size=w342 342w,
    /api/v1/items/{id}/artwork/poster?size=w500 500w,
    /api/v1/items/{id}/artwork/poster?size=original 1000w
  "
  sizes="(max-width: 768px) 50vw, 200px"
  alt="..."
  loading="lazy"
/>
```

## Storage Layout

```
/data/metadata/artwork/          ← persistent, source-of-truth originals
├── tmdb/
│   ├── posters/                 ← original JPEGs from TMDb (untouched)
│   ├── backdrops/
│   ├── logos/                   ← original PNGs (alpha preserved)
│   ├── season_posters/
│   └── episode_stills/
├── fanart/                      ← original PNGs from Fanart.tv
├── uploads/                     ← admin-uploaded originals
└── assets/                      ← symlink/reference to asset directory

/cache/images/                   ← regenerable cache (can be deleted and rebuilt)
├── webp/                        ← WebP variants for delivery
│   ├── posters/
│   │   ├── w185/
│   │   ├── w342/
│   │   ├── w500/
│   │   └── original/
│   ├── backdrops/
│   │   ├── w300/
│   │   ├── w780/
│   │   ├── w1280/
│   │   └── original/
│   └── ...
├── clean/                       ← scaled source artwork (overlay base, pre-overlay)
│   ├── posters/
│   └── backdrops/
└── overlays/                    ← composited results with overlays applied
    ├── posters/
    └── backdrops/
```

**Originals are never modified.** This is the "clean art preservation" principle from [POSTER_MANAGEMENT.md](POSTER_MANAGEMENT.md) — source artwork is the immutable base; all transformations produce files in `/cache/images/` which is regenerable.

## Conversion Pipeline

### When Conversion Happens

| Trigger | What Happens | Latency Budget |
|---|---|---|
| **Scan enrichment** (TMDb download) | Original JPEG/PNG stored in `/data/metadata/artwork/` | Network-bound (download only — no encode) |
| **Post-scan background task** | WebP variants generated for newly-downloaded artwork | Background — does not block scan; runs via scheduler |
| **First request for a missing variant** | On-demand WebP generation, cached to disk | <500ms per image (acceptable for one-time first hit) |
| **Overlay apply task** | Composited result stored as WebP in `/cache/images/overlays/` | Background scheduled task |
| **Storyboard generation** | FFmpeg extracts frames, sprite sheet assembled as WebP directly | Background scheduled task |
| **User upload** | Original stored; WebP variants generated synchronously (user is waiting) | <2s per variant |

**Background-first strategy:** WebP variants for newly-downloaded artwork are generated by a scheduled task (`artwork_variant_generator`, runs after library scan completes — same pattern as `subtitle_auto_fetch`). This avoids encoding latency on first request for the common case (post-scan). On-demand generation is the fallback for cache misses.

### Encoding Settings

WebP encoding via the `webp` crate (libwebp bindings):

| Setting | Value | Rationale |
|---|---|---|
| Quality | 90 (lossy mode) | Visually indistinguishable from original; matches `overlay_image_quality` default |
| Method | 4 (of 0–6) | libwebp default; good balance of encode speed and compression |
| Lossless | false (except for logos/clearart with transparency) | Photographic content compresses better lossy |
| Alpha preservation | true (when source has alpha) | Required for logos/clearart |

For **logos and clearart with transparency**, encoding is lossless WebP (`quality: 100, lossless: true`). These are typically text/graphic content where lossy compression artifacts are visible.

### Variant Generation Order

When generating variants for a new artwork item, generate in size order from smallest to largest. This ensures that if generation is interrupted (server shutdown, OOM), the most-important variants (thumbnails for browse pages) are already available. The original-size WebP is generated last.

## Platform Support Matrix

| Platform | WebP support | Notes |
|---|---|---|
| Chrome, Edge, Firefox (desktop) | ✅ Since 2019–2020 | — |
| Safari (desktop + iOS) | ✅ Since 2020 (Safari 14) | — |
| Samsung Tizen TV | ✅ Tizen 2.4+ (2016, all Chromium models) | Older WebKit Tizen (<2016) is effectively unsupported for Duskcue anyway |
| LG webOS TV | ✅ All Chromium-based versions (4.x+) | — |
| Tauri desktop (Windows/Linux) | ✅ WebView2 / WebKitGTK | — |
| Tauri desktop (macOS) | ✅ WKWebView | — |
| Flutter mobile | ✅ via `image` package decoder | — |

**WebP support is universal on every Duskcue target platform.** No fallback to JPEG is strictly required, but the web client uses `<picture>` for cheap insurance against unknown edge cases (older embedded WebViews, future browser regressions):

```html
<picture>
  <source srcset="...w342.webp" type="image/webp" />
  <img src="...w342.jpg" alt="..." loading="lazy" />
</picture>
```

The `<img>` fallback references a JPEG variant generated alongside WebP (storage overhead is acceptable; JPEG variants are ~25% larger than WebP but small in absolute terms).

## Edge Cases

### Very Large Originals (4K backdrops)

TMDb backdrops can be up to 3840×2160. Encoding an `original`-size WebP variant takes ~200ms on NAS hardware — acceptable. The `w1280` and smaller variants are negligible.

### PNG Logos with Transparency

Logos and clearart from TMDb/Fanart.tv are PNG with alpha channel. Encoding to lossy WebP would corrupt the alpha. Solution: encode these as **lossless WebP** (smaller than PNG, preserves alpha exactly). The pipeline detects alpha presence via the `image` crate and switches to lossless mode automatically.

### Animated Artwork (Future)

TMDb does not currently serve animated artwork. If animated posters/backdrops become a thing, WebP supports animation natively (and is the recommended web format for short animations vs GIF). No changes needed to the pipeline.

### Bulk Import CPU Spike

A Phase 14 migration from Plex/Jellyfin with 1000+ items triggers up to 5000+ artwork downloads + WebP encodes. On NAS hardware this is 10–30 minutes of background CPU. Mitigations:

1. **Background task** — variant generation runs after scan completes, doesn't block user
2. **Rate-limited encoding** — semaphore caps concurrent encodes to `min(cpu_count, 4)` to avoid starving other server tasks (transcodes, API requests)
3. **Resumable** — variants are cached on disk; if interrupted, only missing variants are generated on next run

### Cache Invalidation

When does a WebP variant need regeneration?

| Trigger | Regenerate? | How |
|---|---|---|
| Source artwork changed (TMDb refresh found different image) | ✅ Yes | Source file hash differs; delete old variants, generate new |
| Admin changed `overlay_image_quality` config | ✅ Yes for overlays; ❌ No for plain artwork variants | Config change triggers overlay re-apply task; plain artwork variants don't depend on quality setting |
| Admin changed `overlay_image_format` from `webp` to something else | ✅ Yes | Future enhancement — currently only WebP is supported |
| Library re-scan found no changes | ❌ No | Variants persist |
| Cache directory cleared | ✅ Yes (lazily) | First request triggers on-demand regeneration |

### Artwork Not Found in TMDb

For items TMDb has no artwork for (rare, but happens for obscure content), no WebP variants exist. The artwork endpoint returns HTTP 404; the web client renders the gradient-placeholder from `MediaCard.svelte` (existing behavior).

### Corrupt Source Image

If the source JPEG/PNG is corrupt (truncated download, disk error), WebP encoding fails. Mitigation: the encoder catches the error, logs it, and the artwork endpoint returns 404. The scan log surfaces the corrupt artwork for admin review. No server crash.

## Crate Selection

| Crate | Purpose | Status |
|---|---|---|
| [`image`](https://crates.io/crates/image) `0.25` | Decoding JPEG/PNG/WebP source images; resizing; pixel manipulation | ✅ In workspace (Phase 10 Task 9 — `image_pipeline.rs`) |
| [`webp`](https://crates.io/crates/webp) `0.3` | WebP encoding via libwebp bindings — supports lossy + lossless + alpha | ✅ In workspace (Phase 10 Task 9 — `image_pipeline.rs`) |
| `fast_image_resize` (optional) | SIMD-accelerated resizing for high-throughput variant generation | Optional — `image` crate's built-in resize is adequate for typical workloads |

**Rejected alternatives:**

- `image-webp` (image-rs pure-Rust WebP encoder) — supports only lossless encoding, no lossy mode. Lossy is required for photographic posters.
- `ravif` — AVIF encoder, rejected per the AVIF decision above.
- `mozjpeg` — JPEG encoder. Used only for the JPEG `<picture>` fallback if/when that's implemented; not the primary path.

### `webp` Crate Build Notes

The `webp` crate uses libwebp (Google's C library) via `std::ffi`. Build requirements:

- **Linux/macOS**: links against system libwebp or builds it from source via the `bundled` feature. The `bundled` feature is the default and produces a static binary with no runtime dependency.
- **Windows (MSVC)**: builds cleanly via the `bundled` feature; no NASM required (unlike `aws-lc-sys` which Phase 1 explicitly avoided).

The `bundled` feature is the right choice for Duskcue — ensures consistent builds across all target platforms without requiring operators to install libwebp system-wide.

### `image` + `webp` Crate Integration (Task 9 implementation)

The `webp` crate's default features include `img` which activates `Encoder::from_image(&DynamicImage)`. **Duskcue disables this feature** (`webp = { version = "0.3", default-features = false }`) and uses `Encoder::from_rgba(bytes, w, h)` directly instead. Rationale:

1. **Decouples version selection** — the `webp` crate internally pins `image = "0.25.6"`; if we let it dictate our `image` version, a future `image` release (e.g. `0.26`) couldn't be adopted until `webp` bumps its dep. By disabling `img` and passing raw RGBA bytes ourselves, the two crates evolve independently.
2. **Explicit format control** — Duskcue pins `image = { version = "0.25", default-features = false, features = ["jpeg", "png", "webp"] }` so only the decoders we actually need are compiled (no GIF/TIFF/BMP/AVIF decoder bloat).
3. **No API loss** — `from_rgba` is available without the `img` feature; the only thing we lose is `from_image`, which is a trivial `to_rgba8()` wrapper anyway.

RGBA bytes are extracted via `DynamicImage::to_rgba8().into_raw()` and handed to `Encoder::from_rgba`. The `image` crate's resize uses `FilterType::Lanczos3` (highest-quality downscale filter) for all variant generation.

## Implementation Status

| Component | Status | Notes |
|---|---|---|
| TMDb artwork download (originals) | ✅ Implemented | `services/artwork_downloader.rs` — downloads `original` size, stores in `/data/metadata/artwork/tmdb/` |
| WebP variant generation | ✅ Implemented | `services/image_pipeline.rs` (Phase 10 Task 9) — stateless decode → resize → encode library; alpha-aware (lossy for opaque, lossless for transparency); `variants_for_category` encodes the per-category size catalog below |
| Artwork delivery endpoint | Spec only | Future: Task 10 — `GET /api/v1/items/{id}/artwork/{type}?size={size}&format={format}` will call `image_pipeline::generate_variant` on cache miss |
| Storyboard WebP generation | ✅ Implemented | Phase 10 Task 4 — per [STORYBOARDS.md](STORYBOARDS.md); FFmpeg emits WebP directly (does not use `image_pipeline.rs` — different code path, FFmpeg's own libwebp encoder) |
| Overlay compositing to WebP | Spec only | Phase 12 — per [METADATA_OVERLAYS.md](METADATA_OVERLAYS.md); `overlay_image_format: "webp"` already configured |
| User upload pipeline | Spec only | Phase 13 (admin UI) |
| `<picture>` fallback in web client | Spec only | Phase 8 follow-up or when artwork delivery endpoint lands |

The next concrete implementation step is wiring the artwork delivery endpoint (Task 10) with on-demand WebP variant generation via `image_pipeline::generate_variant` — lazily builds missing variants on first request and caches them under `/cache/images/webp/`. A background `artwork_variant_generator` scheduled task pre-warms the cache after library scans for the common case.

## Key Decisions

1. **WebP over AVIF** — AVIF's compression advantage (~20% over WebP) is not worth the 10–50× encode cost on Duskcue's target hardware (NAS, SBC, mini-PC). WebP gets 70% of the modern-format benefit at 1% of the cost.
2. **WebP over originals-direct** — 25–35% smaller than JPEG, consistent format across all sources (JPEG from TMDb, PNG from Fanart.tv), enables sizing variants, strips upstream metadata.
3. **AVIF rejected for primary delivery** — Encoding cost (1–4 seconds/image at 1080p, 2.5 GB RAM peak) is prohibitive on NAS hardware during bulk imports. Revisit if hardware-accelerated AVIF encode (Intel QSV / GPU) is added.
4. **JPEG XL rejected** — Browser support is Safari-only on the open web (Chrome dropped it in 2022). Revisit if Chrome reverses course.
5. **Originals preserved untouched** — Clean art preservation principle from POSTER_MANAGEMENT.md. Originals are the compositing base and the recovery path if cache is lost.
6. **WebP variants at standard TMDb sizes** (`w185`/`w342`/`w500`/`original`) — predictable, cache-friendly, matches admin mental model.
7. **Background-first variant generation** — Scheduled task after scan completion; doesn't block scan or first-request. On-demand generation is the cache-miss fallback.
8. **Lossless WebP for transparent content** — Logos/clearart with alpha get lossless encoding to preserve transparency exactly.
9. **`webp` crate via libwebp bindings** — The pure-Rust `image-webp` crate supports only lossless encoding; lossy is required for posters. The `bundled` feature produces static binaries with no runtime libwebp dependency.
10. **`<picture>` fallback to JPEG** — Cheap insurance against unknown edge cases. JPEG variants generated alongside WebP at modest storage cost.
11. **Consistency with overlay format decision** — `overlay_image_format: "webp"` in `MetadataConfig` already commits to WebP for composited overlays. Standardizing on WebP for source artwork delivery means a single image format flows through the entire pipeline.

## Relationship to Other Domains

| Document | Relationship |
|---|---|
| [POSTER_MANAGEMENT.md](POSTER_MANAGEMENT.md) | Artwork lifecycle (source → select → customize → display). This document defines the format policy at each stage; that document defines the selection and customization logic. |
| [STORYBOARDS.md](STORYBOARDS.md) | Storyboard sprite sheets are WebP per this policy (already aligned). |
| [METADATA_OVERLAYS.md](METADATA_OVERLAYS.md) | Overlay composite output is WebP per this policy (already aligned via `overlay_image_format: "webp"`). |
| [CACHE_STORAGE.md](../operations/CACHE_STORAGE.md) | `/cache/images/` storage tier, eviction policy, disk-space monitoring for the WebP variant cache. |
| [HTTP_CACHING.md](HTTP_CACHING.md) | Artwork URLs are `Cache-Control: public, max-age=86400, stale-while-revalidate=604800, immutable` — fingerprinted URLs allow aggressive caching. |
| [API_CONVENTIONS.md](API_CONVENTIONS.md) | Per-endpoint Cache-Control table includes the artwork URL policy. |
| [VIDEO_FORMATS.md](VIDEO_FORMATS.md) / [AUDIO_FORMATS.md](AUDIO_FORMATS.md) | Sister "formats" docs for video and audio. This document is the image equivalent. |
| [BUILD_ORDER.md](../../BUILD_ORDER.md) | Phase 8 follow-up (artwork delivery endpoint); Phase 10 (storyboard WebP); Phase 12 (overlay compositing WebP). |

## Research Sources

- **[AVIF vs WebP vs HEIC vs JPEG XL: which should you use in 2026?](https://www.reddit.com/r/software/comments/1s9fege/)** — r/software community discussion with W3Techs deployment statistics (March 2026): WebP 19% deployment, AVIF 1.3%, JXL <0.1%
- **[JPG vs WebP vs AVIF: Which Image Format Should You Use in 2026?](https://hiredigital.com/blog/jpg-vs-webp-vs-avif-2026)** — Comprehensive comparison with decision tree; recommends WebP as default, AVIF for hero images
- **[AVIF encoding speed — the numbers nobody talks about](https://dev.to/serhii_kalyna_730b636889c/avif-encoding-speed-the-numbers-nobody-talks-about-a2h)** — Operational data on AVIF vs WebP encode time, RAM, and CPU; "AVIF optimizes bandwidth, WebP optimizes compute"
- **[Crystallize: AVIF vs. WebP](https://crystallize.com/blog/avif-vs-webp)** — Practical production deployment patterns with `<picture>` element
- **[Samsung Tizen Web Engine Specifications](https://developer.samsung.com/smarttv/develop/specifications/web-engine-specifications.html)** — Per-year Tizen Chromium version mapping (WebP support since Tizen 2.4 / 2016)
- **[LG webOS Web API and Web Engine](https://webostv.developer.lge.com/develop/specifications/web-api-and-web-engine)** — webOS TV Chromium version history
- **[`webp` crate (libwebp bindings)](https://crates.io/crates/webp)** — Rust WebP encoder/decoder; supports lossy + lossless + alpha
- **[`image-webp` crate](https://github.com/image-rs/image-webp)** — Pure-Rust WebP encoder/decoder; lossless-only for encoding (rejected for lossy use case)
- **[`ravif` crate](https://crates.io/crates/ravif)** — Pure-Rust AVIF encoder; rejected for primary use due to encode cost
- **[Can I Use: AVIF](https://caniuse.com/avif)** / **[Can I Use: WebP](https://caniuse.com/webp)** / **[Can I Use: JPEG XL](https://caniuse.com/jpegxl)** — Browser support matrices

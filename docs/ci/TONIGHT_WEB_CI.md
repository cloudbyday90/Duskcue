# Fresh-checkout Tonight web qualification

Prepared October 7, 2026. This Linux CI path qualifies the current shared web sources and encoded browser fixtures. It does not qualify native desktop behavior, assistive-technology speech, production database orchestration or deployment. No local heavy workflow was launched while preparing it.

Verified October 8 `685377e` browser report in run `37733976064`: all 119 cases execute; 115 pass and four fail, with zero skips/flaky cases and 123 attempts. The three actual-caption and eight French/Arabic localization cases pass first attempt. Three progressive failures occur at exact canonical Title-query comparison after media assertions, so their source-only correction adds explicit catalog `from` entry URLs without changing the comparator. The headed background case remains visibly foregrounded on both attempts and requires a real harness repair; no synthetic event, visibility override or skip can qualify it. The 34,358,098-byte retained browser archive SHA256 is `c527944acf00adbe09ed3874a27dca8f12c6b2b5d401044658f228b9bc1224cc`. The full result gate correctly stays failed until a repaired complete run passes.

### Real tab visibility harness correction — October 8

The locked [Playwright 1.63.0 Chromium implementation](https://github.com/microsoft/playwright/blob/v1.63.0/packages/playwright-core/src/server/chromium/crPage.ts#L503-L504) enables `Emulation.setFocusEmulationEnabled` for each ordinary main page. [Chrome's focused-page documentation](https://developer.chrome.com/docs/devtools/rendering/apply-effects#emulate_a_focused_page) states that this setting keeps `document.visibilityState` visible and suppresses `visibilitychange`. The installed package contains that exact default; the failed headed attachment records zero visibility events on both attempts.

The focused test now uses public CDP sessions to disable that existing simulation on both pages, then requires positive equal `Browser.getWindowForTarget` window IDs. It retains actual `bringToFront`, trusted Hidden/Visible events, eleven seconds of unchanged paused timing, restoration without catch-up, permanent Cancel and exact Title return. Disabling existing emulation restores observation; it does not force document state. No production interaction, launch argument or other test's browser mode changes. The [official protocol](https://github.com/ChromeDevTools/devtools-protocol/blob/master/pdl/domains/Emulation.pdl) defines the enabled boolean. Syntax/listing/whitespace checks pass; actual corrected visibility and full-suite execution remain pending hosted CI.

Use Node 24 and the committed npm lockfile. Install matching Chromium and its Linux dependencies through the installed Playwright CLI. Official references checked October 7: [Playwright CI](https://playwright.dev/docs/ci), [browser installation](https://playwright.dev/docs/browsers), and [JSON/HTML reporters](https://playwright.dev/docs/test-reporters). Keep one browser worker and one Vitest worker. The Windows memory wrapper is not a Linux CI command.

The ordinary media fixture needs host FFmpeg with `libx264`, AAC and `libwebp`, plus the standard lavfi/scale/HLS functionality. Install FFmpeg on the runner and set `DUSKCUE_TEST_FFMPEG=/usr/bin/ffmpeg`; an explicit missing host binary fails instead of falling back to an old Docker image. `ensurePlaybackMedia` then generates the 20-second MP4/TS, two-second ending derivative and storyboard from that run. Do not cache or download `.cache/tonight-web/media` from a developer machine.

The real-caption prerequisite is the separate current Linux FFmpeg qualification artifact. Its compiler/image job must compile the current Rust library test executable and paired bootstrap/probe, then run the exact production-argument case `services::transcoding::arguments::tests::real_audio_first_default_description_and_selected_srt_decode_through_production_arguments`, together with all required managed cases and source-defined readiness/lifecycle groups. The qualified image needs FFmpeg/ffprobe with libass, the checked-in Arial/sans Fontconfig alias, and the production Noto provider packages documented in [caption fonts](../design/PLAYBACK_CAPTION_FONTS.md). A freshly generated `producer/` tree retains the synthetic Matroska, both SRT cues and `caption/` manifest/init/four segments. Its JSON result records the commit, complete input inventory, immutable amd64 image, compiled artifact hashes, exact case results and verified owned cleanup. A failed managed case or absent caption output cannot be replaced with raw output or an availability flag.

The hosted Linux job invokes `node --max-old-space-size=128 scripts/qualification/linux.mjs --commit "$GITHUB_SHA" --output "$RUNNER_TEMP/tonight-linux"`. This driver enforces its clean matching GitHub-hosted checkout and records the exact UUID artifact directory as `artifact_dir`. Its pinned Alpine compiler image supplies Rust, build-base, Clang, CMake, headers, Perl, pkgconf, Protobuf and Node; Cargo runs with two jobs and debug information disabled inside the bounded compiler container. The runtime recipe supplies the paired managed binaries and production caption providers. The host needs Node, Git and its local Docker engine. Publish the exact successful `artifact_dir` contents so `result.json` sits directly at the web job's downloaded root; preserve the parent tree only as failure diagnostics. The consumer does not guess among nested runs.

Download that artifact from the same workflow run into a new directory. `web-captions.mjs artifact` verifies the unchanged committed web checkout against the driver's complete source inventory, checks all required case/producer identities and paired amd64 ELF files, and reads only bounded regular SHA256-matched assets. The consumer never executes a downloaded binary or starts Docker/Cargo. It appends the validated caption directory to `GITHUB_ENV` for the following browser step. A dated ignored path, an old compiled executable or a manually copied local caption tree is not a valid substitute.

Run these commands from the repository root, with the artifact-root placeholder replaced by the fresh downloaded directory:

```bash
npm ci --prefix clients/web
(cd clients/web && npx playwright install --with-deps chromium)
node --max-old-space-size=64 --test --test-concurrency=1 scripts/qualification/web-captions.test.mjs
NODE_OPTIONS=--max-old-space-size=3072 npm --prefix clients/web run check
NODE_OPTIONS=--max-old-space-size=1536 npm --prefix clients/web run test:unit -- --run --maxWorkers=1
NODE_OPTIONS=--max-old-space-size=3072 npm --prefix clients/web run build
node --max-old-space-size=128 scripts/qualification/web-captions.mjs artifact "$DOWNLOADED_LINUX_ARTIFACT_ROOT"
```

The unit command explicitly executes the thin Node timing/progressive cases before Vitest; forwarded `--run`/worker options remain on Vitest. The source check regenerates Paraglide before Svelte/type checking. Set the following step's environment, keeping the caption directory exported by the artifact consumer:

```bash
export DUSKCUE_TEST_FFMPEG=/usr/bin/ffmpeg
export PLAYWRIGHT_JSON_OUTPUT_FILE="$GITHUB_WORKSPACE/.cache/tonight-web/playwright/results.json"
export PLAYWRIGHT_HTML_OUTPUT_DIR="$GITHUB_WORKSPACE/.cache/tonight-web/playwright/report"
export PLAYWRIGHT_HTML_OPEN=never
NODE_OPTIONS=--max-old-space-size=3072 npm --prefix clients/web run test:e2e -- --workers=1 --reporter=list,html,json
node --max-old-space-size=64 scripts/qualification/web-captions.mjs results "$PLAYWRIGHT_JSON_OUTPUT_FILE"
```

The browser command deliberately has no file/grep filter. The JSON gate additionally requires all three actual-caption cases, all three progressive cases and the eight locale cases to have executed successfully, with no skipped results. It reports retries/flaky outcomes explicitly; retry success is not an unqualified first-attempt pass. If the browser command fails, retain its JSON, failure screenshots and traces, and run the result gate in an always-run diagnostic step without hiding the browser exit code. Keep logs and artifacts from failure as well as success: source/unit/build logs, the Linux qualification result and producer assets/logs, consumer output, Playwright JSON/HTML/results and cleanup evidence. Caption execution is no longer inferred from a default suite that skipped those opt-in cases.

The expanded locale journeys use actual header search, Movies/TV/Collections links, Viewing preferences and same-Title profile switching. The progressive zero-start case records the first presented frame observed by a passive listener installed before navigation, rather than sampling an already advancing `currentTime` after asynchronous readiness checks. It retains the strict two-second threshold and source identity. Frame callbacks are best-effort; presentation count and source are attached so missed/delayed observation can be diagnosed. The [WICG API explanation](https://wicg.github.io/video-rvfc/) is informative and does not establish a universal first-frame guarantee. Final-ended cases clear the earlier test marker and require the actual media `ended` state, retaining trusted native-event observation and source/stop identities. These harness refinements and the fresh CI artifact path remain pending actual remote execution.

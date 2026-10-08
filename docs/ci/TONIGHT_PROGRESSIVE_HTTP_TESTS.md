# Tonight progressive playback HTTP qualification

## Status

Source prepared October 8, 2026. Compilation and actual execution remain pending. Production HLS still uses VOD. The optional test flag below is not enabled by current hosted qualification, and existing passing Stop-contract results do not prove this new path ran.

This fixture extends the existing exact `stop_is_idempotent_serializes_heartbeat_seek_and_retains_original_profile` integration case without replacing its assertions. Three focused support modules separate HTTP/database orchestration, media validation and owned process observation. It uses the real playback/profile routers, migrated disposable PostgreSQL and the production transcode manager and launcher. Browser decoding against the live server is a separate, unqualified boundary.

## Required admission

Use the existing source-bound, guarded native Linux compiler/runtime/PostgreSQL workflow in [launch readiness](FFMPEG_LAUNCH_READINESS.md). Keep its 512 MiB runtime memory, two CPU and 128 PID limits, serial execution, exact artifact hashes and owned-container cleanup. Do not run this heavyweight fixture on the user's workstation while the lighter-work instruction remains active.

The existing disposable database and owned source requirements still apply. The additional explicit test-only opt-in is `DUSKCUE_TEST_PROGRESSIVE_PLAYBACK=event`, together with `DUSKCUE_TEST_FFMPEG_MODE=linux-local-managed`. It must be enabled and its marker made mandatory only after actual managed EVENT publication and its narrowly justified syscall/path policy are qualified. An absent marker or an omitted flag cannot count as progressive proof.

The prepared `scripts/qualification/progressive-http.mjs` result validator requires exactly one `DUSKCUE_PROGRESSIVE_PLAYBACK` JSON marker with `executed=true`, `passed=true`, the current resource ID, all recorded profile/seek/capacity/cleanup booleans true, physical fixture children confirmed exited, positive source size/runtime, growing segment counts and all four distinct encoder IDs/finite decoded initial timestamps near zero. It bounds output/marker size and stops at a duplicate without accumulating every record. Four thin Node cases verify valid admission and rejection of missing, failed, foreign, duplicate, malformed, oversized or incomplete evidence. These are validator behavior tests, not HTTP/encoder runtime proof. The validator is not wired into current SQL admission while the opt-in remains disabled; that connection is required with production EVENT. The ordinary exact one-test success and source/hash admission remain required in addition to the marker. Retries, missing results and unexecuted cases are not substitutes for a passing observation.

## Actual behavior and bounds

- Generate a finite 600-second stream-copy clip from the existing owned eight-second audio-first source using the installed original FFmpeg. Bound the file to 32 MiB, each tool's stdout/stderr to 1 MiB and execution to 30 seconds. FFprobe verifies physical duration, stream order, codecs and resolution before inserting fixture metadata.
- Create a distinct UUID data/cache root, fixture media item and a manager configured for one software transcode slot, one FFmpeg thread and two-second segments. Use a fresh `AppState::new_with_config` so the manager and request handlers share the intended configuration.
- Require real HTTP Start to return playable EVENT output without ENDLIST while one exact owned encoder is physically alive. Pin PID, parent, creation ticks, managed executable/device/inode and exact source/manifest arguments. Revalidate that identity before and after fetching and decoding complete init/fragment bytes. No injected production pacing, manager callbacks or synthetic process state establish this result.
- Reject another Start with the one-slot capacity response while the first encoder remains alive. Switch profiles and verify that the new active profile and another owner cannot fetch its assets. Perform actual account-owned Seek to 30 seconds, retain the original profile in the database, release the old PID/cache and verify the replacement encoder's actual seek argument and near-zero decoded stream timestamp. Playback Info must retain the source-wide runtime and absolute position.
- Observe append-only playlist growth through final ENDLIST and physical encoder exit. Await HTTP Stop, then require a fresh live Start to prove permit reuse; stop an active replacement and require PID/cache/session release. Bound the proof to 150 seconds, HTTP bodies to 16 MiB and the complete owned cache inventory to 128 MiB/4096 entries.
- On every returned error or timeout, attempt awaited session/manager cleanup and restore the original profile. Delete only the fixture item and exact canonical UUID root after all known encoders and original fixture tools have confirmed exit. A cancelled tool or uncertain kill/wait retains its per-fixture pending count and prevents filesystem removal until owned-container cleanup.

The HTTP marker explicitly leaves encoder exit status unqualified: ENDLIST and PID absence do not prove status zero. The separate managed EVENT case must observe the owned worker's actual `Succeeded` state. Likewise, a final progress line does not prove physical completion. Keep those component and HTTP proof boundaries distinct in the [completion audit](TONIGHT_COMPLETION_AUDIT.md).

Formatting and source inspection are the current checks. No C/Rust compiler, FFmpeg, database, runtime container or browser was launched locally to prepare this fixture.

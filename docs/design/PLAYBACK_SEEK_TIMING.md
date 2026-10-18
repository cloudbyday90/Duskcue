# Playback seek timing

Status: implemented with real-media and current-source HTTP qualification passed, 17 October 2026; final production image pending. This document owns the mixed-stream timestamp boundary used by explicit seeks and [bounded recovery](PLAYBACK_RECOVERY.md). [Playback compatibility](PLAYBACK_COMPATIBILITY.md) owns stream admission and processing actions.

## Observed problem

The controlled HTTP fixture rejected its first replacement after a 30-second seek. An independent generated-media regression reproduced the failure with the production input, mapping, encoding, threading and HLS argument builders on FFmpeg 8.1.2. The input has audio at index 0, video at index 1 and selected audio at index 2; an eight-second master is copied into a bounded 600-second source.

With encoded video and copied AAC audio, the first audio packet starts at 0 seconds while the first decoded video frame starts at **1.926025 seconds**. Selected audio measures **879.84 Hz** against the expected 880 Hz. The issue is timeline alignment, not selection of the wrong audio track. The original 250 ms acceptance threshold and 30-second target remain unchanged.

## Official sources and decision

The [FFmpeg manual](https://www.ffmpeg.org/ffmpeg.html) distinguishes input seeking from output trimming. Accurate input seeking discards decoded preroll for transcoded streams, while copied streams can retain it. A common muxer timeline can therefore expose retained audio before the accurately sought video.

The [FFmpeg 8.1 stream-copy implementation](https://www.ffmpeg.org/doxygen/8.1/ffmpeg__mux_8c_source.html) distinguishes an omitted output start time from an explicit zero. Explicit output `-ss 0` enables the initial packet boundary check without introducing a nonzero timestamp offset. [Output configuration](https://www.ffmpeg.org/doxygen/8.1/ffmpeg__mux__init_8c_source.html) and the [filter implementation](https://www.ffmpeg.org/doxygen/8.1/ffmpeg__filter_8c_source.html) describe the matching encoded-stream trim boundary. These official URLs were discovered through connected research tools on 17 October 2026; the measured runtime is 8.1.2.

The selected design appends output `-ss 0` after the input arguments only when the source seek is positive, video is encoded and audio is copied. Keep the input seek, requested absolute source offset, codec choices, stream indices, processing actions, hardware choice and recovery budget unchanged. This is a shared argument-builder rule used by startup/restart and the regression fixture.

| Option | Benefit | Cost or boundary |
| --- | --- | --- |
| Scoped output trim | Retains copied audio and the common timeline without another encoder | Drops preroll before the requested boundary; codec-specific delay still needs qualification |
| Re-encode audio for every seek | Gives both streams decoded trim boundaries | Adds CPU and possible quality loss; changes the preserved processing plan |
| Independently reset each stream's timestamps | Can make each first timestamp zero | Can erase a legitimate relative offset and conceal synchronization errors |
| Trim copied video with the same rule | Simple universal argument rule | Can discard its earlier keyframe and postpone video until a later keyframe; does not establish accurate copied-video seeking |

The recommendation is the scoped trim within the existing sandboxed FFmpeg plan, generation replacement and source-offset contract. No new codec, filter, network protocol, filesystem access or dependency is introduced. Negative decode timestamps can be legitimate reorder state; acceptance must inspect presentation, selected audio and actual decoding rather than require every DTS to be nonnegative.

## Qualification and outcome

Unit coverage must include absent, zero, negative and positive seeks, all stream-action combinations and argument ordering after `-i`. The real regression retains the 250 ms first-video bound, verifies selected 880 Hz audio, compares leading audio/video presentation timing and checks the unchanged source hash. A video-only reference with the same input seek and encoder settings checks first-frame pixel preservation. Its constant-red source does not independently prove absolute content position; the live corpus needs visible frame identifiers for that separate gate.

The uncorrected regression failed as described above. The corrected production builder passes against FFmpeg 8.1.2: first decoded video PTS **0.083008 seconds**, first audio PTS **0.098 seconds**, video-minus-audio **−0.014992 seconds**, selected audio **879.55 Hz**, and matching first-frame hashes `f8f8328e55ac1f810afa3aa65bacd374`. Source immutability and the original bounds pass. A small negative video DTS remains valid reorder state. The authenticated current-source HTTP contract also passes: all four progressive generations and all three recovery/seek generations start at 0.083008 seconds, with physical worker/cache cleanup confirmed. Both the HTTP contract and this focused regression passed again against final no-cache production image `sha256:f45eda1c8bbe3a9685196aae1200543601ec5efc2fb0f858ca863dd9d42ae8dc`; the focused case completed in 16.87 seconds. The deployed image evaluation is recorded in [Playback recovery](PLAYBACK_RECOVERY.md#acceptance-and-outcome). Logs are retained under ignored `.cache/local-evaluation/recovery-seek-*` and `.cache/testing-memory/playback-recovery-*`.

The measured AAC route does not qualify every copied-audio codec. Copied-video keyframe accuracy, arbitrary corruption, physical HDR fidelity and actual browser/desktop source-content positioning remain separate qualification work.

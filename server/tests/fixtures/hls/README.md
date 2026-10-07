# Real FFmpeg HLS fixtures

The two checked-in text fixtures contain actual FFmpeg 8.1.2 output from the Tonight 20-second H.264/AAC clip, with terminal blank lines normalized to a single LF newline. Binary initialization and media fragments stay under ignored `.cache/`. The `720p` variant name is a routing label; the fixture's actual video resolution is 320×180, as its manifest states.

Run these PowerShell commands from the repository root using an existing `duskcue:local` image. No image or binary is downloaded implicitly.

```powershell
node --input-type=module -e "import { ensurePlaybackMedia } from './clients/web/tests/fixtures/playback-media.mjs'; await ensurePlaybackMedia();"
New-Item -ItemType Directory -Path .cache/tonight-hls-server/single,.cache/tonight-hls-server/master/720p -Force | Out-Null
$sourceDirectory = (Resolve-Path .cache/tonight-web/media).Path
$fixtureDirectory = (Resolve-Path .cache/tonight-hls-server).Path
$dockerArgs = @('run','--rm','--pull=never','--entrypoint','ffmpeg','--mount',"type=bind,source=$sourceDirectory,target=/source,readonly",'--mount',"type=bind,source=$fixtureDirectory,target=/fixtures",'duskcue:local')
$outputArgs = @('-hide_banner','-loglevel','error','-i','/source/standard.mp4','-map','0:v:0','-map','0:a:0','-c','copy','-f','hls','-hls_time','4','-hls_segment_type','fmp4','-hls_fmp4_init_filename','init.mp4','-hls_list_size','0','-hls_playlist_type','vod')
docker @dockerArgs @outputArgs -hls_segment_filename /fixtures/single/seg_%04d.m4s -y /fixtures/single/manifest.m3u8
docker @dockerArgs @outputArgs -var_stream_map 'v:0,a:0,name:720p' -master_pl_name manifest.m3u8 -hls_segment_filename /fixtures/master/%v/seg_%04d.m4s -y /fixtures/master/%v/index.m3u8
```

Use an isolated PostgreSQL 18 database named `duskcue_transcode_test...`. The test applies the repository migrations. Its explicit fixture opt-in and database-name check reject ordinary application databases. Set the connection URL for that disposable database, then run:

```powershell
$env:DUSKCUE_DATABASE_URL = '<disposable PostgreSQL connection URL>'
$env:DUSKCUE_TRANSCODE_ACCESS_TESTS = 'disposable'
$env:DUSKCUE_HLS_TEST_MEDIA_DIR = $fixtureDirectory
cargo test -p duskcue domains::playback::hls --lib --locked -- --include-ignored --nocapture
cargo test -p duskcue --test transcode_profile_access --locked -- --ignored --nocapture
```

The HTTP test exercises the actual router, authentication, profile/access checks, migrated database and real bytes on disk. A narrow `cfg(test)` helper registers the cached transcode sessions. This test does not start `TranscodeManager`'s FFmpeg worker or a real browser/WebView; those remain separate qualification boundaries described in [streaming authentication](../../../../docs/design/STREAMING_AUTHENTICATION.md).

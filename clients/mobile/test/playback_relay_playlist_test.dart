// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import 'dart:convert';
import 'package:flutter_test/flutter_test.dart';
import 'package:duskcue_mobile/playback/commands/wire.dart';
import 'package:duskcue_mobile/playback/relay/playlist.dart';
import 'relay_fixture.dart';

void main() {
  test(
    'exact HLS grammar refuses unsupported tags and malformed attributes atomically',
    () {
      final playlists = RelayPlaylists(relayPrefix, '/secret/');
      playlists.rewrite(utf8.encode(mediaPlaylist), 'manifest.m3u8');
      for (final invalid in [
        mediaPlaylist.replaceFirst('MAP:URI=', 'MAP:XURI='),
        mediaPlaylist.replaceFirst('MAP:URI=', 'MAP:URI="wrong",URI='),
        mediaPlaylist.replaceFirst('MAP:URI=', 'MAP:BYTERANGE="2@0",URI='),
        mediaPlaylist.replaceFirst(
          '#EXTINF:2,',
          '#EXT-X-KEY:METHOD=NONE\n#EXTINF:2,',
        ),
        mediaPlaylist.replaceFirst(
          '#EXTINF:2,',
          '#EXT-X-BYTERANGE:2@0\n#EXTINF:2,',
        ),
        mediaPlaylist.replaceFirst('#EXTINF:2,', '#EXTINF:NaN,'),
        mediaPlaylist.replaceFirst('#EXTINF:2,', '#EXTINF:0,'),
        mediaPlaylist.replaceFirst('#EXTINF:2,', '#EXTINF:2,\n#EXTINF:2,'),
        mediaPlaylist.replaceFirst(
          '#EXTINF:2,',
          '#EXT-X-SESSION-DATA:URI="https://canary.invalid"\n#EXTINF:2,',
        ),
        mediaPlaylist.replaceFirst(
          '#EXT-X-ENDLIST',
          '#EXT-X-ENDLIST\n#EXT-X-ENDLIST',
        ),
        mediaPlaylist.replaceFirst(
          '#EXT-X-TARGETDURATION:2',
          '#EXT-X-TARGETDURATION:2\n#EXT-X-TARGETDURATION:2',
        ),
        mediaPlaylist.substring(0, mediaPlaylist.length - 1),
      ]) {
        expect(
          () => playlists.rewrite(utf8.encode(invalid), 'manifest.m3u8'),
          throwsA(isA<VodFailure>()),
        );
        expect(playlists.revision, 1);
        expect(playlists.permits('v/seg_0.m4s'), isTrue);
      }
      for (final attributes in [
        'XURI="https://canary.invalid",BANDWIDTH=1',
        'BANDWIDTH=1,BANDWIDTH=2',
        'BANDWIDTH=1,',
        'BANDWIDTH=1,CODECS="avc1.64001f,mp4a.40.2",URI="bad"',
      ]) {
        expect(
          () => playlists.rewrite(
            utf8.encode(
              '#EXTM3U\n#EXT-X-STREAM-INF:$attributes\n${relayPrefix}v/index.m3u8\n',
            ),
            'manifest.m3u8',
          ),
          throwsA(isA<VodFailure>()),
        );
      }
    },
  );

  test('root replacement fences late variants and reference capacity is atomic', () {
    final playlists = RelayPlaylists(relayPrefix, '/secret/');
    String master(String rendition) =>
        '#EXTM3U\n#EXT-X-STREAM-INF:BANDWIDTH=1\n$relayPrefix$rendition/index.m3u8\n';
    playlists.rewrite(utf8.encode(master('v')), 'manifest.m3u8');
    final ticket = playlists.revision;
    playlists.rewrite(utf8.encode(master('other')), 'manifest.m3u8');
    expect(
      () => playlists.rewrite(
        utf8.encode(mediaPlaylist),
        'v/index.m3u8',
        revision: ticket,
      ),
      throwsA(isA<VodFailure>()),
    );
    expect(playlists.permits('v/init.mp4'), isFalse);
    final tooMany =
        '#EXTM3U\n${List.generate(17, (i) => '#EXT-X-STREAM-INF:BANDWIDTH=1\n${relayPrefix}v$i/index.m3u8\n').join()}';
    expect(
      () => playlists.rewrite(utf8.encode(tooMany), 'manifest.m3u8'),
      throwsA(isA<VodFailure>()),
    );
    final large =
        '#EXTM3U\n#EXT-X-TARGETDURATION:2\n#EXT-X-MAP:URI="${relayPrefix}v/init.mp4"\n${List.generate(8192, (i) => '#EXTINF:2,\n${relayPrefix}v/seg_$i.m4s\n').join()}';
    expect(
      () => playlists.rewrite(utf8.encode(large), 'manifest.m3u8'),
      throwsA(isA<VodFailure>()),
    );
    expect(playlists.permits('other/index.m3u8'), isTrue);
    expect(playlists.permits('v/seg_0.m4s'), isFalse);
    final small = RelayPlaylists('/x/', '/secret/');
    final exhausted =
        '#EXTM3U\n#EXT-X-TARGETDURATION:2\n#EXT-X-MAP:URI="/x/v/init.mp4"\n${List.generate(8192, (i) => '#EXTINF:2,\n/x/v/seg_$i.m4s\n').join()}';
    expect(utf8.encode(exhausted).length, lessThan(1048576));
    expect(
      () => small.rewrite(utf8.encode(exhausted), 'manifest.m3u8'),
      throwsA(isA<VodFailure>()),
    );
    expect(small.permits('v/init.mp4'), isFalse);
  });
}

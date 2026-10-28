// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import 'dart:convert';
import 'dart:io';
import 'package:flutter_test/flutter_test.dart';
import 'package:duskcue_mobile/playback/commands/wire.dart';
import 'package:duskcue_mobile/playback/relay/playlist.dart';
import 'package:duskcue_mobile/playback/relay/request.dart';
import 'package:duskcue_mobile/playback/relay/service.dart';
import 'package:duskcue_mobile/playback/relay/upstream.dart';

const prefix =
    '/api/v1/playback-commands/019a2b00-0000-7000-8000-000000000001/outputs/019a2b00-0000-7000-8000-000000000002/hls/';

Future<String> fetch(Uri uri, {Map<String, String> headers = const {}}) async {
  final client = HttpClient()..autoUncompress = false;
  try {
    final request = await client.getUrl(uri);
    headers.forEach(request.headers.set);
    final response = await request.close();
    final body = await utf8.decoder.bind(response).join();
    return '${response.statusCode}:$body';
  } finally {
    client.close(force: true);
  }
}

void main() {
  test(
    'narrow parser rejects credentials, aliases, framing, duplicates and pipelines',
    () {
      const valid =
          'GET /secret/manifest.m3u8 HTTP/1.1\r\nHost: 127.0.0.1:1234\r\n\r\n';
      RelayRequest parse(String value) => RelayRequest.parse(
        ascii.encode(value),
        host: '127.0.0.1:1234',
        capability: 'secret',
      );
      expect(parse(valid).path, 'manifest.m3u8');
      for (final bad in [
        valid.replaceFirst('secret/', 'secret/%6d'),
        valid.replaceFirst('secret/', 'secret/../'),
        valid.replaceFirst('GET', 'POST'),
        valid.replaceFirst('HTTP/1.1', 'HTTP/1.0'),
        valid.replaceFirst('manifest.m3u8', 'manifest.m3u8?q=x'),
        valid.replaceFirst('Host:', 'Cookie: forbidden\r\nHost:'),
        valid.replaceFirst('Host:', 'Authorization: Bearer forbidden\r\nHost:'),
        valid.replaceFirst('Host:', 'Content-Length: 0\r\nHost:'),
        valid.replaceFirst('Host:', 'Transfer-Encoding: chunked\r\nHost:'),
        valid.replaceFirst('Host:', 'Host: 127.0.0.1:1234\r\nHost:'),
        '$valid$valid',
      ]) {
        expect(
          () => parse(bad),
          throwsA(isA<VodFailure>()),
          reason: bad.split('\r\n').first,
        );
      }
    },
  );
  test(
    'playlist validation is atomic and binds references to exact output and rendition',
    () {
      final playlists = RelayPlaylists(prefix, '/secret/');
      final master = utf8.encode(
        '#EXTM3U\n#EXT-X-STREAM-INF:BANDWIDTH=1\n${prefix}v/index.m3u8\n',
      );
      expect(
        utf8.decode(playlists.rewrite(master, 'manifest.m3u8')),
        contains('/secret/v/index.m3u8'),
      );
      expect(playlists.permits('v/index.m3u8'), isTrue);
      for (final target in [
        'https://canary.invalid/key',
        '${prefix}other/init.mp4',
        '${prefix}v/../init.mp4',
      ]) {
        expect(
          () => playlists.rewrite(
            utf8.encode('#EXTM3U\n#EXT-X-MAP:URI="$target"\n'),
            'v/index.m3u8',
          ),
          throwsA(isA<VodFailure>()),
        );
      }
      expect(
        () => playlists.rewrite(
          utf8.encode('#EXTM3U\n#EXT-X-KEY:METHOD=AES-128\n'),
          'v/index.m3u8',
        ),
        throwsA(isA<VodFailure>()),
      );
      expect(playlists.permits('v/init.mp4'), isFalse);
      playlists.rewrite(
        utf8.encode(
          '#EXTM3U\n#EXT-X-MAP:URI="${prefix}v/init.mp4"\n#EXTINF:2,\n${prefix}v/seg_0.m4s\n',
        ),
        'v/index.m3u8',
      );
      expect(playlists.permits('v/init.mp4'), isTrue);
      expect(playlists.permits('v/seg_1.m4s'), isFalse);
    },
  );
  test('partial responses must match the requested byte range exactly', () {
    expect(validRange('bytes 10-19/100', 'bytes=10-19', 10), isTrue);
    expect(validRange('bytes 10-20/100', 'bytes=10-19', 11), isFalse);
    expect(validRange('bytes 90-99/100', 'bytes=-10', 10), isTrue);
    expect(validRange('bytes 1-9/100', 'bytes=-10', 9), isFalse);
  });
  test(
    'real loopback rewrites all resources and sends bearer only to exact upstream',
    () async {
      final server = await HttpServer.bind(InternetAddress.loopbackIPv4, 0);
      final seen = <String>[];
      server.listen((request) async {
        seen.add(request.uri.path);
        expect(
          request.headers.value('authorization'),
          'Bearer private-fixture-token',
        );
        expect(request.headers.value('x-session-context'), 'test-context');
        request.response.headers.contentType = ContentType(
          'application',
          'vnd.apple.mpegurl',
        );
        if (request.uri.path.endsWith('manifest.m3u8')) {
          request.response.write(
            '#EXTM3U\n#EXT-X-STREAM-INF:BANDWIDTH=1\n${prefix}v/index.m3u8\n',
          );
        } else if (request.uri.path.endsWith('index.m3u8')) {
          request.response.write(
            '#EXTM3U\n#EXT-X-MAP:URI="${prefix}v/init.mp4"\n#EXTINF:2,\n${prefix}v/seg_0.m4s\n#EXT-X-ENDLIST\n',
          );
        } else {
          request.response.add([1, 2, 3]);
        }
        await request.response.close();
      });
      var current = true;
      final relay = await PlaybackRelay.bind(
        origin: Uri.parse('http://127.0.0.1:${server.port}'),
        prefix: prefix,
        token: 'private-fixture-token',
        context: 'test-context',
        current: () => current,
      );
      try {
        final master = await fetch(relay.uri);
        expect(master, startsWith('200:'));
        expect(master, contains('/${relay.capability}/v/index.m3u8'));
        expect(master, isNot(contains('private-fixture-token')));
        expect(
          await fetch(relay.uri.resolve('v/index.m3u8')),
          startsWith('200:'),
        );
        expect(
          await fetch(relay.uri.resolve('v/seg_0.m4s')),
          startsWith('200:'),
        );
        final count = seen.length;
        expect(
          await fetch(relay.uri, headers: {'Authorization': 'Bearer attacker'}),
          startsWith('503:'),
        );
        expect(seen.length, count);
        current = false;
        await expectLater(fetch(relay.uri), throwsA(isA<HttpException>()));
        expect(seen.length, count);
      } finally {
        await relay.close();
        await server.close(force: true);
      }
      expect(relay.sockets, 0);
      expect(relay.upstream, 0);
    },
  );
  test('upstream redirect and unsupported coding never reach canary', () async {
    final canary = await HttpServer.bind(InternetAddress.loopbackIPv4, 0),
        server = await HttpServer.bind(InternetAddress.loopbackIPv4, 0);
    var hits = 0, coding = false;
    canary.listen((request) async {
      hits++;
      await request.response.close();
    });
    server.listen((request) async {
      if (!coding) {
        request.response.statusCode = 302;
        request.response.headers.set(
          'location',
          'http://127.0.0.1:${canary.port}/secret',
        );
      } else {
        request.response.headers.set('content-encoding', 'gzip');
        request.response.write('invalid');
      }
      await request.response.close();
    });
    final relay = await PlaybackRelay.bind(
      origin: Uri.parse('http://127.0.0.1:${server.port}'),
      prefix: prefix,
      token: 'canary-secret',
      context: 'context',
      current: () => true,
    );
    try {
      expect(await fetch(relay.uri), startsWith('503:'));
      expect(hits, 0);
      coding = true;
      expect(await fetch(relay.uri), startsWith('503:'));
      expect(hits, 0);
    } finally {
      await relay.close();
      await server.close(force: true);
      await canary.close(force: true);
    }
  });
  test(
    'incomplete headers consume finite sockets and closure observes them',
    () async {
      final relay = await PlaybackRelay.bind(
        origin: Uri.parse('http://127.0.0.1:1'),
        prefix: prefix,
        token: 'unused',
        context: 'context',
        current: () => true,
      );
      final sockets = <Socket>[];
      try {
        for (var index = 0; index < 8; index++) {
          final socket = await Socket.connect(
            InternetAddress.loopbackIPv4,
            relay.uri.port,
          );
          socket.listen((_) {}, onError: (Object _) {});
          socket.write('GET /');
          sockets.add(socket);
        }
        await Future<void>.delayed(const Duration(milliseconds: 30));
        expect(relay.sockets, 8);
        expect(relay.upstream, 0);
        await relay.close();
        expect(relay.sockets, 0);
      } finally {
        for (final socket in sockets) {
          socket.destroy();
        }
        await relay.close();
      }
    },
  );
}

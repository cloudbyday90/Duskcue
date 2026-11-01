// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import 'dart:async';
import 'dart:convert';
import 'dart:io';
import 'package:flutter_test/flutter_test.dart';
import 'relay_fixture.dart';

void main() {
  test(
    'a late HTTP variant cannot publish evidence from an older root revision',
    () async {
      final f = await RelayFixture.create();
      final started = Completer<void>(), release = Completer<void>();
      var roots = 0;
      f.handler = (request) async {
        request.response.headers.set(
          'Content-Type',
          'application/vnd.apple.mpegurl',
        );
        if (request.uri.path.endsWith('manifest.m3u8')) {
          roots++;
          request.response.write(
            '#EXTM3U\n#EXT-X-STREAM-INF:BANDWIDTH=$roots\n${relayPrefix}v/index.m3u8\n',
          );
        } else {
          if (!started.isCompleted) {
            started.complete();
            await release.future;
          }
          request.response.write(mediaPlaylist);
        }
        await request.response.close();
      };
      try {
        await f.publish();
        final stale = rawFetch(f.relay.uri.resolve('v/index.m3u8'));
        await started.future;
        await f.publish();
        release.complete();
        expect(await stale, startsWith('HTTP/1.1 503'));
        expect(f.relay.playlists.permits('v/init.mp4'), isFalse);
        expect(
          await rawFetch(f.relay.uri.resolve('v/index.m3u8')),
          startsWith('HTTP/1.1 200'),
        );
        expect(f.relay.playlists.permits('v/init.mp4'), isTrue);
      } finally {
        if (!release.isCompleted) release.complete();
        await f.close();
      }
    },
  );
  test(
    'eighth accepted socket pauses admission; EOF resumes without losing a close',
    () async {
      final f = await RelayFixture.create();
      final sockets = <Socket>[];
      try {
        for (var index = 0; index < 9; index++) {
          final socket = await Socket.connect('127.0.0.1', f.relay.uri.port);
          socket.listen((_) {}, onError: (Object _) {});
          socket.write('GET /');
          sockets.add(socket);
        }
        await eventually(() => f.relay.sockets == 8);
        await Future<void>.delayed(const Duration(milliseconds: 100));
        expect(f.relay.sockets, 8);
        expect(f.requests, isEmpty);
        sockets.first.destroy();
        await Future<void>.delayed(const Duration(milliseconds: 100));
        expect(f.relay.sockets, 8);
        await f.close();
        expect(f.relay.sockets, 0);
      } finally {
        for (final socket in sockets) {
          socket.destroy();
        }
        await f.close();
      }
    },
  );

  test(
    'four upstream obligations refuse a fifth, and local EOF cancels pending headers',
    () async {
      final f = await RelayFixture.create();
      final sockets = <Socket>[];
      final release = Completer<void>();
      f.handler = (request) async {
        await release.future;
        await request.response.close();
      };
      try {
        for (var index = 0; index < 4; index++) {
          final socket = await Socket.connect('127.0.0.1', f.relay.uri.port);
          socket.listen((_) {}, onError: (Object _) {});
          socket.write(
            'GET ${f.relay.uri.path} HTTP/1.1\r\nHost: 127.0.0.1:${f.relay.uri.port}\r\n\r\n',
          );
          sockets.add(socket);
        }
        await eventually(() => f.requests.length == 4);
        expect(f.relay.upstream, 4);
        expect(await rawFetch(f.relay.uri), startsWith('HTTP/1.1 503'));
        expect(f.requests.length, 4);
        sockets.first.destroy();
        await eventually(() => f.relay.upstream == 3);
        await f.relay.close().timeout(const Duration(seconds: 3));
        expect(f.relay.upstream, 0);
      } finally {
        release.complete();
        for (final socket in sockets) {
          socket.destroy();
        }
        await f.close();
      }
    },
  );

  test(
    'header byte bound and late pipelining cannot dispatch another request',
    () async {
      final f = await RelayFixture.create();
      try {
        final base =
            'GET ${f.relay.uri.path} HTTP/1.1\r\nHost: 127.0.0.1:${f.relay.uri.port}\r\n';
        expect(
          await rawFetch(
            f.relay.uri,
            request: '${base}X: ${'a' * 8192}\r\n\r\n',
          ),
          isEmpty,
        );
        expect(f.requests, isEmpty);
        expect(
          await rawFetch(f.relay.uri, request: '$base\r\n$base\r\n'),
          startsWith('HTTP/1.1 503'),
        );
        expect(f.requests, isEmpty);
        final blocked = Completer<void>();
        f.handler = (request) async {
          await blocked.future;
          await request.response.close();
        };
        final socket = await Socket.connect('127.0.0.1', f.relay.uri.port);
        socket.listen((_) {}, onError: (Object _) {});
        socket.write('$base\r\n');
        await eventually(() => f.requests.length == 1);
        socket.write('G');
        await eventually(() => f.relay.upstream == 0);
        blocked.complete();
        socket.destroy();
        expect(f.requests.length, 1);
      } finally {
        await f.close();
      }
    },
  );

  test(
    'trickled local headers have one absolute five-second deadline',
    () async {
      final f = await RelayFixture.create();
      final socket = await Socket.connect('127.0.0.1', f.relay.uri.port);
      final done = Completer<void>();
      socket.listen(
        (_) {},
        onDone: done.complete,
        onError: (Object _) {
          if (!done.isCompleted) done.complete();
        },
      );
      final watch = Stopwatch()..start();
      final timer = Timer.periodic(
        const Duration(milliseconds: 300),
        (_) => socket.write('G'),
      );
      try {
        await done.future.timeout(const Duration(seconds: 7));
        expect(watch.elapsedMilliseconds, inInclusiveRange(4500, 6800));
        expect(f.requests, isEmpty);
      } finally {
        timer.cancel();
        socket.destroy();
        await f.close();
      }
    },
  );

  test(
    'scope invalidation closes a stalled upstream without new bytes',
    () async {
      final f = await RelayFixture.create();
      final release = Completer<void>();
      f.handler = (request) async {
        await release.future;
        await request.response.close();
      };
      final read = rawFetch(f.relay.uri);
      try {
        await eventually(() => f.relay.upstream == 1);
        f.current = false;
        expect(await read.timeout(const Duration(seconds: 2)), isEmpty);
        await f.relay.close();
        expect(f.relay.upstream, 0);
        expect(f.relay.sockets, 0);
      } finally {
        release.complete();
        await f.close();
      }
    },
  );

  test(
    'binary responses stream exact ranges, chunk finality, and canonical headers',
    () async {
      final f = await RelayFixture.create();
      try {
        await f.publish();
        f.handler = (request) async {
          request.response.statusCode = 206;
          request.response.headers.set('Content-Type', 'video/iso.segment');
          request.response.headers.set('Content-Range', 'bytes 10-19/100');
          request.response.headers.set(
            'X-Private-Upstream',
            'must-not-forward',
          );
          request.response.contentLength = 10;
          request.response.add(List.filled(10, 65));
          await request.response.close();
        };
        final uri = f.relay.uri.resolve('v/seg_0.m4s');
        final partial = await rawFetch(
          uri,
          request:
              'GET ${uri.path} HTTP/1.1\r\nHost: 127.0.0.1:${uri.port}\r\nRange: bytes=10-19\r\n\r\n',
        );
        expect(partial, startsWith('HTTP/1.1 206'));
        expect(partial, contains('Content-Range: bytes 10-19/100'));
        expect(partial, endsWith('A' * 10));
        expect(partial, isNot(contains('must-not-forward')));
        f.handler = (request) async {
          request.response.headers.set('Content-Type', 'video/iso.segment');
          request.response.add(utf8.encode('body'));
          await request.response.close();
        };
        final chunked = await rawFetch(uri);
        expect(chunked, contains('Transfer-Encoding: chunked'));
        expect(chunked, endsWith('0\r\n\r\n'));
        f.handler = (request) async {
          request.response.headers.set('Content-Type', 'video/mp4');
          request.response.add([1]);
          await request.response.close();
        };
        expect(await rawFetch(uri), startsWith('HTTP/1.1 503'));
      } finally {
        await f.close();
      }
    },
  );
}

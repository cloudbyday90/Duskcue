// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import 'dart:async';
import 'dart:convert';
import 'package:flutter_test/flutter_test.dart';
import 'relay_fixture.dart';

void main() {
  test(
    'sixty-second body ceiling cancels a finite stream that never becomes idle',
    () async {
      final f = await RelayFixture.create();
      try {
        await f.publish();
        f.handler = (request) async {
          request.response.headers.set('Content-Type', 'video/iso.segment');
          request.response.bufferOutput = false;
          await request.response.addStream(
            Stream<List<int>>.periodic(
              const Duration(milliseconds: 500),
              (_) => [65],
            ).take(130),
          );
          await request.response.close();
        };
        final watch = Stopwatch()..start();
        final result = await rawFetch(
          f.relay.uri.resolve('v/seg_0.m4s'),
          timeout: const Duration(seconds: 70),
        );
        expect(watch.elapsedMilliseconds, inInclusiveRange(59000, 64000));
        expect(result, startsWith('HTTP/1.1 200'));
        expect(result, isNot(endsWith('0\r\n\r\n')));
        expect(f.relay.upstream, 0);
      } finally {
        await f.close();
      }
    },
    timeout: const Timeout(Duration(seconds: 75)),
  );

  test(
    'SDK framing and binary admission reject malformed, oversized and truncated evidence',
    () async {
      final f = await RelayFixture.create();
      try {
        await f.publish();
        var reply = '';
        f.handler = (request) async {
          final socket = await request.response.detachSocket(
            writeHeaders: false,
          );
          socket.add(latin1.encode(reply));
          await socket.close();
          socket.destroy();
        };
        final uri = f.relay.uri.resolve('v/seg_0.m4s');
        for (final headers in [
          'Content-Type: text/html\r\nContent-Length: 1',
          'Content-Length: 1',
          'Content-Type: video/iso.segment\r\nContent-Encoding: gzip\r\nContent-Length: 1',
          'Content-Type: video/iso.segment\r\nContent-Length: 16777217',
          'Content-Type: video/iso.segment\r\nContent-Length: 1\r\nContent-Length: 2',
          'Content-Type: video/iso.segment\r\nContent-Range: bytes 0-0/1\r\nContent-Length: 1',
        ]) {
          reply = 'HTTP/1.1 200 OK\r\n$headers\r\nConnection: close\r\n\r\nx';
          expect(
            await rawFetch(uri),
            startsWith('HTTP/1.1 503'),
            reason: headers,
          );
        }
        reply =
            'HTTP/1.1 200 OK\r\nContent-Type: video/iso.segment\r\nContent-Length: 999\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n1\r\nx\r\n0\r\n\r\n';
        final normalized = await rawFetch(uri);
        expect(normalized, contains('Transfer-Encoding: chunked'));
        expect(normalized, isNot(contains('Content-Length: 999')));
        expect(normalized, endsWith('0\r\n\r\n'));
        for (final body in [
          '4\r\nab',
          '4\r\nbody\r\n',
          'x\r\ninvalid\r\n0\r\n\r\n',
        ]) {
          reply =
              'HTTP/1.1 200 OK\r\nContent-Type: video/iso.segment\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n$body';
          final local = await rawFetch(uri);
          expect(local, isNot(endsWith('0\r\n\r\n')));
        }
        reply =
            'HTTP/1.1 200 OK\r\nContent-Type: video/iso.segment\r\nContent-Length: 9\r\nConnection: close\r\n\r\nshort';
        final truncated = await rawFetch(uri);
        expect(truncated, contains('Content-Length: 9'));
        expect(truncated.split('\r\n\r\n').last.length, lessThan(9));
      } finally {
        await f.close();
      }
    },
  );

  test(
    'pending upstream headers and body idle have independent absolute deadlines',
    () async {
      final f = await RelayFixture.create();
      final release = Completer<void>();
      try {
        f.handler = (request) async {
          await release.future;
          await request.response.close();
        };
        final watch = Stopwatch()..start();
        expect(await rawFetch(f.relay.uri), isEmpty);
        expect(watch.elapsedMilliseconds, inInclusiveRange(4500, 6800));
        release.complete();
        f.handler = f.playlist;
        await f.publish();
        final idleRelease = Completer<void>();
        f.handler = (request) async {
          request.response.headers.set('Content-Type', 'video/iso.segment');
          request.response.write('body');
          await request.response.flush();
          await idleRelease.future;
          await request.response.close();
        };
        watch.reset();
        try {
          final local = await rawFetch(f.relay.uri.resolve('v/seg_0.m4s'));
          expect(local, contains('Transfer-Encoding: chunked'));
          expect(local, isNot(endsWith('0\r\n\r\n')));
          expect(watch.elapsedMilliseconds, inInclusiveRange(9500, 12500));
          expect(f.relay.upstream, 0);
        } finally {
          idleRelease.complete();
        }
      } finally {
        if (!release.isCompleted) release.complete();
        await f.close();
      }
    },
  );
}

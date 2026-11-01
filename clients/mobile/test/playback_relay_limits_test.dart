// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import 'dart:async';
import 'dart:io';
import 'dart:typed_data';
import 'package:flutter_test/flutter_test.dart';
import 'relay_fixture.dart';

void main() {
  test(
    'unknown-length binary overflow stays incomplete with at most sixteen MiB adopted',
    () async {
      final f = await RelayFixture.create();
      final client = HttpClient();
      try {
        await f.publish();
        f.handler = (request) async {
          request.response.headers.set('Content-Type', 'video/iso.segment');
          await request.response.addStream(
            Stream<List<int>>.fromIterable(
              Iterable.generate(257, (_) => Uint8List(65536)),
            ),
          );
          await request.response.close();
        };
        final request = await client.getUrl(f.relay.uri.resolve('v/seg_0.m4s'));
        final response = await request.close();
        expect(response.statusCode, 200);
        final done = Completer<void>();
        var bytes = 0, incomplete = false;
        response.listen(
          (part) {
            bytes += part.length;
          },
          onError: (Object _) {
            incomplete = true;
            if (!done.isCompleted) done.complete();
          },
          onDone: () {
            if (!done.isCompleted) done.complete();
          },
        );
        await done.future.timeout(const Duration(seconds: 10));
        expect(incomplete, isTrue);
        expect(bytes, inInclusiveRange(1, 16777216));
        await eventually(() => f.relay.upstream == 0);
      } finally {
        client.close(force: true);
        await f.close();
      }
    },
  );

  test(
    'unknown-length playlist overflow exposes no metadata and preserves the last reference set',
    () async {
      final f = await RelayFixture.create();
      try {
        await f.publish();
        expect(f.relay.playlists.permits('v/init.mp4'), isTrue);
        final revision = f.relay.playlists.revision;
        f.handler = (request) async {
          request.response.headers.set(
            'Content-Type',
            'application/vnd.apple.mpegurl',
          );
          await request.response.addStream(
            Stream<List<int>>.fromIterable(
              Iterable.generate(17, (_) => Uint8List(65536)),
            ),
          );
          await request.response.close();
        };
        expect(await rawFetch(f.relay.uri), startsWith('HTTP/1.1 503'));
        expect(f.relay.playlists.revision, revision);
        expect(f.relay.playlists.permits('v/init.mp4'), isTrue);
      } finally {
        await f.close();
      }
    },
  );
}

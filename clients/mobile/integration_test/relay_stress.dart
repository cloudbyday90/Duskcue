// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import 'dart:async';
import 'dart:io';
import 'dart:typed_data';
import 'package:duskcue_mobile/playback/commands/wire.dart';
import 'package:duskcue_mobile/playback/relay/service.dart';
import '../test/relay_fixture.dart';

Future<void> nativeRelayStress(PlaybackRelay actual) async {
  final sockets = <Socket>[];
  try {
    await eventually(() => actual.sockets == 0);
    for (var i = 0; i < 9; i++) {
      final socket = await Socket.connect('127.0.0.1', actual.uri.port);
      socket.listen((_) {}, onError: (Object _) {});
      socket.write('GET /');
      sockets.add(socket);
    }
    await eventually(() => actual.sockets == 8);
    await Future<void>.delayed(const Duration(milliseconds: 100));
    require(actual.sockets == 8 && actual.upstream == 0);
    sockets.first.destroy();
    await Future<void>.delayed(const Duration(milliseconds: 100));
    require(actual.sockets == 8);
  } finally {
    for (final socket in sockets) {
      socket.destroy();
    }
    await eventually(() => actual.sockets == 0);
  }
  final f = await RelayFixture.create();
  Socket? slow;
  try {
    await f.publish();
    f.handler = (request) async {
      request.response.headers.set('Content-Type', 'video/iso.segment');
      request.response.bufferOutput = false;
      await request.response.addStream(
        Stream<List<int>>.fromIterable(
          Iterable.generate(256, (_) => Uint8List(65536)),
        ),
      );
      await request.response.close();
    };
    slow = await Socket.connect('127.0.0.1', f.relay.uri.port);
    slow.setRawOption(RawSocketOption.fromInt(1, 8, 4096));
    slow.write(
      'GET /${f.relay.capability}/v/seg_0.m4s HTTP/1.1\r\nHost: 127.0.0.1:${f.relay.uri.port}\r\n\r\n',
    );
    await slow.flush();
    await eventually(
      () => f.relay.upstream == 1 && f.relay.peakQueuedChunks > 0,
    );
    await Future<void>.delayed(const Duration(milliseconds: 250));
    require(f.relay.peakQueuedChunks <= 2);
    await f.relay.close().timeout(const Duration(seconds: 3));
    require(f.relay.sockets == 0 && f.relay.upstream == 0);
  } finally {
    slow?.destroy();
    await f.close();
  }
}

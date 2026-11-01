// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import 'dart:async';
import 'dart:convert';
import 'dart:io';
import 'package:duskcue_mobile/playback/relay/service.dart';

const relayPrefix =
    '/api/v1/playback-commands/019a2b00-0000-7000-8000-000000000001/outputs/019a2b00-0000-7000-8000-000000000002/hls/';
const mediaPlaylist =
    '#EXTM3U\n#EXT-X-VERSION:7\n#EXT-X-TARGETDURATION:2\n#EXT-X-PLAYLIST-TYPE:EVENT\n#EXT-X-MAP:URI="${relayPrefix}v/init.mp4"\n#EXTINF:2,\n${relayPrefix}v/seg_0.m4s\n#EXT-X-ENDLIST\n';

Future<void> eventually(bool Function() predicate) async {
  final end = DateTime.now().add(const Duration(seconds: 3));
  while (!predicate()) {
    if (DateTime.now().isAfter(end)) {
      throw StateError('Owned observation did not settle');
    }
    await Future<void>.delayed(const Duration(milliseconds: 10));
  }
}

Future<String> rawFetch(
  Uri uri, {
  String? request,
  Duration timeout = const Duration(seconds: 15),
}) async {
  final socket = await Socket.connect(uri.host, uri.port);
  final result = <int>[];
  final done = Completer<void>();
  socket.listen(
    result.addAll,
    onDone: done.complete,
    onError: done.completeError,
  );
  socket.add(
    ascii.encode(
      request ??
          'GET ${uri.path} HTTP/1.1\r\nHost: ${uri.host}:${uri.port}\r\n\r\n',
    ),
  );
  try {
    await done.future.timeout(timeout);
    return latin1.decode(result);
  } finally {
    socket.destroy();
  }
}

class RelayFixture {
  RelayFixture._(this.server);
  final HttpServer server;
  late final PlaybackRelay relay;
  var current = true;
  Future<void> Function(HttpRequest)? handler;
  final requests = <String>[];
  static Future<RelayFixture> create() async {
    final value = RelayFixture._(
      await HttpServer.bind(InternetAddress.loopbackIPv4, 0),
    );
    value.server.listen((request) async {
      value.requests.add(request.uri.path);
      try {
        if (value.handler case final handler?) {
          await handler(request);
        } else {
          await value.playlist(request);
        }
      } catch (_) {
        request.response.done.catchError((Object _) {});
      }
    });
    value.relay = await PlaybackRelay.bind(
      origin: Uri.parse('http://127.0.0.1:${value.server.port}'),
      prefix: relayPrefix,
      token: 'fixture-only-bearer',
      context: 'fixture-only-context',
      current: () => value.current,
    );
    return value;
  }

  Future<void> playlist(HttpRequest request) async {
    request.response.headers.set(
      'Content-Type',
      'application/vnd.apple.mpegurl',
    );
    request.response.write(mediaPlaylist);
    await request.response.close();
  }

  Future<void> publish() async {
    await rawFetch(relay.uri);
  }

  Future<void> close() async {
    await relay.close();
    await server.close(force: true);
  }
}

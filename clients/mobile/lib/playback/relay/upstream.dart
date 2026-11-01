// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import 'dart:async';
import 'dart:io';
import '../commands/wire.dart';
import 'connection.dart';
import 'request.dart';
import 'response.dart';
import 'service.dart';
import 'transport.dart';
export 'response.dart' show validRange;

Future<void> forward(
  PlaybackRelay owner,
  RelayConnection local,
  RelayRequest request,
) async {
  require(owner.clients.length < 4, 'relay_busy');
  final ticket = owner.playlists.revision;
  var cancelled = false;
  void active() => require(
    !cancelled &&
        !local.closed &&
        !owner.closing &&
        owner.current() &&
        owner.playlists.permits(request.path),
    'scope_changed',
  );
  final transport = RelayTransport(
    owner.origin.resolve('${owner.prefix}${request.path}'),
    active,
  );
  final client = HttpClient()
    ..autoUncompress = false
    ..findProxy = ((_) => 'DIRECT')
    ..connectionFactory = transport.factory
    ..connectionTimeout = const Duration(seconds: 5)
    ..maxConnectionsPerHost = 1;
  owner.clients.add(transport);
  HttpClientRequest? issued;
  void cancel() {
    if (cancelled) return;
    cancelled = true;
    issued?.abort(const VodFailure('socket_closed'));
    client.close(force: true);
    transport.cancel();
    local.cancel();
  }

  local.onCancel(cancel);
  var idle = Timer(const Duration(seconds: 5), cancel);
  Timer? body;
  void resetIdle() {
    idle.cancel();
    idle = Timer(const Duration(seconds: 10), cancel);
  }

  try {
    active();
    final upstream = await client.getUrl(transport.uri);
    issued = upstream;
    active();
    upstream.followRedirects = false;
    upstream.maxRedirects = 0;
    upstream.persistentConnection = false;
    upstream.headers.set('Authorization', 'Bearer ${owner.token}');
    upstream.headers.set('X-Session-Context', owner.context);
    upstream.headers.set('Accept-Encoding', 'identity');
    upstream.headers.set('Cache-Control', 'no-store');
    if (request.range != null) upstream.headers.set('Range', request.range!);
    final response = await upstream.close();
    active();
    final accepted = RelayResponse(response, request);
    resetIdle();
    body = Timer(const Duration(seconds: 60), cancel);
    if (accepted.binary) {
      await local.respond(
        response.statusCode,
        length: response.contentLength < 0 ? null : response.contentLength,
        type: accepted.type,
        contentRange: accepted.range,
      );
    }
    var count = 0;
    final bytes = <int>[];
    await for (final part in response) {
      active();
      resetIdle();
      count += part.length;
      require(count <= accepted.maximum);
      if (accepted.binary) {
        await local.body(part);
        active();
      } else {
        bytes.addAll(part);
      }
    }
    active();
    require(response.contentLength < 0 || count == response.contentLength);
    if (!accepted.binary) {
      final rewritten = owner.playlists.rewrite(
        bytes,
        request.path,
        revision: ticket,
      );
      await local.respond(200, type: accepted.type, length: rewritten.length);
      active();
      await local.body(rewritten);
    }
    active();
    await local.finish();
  } finally {
    idle.cancel();
    body?.cancel();
    local.removeCancel(cancel);
    client.close(force: true);
    try {
      await transport.close();
      owner.clients.remove(transport);
    } catch (_) {
      owner.quarantined = true;
      unawaited(owner.close().catchError((Object _) {}));
      rethrow;
    }
  }
}

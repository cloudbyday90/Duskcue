// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import 'dart:async';
import 'dart:convert';
import 'dart:io';
import 'dart:math';
import '../commands/wire.dart';
import 'connection.dart';
import 'playlist.dart';
import 'request.dart';
import 'upstream.dart';

class PlaybackRelay {
  PlaybackRelay._(
    this.origin,
    this.prefix,
    this.token,
    this.context,
    this.current,
    this._listener,
    this.capability,
  ) {
    playlists = RelayPlaylists(prefix, '/$capability/');
    _subscription = _listener.listen(
      _accept,
      onError: (Object _) {
        unawaited(close());
      },
    );
  }
  final Uri origin;
  final String prefix, token, context, capability;
  final bool Function() current;
  final RawServerSocket _listener;
  late final StreamSubscription<RawSocket> _subscription;
  late final RelayPlaylists playlists;
  final Map<RelayConnection, Future<void>> _connections = {};
  final Set<HttpClient> clients = {};
  bool closing = false;
  Future<void>? _completion;
  int get sockets => _connections.length;
  int get upstream => clients.length;
  Uri get uri =>
      Uri.parse('http://127.0.0.1:${_listener.port}/$capability/manifest.m3u8');
  static Future<PlaybackRelay> bind({
    required Uri origin,
    required String prefix,
    required String token,
    required String context,
    required bool Function() current,
  }) async {
    require(
      origin.origin == origin.toString() &&
          prefix.startsWith('/api/v1/playback-commands/') &&
          prefix.endsWith('/hls/'),
    );
    final parts = prefix.split('/');
    require(
      parts.length == 9 &&
          prefix ==
              commandPath(
                entity(parts[4], v7: true),
                '/outputs/${entity(parts[6], v7: true)}/hls/',
              ),
    );
    require(current(), 'scope_changed');
    final socket = await RawServerSocket.bind(
      InternetAddress.loopbackIPv4,
      0,
      backlog: 8,
      shared: false,
    );
    if (!current()) {
      await socket.close();
      throw const VodFailure('scope_changed');
    }
    final random = Random.secure();
    final cap = base64Url
        .encode(List.generate(32, (_) => random.nextInt(256)))
        .replaceAll('=', '');
    return PlaybackRelay._(
      origin,
      prefix,
      token,
      context,
      current,
      socket,
      cap,
    );
  }

  void _accept(RawSocket socket) {
    if (closing || !current() || sockets >= 8) {
      unawaited(socket.close());
      return;
    }
    final connection = RelayConnection(socket);
    final work = _serve(connection);
    _connections[connection] = work;
    work.then(
      (_) => _connections.remove(connection),
      onError: (Object _) => _connections.remove(connection),
    );
  }

  Future<void> _serve(RelayConnection connection) async {
    try {
      final request = RelayRequest.parse(
        await connection.headers,
        host: '127.0.0.1:${_listener.port}',
        capability: capability,
      );
      require(
        !closing && current() && playlists.permits(request.path),
        'scope_changed',
      );
      await forward(this, connection, request);
    } catch (_) {
      await connection.error();
    } finally {
      await connection.close();
    }
  }

  Future<void> close() => _completion ??= () async {
    closing = true;
    await _listener.close();
    await _subscription.cancel();
    for (final client in List.of(clients)) {
      client.close(force: true);
    }
    for (final connection in List.of(_connections.keys)) {
      connection.cancel();
    }
    await Future.wait(List.of(_connections.values));
    playlists.clear();
  }();
}

// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import 'dart:async';
import 'dart:io';
import '../commands/wire.dart';
import 'raw_io.dart';
import 'socket_adapter.dart';

typedef RelayConnect = Future<RelayDial> Function(String host, int port);

class RelayDial {
  RelayDial(this.socket, this.cancel);
  final Future<RawSocket> socket;
  final void Function() cancel;
}

class RelayTransport {
  RelayTransport(
    this.uri,
    this.active, {
    RelayConnect? connect,
    this.securityContext,
  }) : connect =
           connect ??
           ((host, port) async {
             final task = await RawSocket.startConnect(host, port);
             return RelayDial(task.socket, task.cancel);
           });
  final Uri uri;
  final void Function() active;
  final RelayConnect connect;
  final SecurityContext? securityContext;
  RelayDial? _task;
  RawSocket? _tcp;
  RawSecureSocket? _secure;
  RelayRawIo? _io;
  Future<Socket>? _opening;
  Future<void>? _closing;
  Future<RawSocket>? _tcpClosing;
  Future<RawSocket>? _secureClosing;
  bool cancelled = false;
  bool _handshaking = false;

  void _active() {
    require(!cancelled, 'socket_closed');
    active();
  }

  Future<ConnectionTask<Socket>> factory(
    Uri target,
    String? proxyHost,
    int? proxyPort,
  ) async {
    _active();
    require(
      target == uri &&
          proxyHost == null &&
          proxyPort == null &&
          _opening == null,
    );
    final opening = _open();
    _opening = opening;
    opening.then((_) {}, onError: (Object _) {});
    return ConnectionTask.fromSocket(opening, cancel);
  }

  Future<Socket> _open() async {
    try {
      final task = await connect(uri.host, uri.port);
      _task = task;
      if (cancelled) task.cancel();
      final raw = await task.socket;
      _tcp = raw;
      _active();
      RawSocket socket = raw;
      if (uri.scheme == 'https') {
        _handshaking = true;
        final secure = await RawSecureSocket.secure(
          raw,
          host: uri.host,
          context: securityContext,
          supportedProtocols: const ['http/1.1'],
        ).whenComplete(() => _handshaking = false);
        _secure = secure;
        _active();
        require(
          secure.selectedProtocol == null ||
              secure.selectedProtocol == 'http/1.1',
        );
        socket = secure;
      }
      final io = RelayRawIo(socket, active: _active);
      _io = io;
      return socket is RawSecureSocket
          ? RelaySecureSocket(io)
          : RelaySocket(io);
    } catch (_) {
      await _destroy();
      rethrow;
    }
  }

  void cancel() {
    if (cancelled) return;
    cancelled = true;
    if (_tcp == null) {
      _task?.cancel();
    } else if (_handshaking) {
      _tcp!.shutdown(SocketDirection.receive);
    } else {
      _tcpClosing ??= _tcp!.close();
    }
    if (_io != null) unawaited(_io!.close().catchError((Object _) {}));
  }

  Future<void> close() => _closing ??= () async {
    cancel();
    try {
      await _opening;
    } catch (_) {}
    await _destroy();
  }();
  Future<void> _destroy() async {
    await Future.wait<void>([
      if (_io != null) _io!.close(),
      if (_secure != null) (_secureClosing ??= _secure!.close()).then((_) {}),
      if (_tcp != null) (_tcpClosing ??= _tcp!.close()).then((_) {}),
    ]);
  }
}

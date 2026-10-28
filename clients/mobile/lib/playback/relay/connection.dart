// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import 'dart:async';
import 'dart:convert';
import 'dart:io';
import 'dart:typed_data';
import '../commands/wire.dart';

class RelayConnection {
  RelayConnection(this.socket) {
    socket.writeEventsEnabled = false;
    _subscription = socket.listen(
      _event,
      onError: (Object _) => cancel(),
      onDone: () {
        _closed = true;
        _wake();
        if (!_headers.isCompleted) {
          _headers.completeError(const VodFailure('socket_closed'));
        }
      },
    );
    _headerTimer = Timer(const Duration(seconds: 5), cancel);
    _totalTimer = Timer(const Duration(seconds: 75), cancel);
  }
  final RawSocket socket;
  late final StreamSubscription<RawSocketEvent> _subscription;
  late final Timer _headerTimer, _totalTimer;
  final _headers = Completer<List<int>>();
  final _bytes = <int>[];
  Completer<void>? _writable;
  bool _closed = false, _responding = false;
  Future<void>? _closing;
  Future<List<int>> get headers => _headers.future;
  void _wake() {
    final pending = _writable;
    _writable = null;
    if (pending != null && !pending.isCompleted) pending.complete();
  }

  void _event(RawSocketEvent event) {
    if (event == RawSocketEvent.write) {
      socket.writeEventsEnabled = false;
      _wake();
    }
    if (event == RawSocketEvent.readClosed || event == RawSocketEvent.closed) {
      cancel();
      return;
    }
    if (event != RawSocketEvent.read) return;
    try {
      final chunk = socket.read(8193 - _bytes.length);
      if (chunk == null) return;
      require(!_headers.isCompleted, 'pipelining');
      _bytes.addAll(chunk);
      require(_bytes.length <= 8192);
      final end =
          _bytes.length >= 4 &&
          _bytes[_bytes.length - 4] == 13 &&
          _bytes[_bytes.length - 3] == 10 &&
          _bytes[_bytes.length - 2] == 13 &&
          _bytes.last == 10;
      if (end) {
        _headerTimer.cancel();
        _headers.complete(List.of(_bytes));
      }
    } catch (_) {
      cancel();
    }
  }

  Future<void> write(List<int> bytes) async {
    for (var start = 0; start < bytes.length; start += 65536) {
      final end = (start + 65536).clamp(0, bytes.length);
      final part = Uint8List.fromList(bytes.sublist(start, end));
      var offset = 0;
      while (offset < part.length) {
        require(!_closed, 'socket_closed');
        final count = socket.write(part, offset, part.length - offset);
        if (count == 0) {
          final wake = _writable ??= Completer<void>();
          socket.writeEventsEnabled = true;
          try {
            await wake.future.timeout(const Duration(seconds: 10));
          } catch (_) {
            cancel();
            rethrow;
          }
        } else {
          offset += count;
        }
      }
    }
  }

  Future<void> respond(
    int code, {
    String type = 'application/octet-stream',
    int? length,
    String? contentRange,
  }) async {
    require(!_responding);
    _responding = true;
    final label = code == 200
        ? 'OK'
        : code == 206
        ? 'Partial Content'
        : code == 503
        ? 'Service Unavailable'
        : 'Bad Gateway';
    await write(
      ascii.encode(
        'HTTP/1.1 $code $label\r\nConnection: close\r\nCache-Control: no-store\r\nContent-Type: $type\r\n'
        '${length == null ? '' : 'Content-Length: $length\r\n'}${contentRange == null ? '' : 'Content-Range: $contentRange\r\n'}'
        '${code == 503 ? 'Retry-After: 1\r\n' : ''}\r\n',
      ),
    );
  }

  Future<void> error() async {
    if (!_responding && !_closed) {
      try {
        await respond(503, length: 0);
      } catch (_) {}
    }
  }

  void cancel() {
    unawaited(close());
  }

  Future<void> close() => _closing ??= () async {
    _closed = true;
    _headerTimer.cancel();
    _totalTimer.cancel();
    _wake();
    if (!_headers.isCompleted) {
      _headers.completeError(const VodFailure('socket_closed'));
    }
    await socket.close();
    await _subscription.cancel();
  }();
}

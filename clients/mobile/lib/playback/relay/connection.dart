// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import 'dart:async';
import 'dart:convert';
import 'dart:io';
import '../commands/wire.dart';
import 'raw_io.dart';

class RelayConnection {
  RelayConnection(
    RawSocket socket, {
    void Function()? active,
    Timer Function(Duration, void Function())? createTimer,
  }) {
    io = RelayRawIo(
      socket,
      active: () {
        require(!_closed, 'socket_closed');
        active?.call();
      },
      readLimit: () => _headers.isCompleted ? 1 : 8193 - _bytes.length,
    );
    _input = io.input.listen(
      (bytes) {
        try {
          require(!_headers.isCompleted, 'pipelining');
          _bytes.addAll(bytes);
          require(_bytes.length <= 8192);
          final length = _bytes.length;
          if (length >= 4 &&
              _bytes[length - 4] == 13 &&
              _bytes[length - 3] == 10 &&
              _bytes[length - 2] == 13 &&
              _bytes.last == 10) {
            _headerTimer.cancel();
            _headers.complete(List.of(_bytes));
            _bytes.clear();
          }
        } catch (_) {
          cancel();
        }
      },
      onError: (Object _) {
        cancel();
      },
      onDone: cancel,
    );
    final timer = createTimer ?? Timer.new;
    _headerTimer = timer(const Duration(seconds: 5), cancel);
    _totalTimer = timer(const Duration(seconds: 75), cancel);
  }
  late final RelayRawIo io;
  late final StreamSubscription<List<int>> _input;
  late final Timer _headerTimer, _totalTimer;
  final _headers = Completer<List<int>>();
  final _bytes = <int>[];
  final Set<void Function()> _cancellation = {};
  bool _closed = false, _responding = false, _chunked = false;
  Future<void>? _closing;
  Future<List<int>> get headers => _headers.future;
  bool get closed => _closed;
  void onCancel(void Function() callback) {
    if (_closed) {
      callback();
    } else {
      _cancellation.add(callback);
    }
  }

  void removeCancel(void Function() callback) => _cancellation.remove(callback);
  Future<void> write(List<int> bytes) => io.writer.write(bytes);
  Future<void> body(List<int> bytes) async {
    if (_chunked && bytes.isNotEmpty) {
      await write(ascii.encode('${bytes.length.toRadixString(16)}\r\n'));
    }
    await write(bytes);
    if (_chunked && bytes.isNotEmpty) await write(const [13, 10]);
  }

  Future<void> finish() async {
    if (_chunked) await write(ascii.encode('0\r\n\r\n'));
    await io.writer.completion;
  }

  Future<void> respond(
    int code, {
    String type = 'application/octet-stream',
    int? length,
    String? contentRange,
  }) async {
    require(!_responding);
    _responding = true;
    _chunked = length == null;
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
        '${length == null ? 'Transfer-Encoding: chunked\r\n' : 'Content-Length: $length\r\n'}'
        '${contentRange == null ? '' : 'Content-Range: $contentRange\r\n'}'
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
    unawaited(close().catchError((Object _) {}));
  }

  Future<void> close() {
    if (_closing case final work?) return work;
    final completion = Completer<void>();
    _closing = completion.future;
    _dispose().then(completion.complete, onError: completion.completeError);
    return completion.future;
  }

  Future<void> _dispose() async {
    _closed = true;
    _headerTimer.cancel();
    _totalTimer.cancel();
    for (final cancel in List.of(_cancellation)) {
      cancel();
    }
    _cancellation.clear();
    if (!_headers.isCompleted) {
      _headers.completeError(const VodFailure('socket_closed'));
    }
    await Future.wait<void>([_input.cancel(), io.close()]);
  }
}

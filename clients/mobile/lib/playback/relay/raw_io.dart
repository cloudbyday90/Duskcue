// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import 'dart:async';
import 'dart:io';
import 'dart:typed_data';
import 'writer.dart';

class RelayRawIo {
  RelayRawIo(
    this.socket, {
    required void Function() active,
    int Function()? readLimit,
  }) : limit = readLimit ?? (() => 65536) {
    socket.readEventsEnabled = false;
    socket.writeEventsEnabled = false;
    writer = RelayWriter(socket, active);
    _input = StreamController<Uint8List>(
      sync: true,
      onListen: _readable,
      onResume: _readable,
      onPause: () {
        socket.readEventsEnabled = false;
      },
      onCancel: () {
        socket.readEventsEnabled = false;
      },
    );
    _events = socket.listen(
      _event,
      onError: (Object error, StackTrace stack) {
        if (!_ended) _input.addError(error, stack);
        _end();
        unawaited(close().catchError((Object _) {}));
      },
      onDone: () {
        _end();
        writer.cancel();
      },
    );
  }
  final RawSocket socket;
  final int Function() limit;
  late final RelayWriter writer;
  late final StreamController<Uint8List> _input;
  late final StreamSubscription<RawSocketEvent> _events;
  final ended = Completer<void>();
  Future<void>? _closing;
  bool _ended = false, closing = false;
  Stream<Uint8List> get input => _input.stream;

  void _readable() {
    if (!closing && !_ended) socket.readEventsEnabled = true;
  }

  void _end() {
    if (_ended) return;
    _ended = true;
    if (!ended.isCompleted) ended.complete();
    unawaited(_input.close());
  }

  void _event(RawSocketEvent event) {
    if (event == RawSocketEvent.write) writer.writable();
    if (event == RawSocketEvent.readClosed || event == RawSocketEvent.closed) {
      _end();
      if (event == RawSocketEvent.closed) writer.cancel();
    }
    if (event != RawSocketEvent.read ||
        closing ||
        _ended ||
        !_input.hasListener ||
        _input.isPaused) {
      return;
    }
    try {
      final maximum = limit();
      if (maximum < 1 || maximum > 65536) {
        throw StateError('Invalid owned read limit');
      }
      final bytes = socket.read(maximum);
      if (bytes != null) _input.add(bytes);
    } catch (error, stack) {
      if (!_ended) _input.addError(error, stack);
      _end();
      unawaited(close().catchError((Object _) {}));
    }
  }

  Future<void> close() => _closing ??= () async {
    closing = true;
    socket.readEventsEnabled = false;
    writer.cancel();
    try {
      await Future.wait<void>([socket.close().then((_) {}), _events.cancel()]);
    } finally {
      _end();
    }
    try {
      await writer.completion;
    } catch (_) {}
  }();
}

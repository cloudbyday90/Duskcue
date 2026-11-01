// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import 'dart:async';
import 'dart:io';
import 'dart:typed_data';
import '../commands/wire.dart';

class RelayWriter {
  RelayWriter(this.socket, this.active);
  final RawSocket socket;
  final void Function() active;
  Future<void> _tail = Future.value();
  Completer<void>? _writable;
  bool cancelled = false;
  int queued = 0;
  int peakQueued = 0;
  Future<void> get completion => _tail;

  void writable() {
    socket.writeEventsEnabled = false;
    final waiter = _writable;
    _writable = null;
    waiter?.complete();
  }

  void cancel() {
    if (cancelled) return;
    cancelled = true;
    final waiter = _writable;
    _writable = null;
    waiter?.completeError(const VodFailure('socket_closed'));
  }

  Future<void> _ready() async {
    require(!cancelled, 'socket_closed');
    active();
    final waiter = _writable ??= Completer<void>();
    socket.writeEventsEnabled = true;
    await waiter.future.timeout(const Duration(seconds: 10));
    require(!cancelled, 'socket_closed');
    active();
  }

  Future<void> enqueue(List<int> bytes) {
    require(!cancelled && bytes.length <= 131072, 'relay_busy');
    final count = (bytes.length + 65535) ~/ 65536;
    require(queued + count <= 2, 'relay_busy');
    final chunks = <Uint8List>[
      for (var at = 0; at < bytes.length; at += 65536)
        Uint8List.fromList(
          bytes.sublist(at, (at + 65536).clamp(0, bytes.length)),
        ),
    ];
    queued += count;
    if (queued > peakQueued) peakQueued = queued;
    final result = _tail
        .then((_) async {
          for (final chunk in chunks) {
            var offset = 0;
            while (offset < chunk.length) {
              require(!cancelled, 'socket_closed');
              active();
              final written = socket.write(
                chunk,
                offset,
                chunk.length - offset,
              );
              require(written >= 0 && written <= chunk.length - offset);
              offset += written;
              await _ready();
            }
          }
        })
        .whenComplete(() => queued -= count);
    _tail = result;
    result.then(
      (_) {},
      onError: (Object _) {
        cancel();
      },
    );
    return result;
  }

  Future<void> write(List<int> bytes) async {
    for (var at = 0; at < bytes.length; at += 65536) {
      await enqueue(bytes.sublist(at, (at + 65536).clamp(0, bytes.length)));
    }
  }
}

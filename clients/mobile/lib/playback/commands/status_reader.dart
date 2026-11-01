// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import 'dart:async';
import 'models.dart';
import 'retained.dart';
import 'transport.dart';
import 'wire.dart';

class PlaybackStatusReader {
  PlaybackStatusReader(this.transport, this.owns, this.now, this.wait);
  final PlaybackTransport transport;
  final bool Function(int, String) owns;
  final int Function() now;
  final Future<void> Function(Duration) wait;
  Future<void>? _pending;
  void Function()? _cancel;
  int _last = -1000;
  Future<VodReply?> read(
    VodRecord record,
    int token,
    int deadline, {
    String suffix = '',
    bool cleanup = false,
  }) {
    require(_pending == null, 'status_busy');
    final completed = Completer<void>(), cancelled = Completer<void>();
    final slot = completed.future;
    RetainedOperation<VodReply>? request;
    _pending = slot;
    void release() {
      if (identical(_pending, slot)) {
        _pending = null;
        _cancel = null;
      }
      if (!completed.isCompleted) completed.complete();
    }

    _cancel = () {
      if (!cancelled.isCompleted) cancelled.complete();
      request?.cancel();
    };
    final work = () async {
      try {
        final delay = 1000 - (now() - _last);
        if (delay > 0) await wait(Duration(milliseconds: delay));
        require(
          !cancelled.isCompleted && owns(token, record.start),
          'scope_changed',
        );
        final left = deadline - now();
        if (left <= 0) return null;
        _last = now();
        request = transport.read(
          commandPath(record.start, suffix),
          record.scope,
          forCleanup: cleanup,
          timeout: Duration(milliseconds: left.clamp(1, 5000)),
        );
        request!.completion.then((_) => release());
        return await request!.result;
      } finally {
        if (request == null) release();
      }
    }();
    return Future.any<VodReply?>([
      work,
      cancelled.future.then((_) => throw const VodFailure('scope_changed')),
    ]);
  }

  void cancel() => _cancel?.call();
  Future<void> drain() async {
    await _pending;
  }
}

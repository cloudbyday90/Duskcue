// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import 'models.dart';
import 'transport.dart';
import 'wire.dart';

class PlaybackStatusReader {
  PlaybackStatusReader(this.transport, this.owns, this.now, this.wait);
  final PlaybackTransport transport;
  final bool Function(int, String) owns;
  final int Function() now;
  final Future<void> Function(Duration) wait;
  Future<VodReply?>? _pending;
  int _last = -1000;
  Future<VodReply?> read(
    VodRecord record,
    int token,
    int deadline, {
    String suffix = '',
    bool cleanup = false,
  }) {
    require(_pending == null, 'status_busy');
    final work = () async {
      final delay = 1000 - (now() - _last);
      if (delay > 0) await wait(Duration(milliseconds: delay));
      require(owns(token, record.start), 'scope_changed');
      final left = deadline - now();
      if (left <= 0) return null;
      _last = now();
      return transport.call(
        'GET',
        commandPath(record.start, suffix),
        record.scope,
        forCleanup: cleanup,
        timeout: Duration(milliseconds: left.clamp(1, 5000)),
      );
    }();
    _pending = work;
    work.then(
      (_) {
        if (identical(_pending, work)) _pending = null;
      },
      onError: (Object _) {
        if (identical(_pending, work)) _pending = null;
      },
    );
    return work;
  }

  Future<void> drain() async {
    try {
      await _pending;
    } catch (_) {}
  }
}

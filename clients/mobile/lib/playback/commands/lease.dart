// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import 'dart:async';

class PlaybackLease {
  PlaybackLease({
    required this.remaining,
    required this.renew,
    required this.now,
    required this.uncertain,
  });
  final int Function() remaining, now;
  final Future<void> Function() renew;
  final void Function() uncertain;
  Timer? _timer;
  Future<void>? _pending;
  int _last = 0, _epoch = 0;
  bool get running => _timer != null;
  void start() {
    stop();
    _last = now();
    _timer = Timer.periodic(const Duration(seconds: 1), (_) => tick());
  }

  void tick() {
    if (_timer == null) return;
    var left = -1;
    try {
      left = remaining();
      if (left <= 0 || now() - _last > 60000) {
        stop();
        uncertain();
        return;
      }
    } catch (_) {
      stop();
      uncertain();
      return;
    }
    if (_pending != null || now() - _last < 15000 && left > 15000) return;
    _last = now();
    final epoch = _epoch;
    final work = Future<void>.sync(renew);
    _pending = work;
    work.then(
      (_) {
        if (identical(_pending, work)) _pending = null;
      },
      onError: (Object _) {
        if (identical(_pending, work)) _pending = null;
        if (epoch == _epoch) {
          stop();
          uncertain();
        }
      },
    );
  }

  void stop() {
    _epoch++;
    _timer?.cancel();
    _timer = null;
  }

  Future<void> drain() async {
    try {
      await _pending;
    } catch (_) {}
  }
}

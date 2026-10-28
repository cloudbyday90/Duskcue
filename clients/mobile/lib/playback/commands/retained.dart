// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import 'dart:async';
import 'wire.dart';

class RetainedOperations {
  RetainedOperations(this.limit);
  final int limit;
  final Set<Future<void>> _pending = {};
  bool closed = false;
  int get count => _pending.length;
  Future<T> run<T>(
    Future<T> Function() action, {
    Duration timeout = const Duration(seconds: 5),
    void Function()? cancel,
  }) {
    if (closed || count >= limit) return Future.error(const VodFailure('busy'));
    final observed = Completer<T>();
    final completion = Completer<void>();
    final retained = completion.future;
    _pending.add(retained);
    final timer = Timer(timeout, () {
      try {
        cancel?.call();
      } catch (_) {}
      if (!observed.isCompleted) {
        observed.completeError(const VodFailure('observation_timeout'));
      }
    });
    void finished() {
      timer.cancel();
      _pending.remove(retained);
      completion.complete();
    }

    Future<T>.sync(action).then(
      (value) {
        finished();
        if (!observed.isCompleted) observed.complete(value);
      },
      onError: (Object error, StackTrace trace) {
        finished();
        if (!observed.isCompleted) observed.completeError(error, trace);
      },
    );
    return observed.future;
  }

  Future<void> drain() async {
    while (_pending.isNotEmpty) {
      await Future.wait(List.of(_pending));
    }
  }

  Future<void> close() {
    closed = true;
    return drain();
  }
}

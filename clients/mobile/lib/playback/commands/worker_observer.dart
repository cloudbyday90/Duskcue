// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import 'dart:async';
import 'retained.dart';
import 'wire.dart';

class WorkerEvidenceObserver {
  WorkerEvidenceObserver(this.now, this.wait);
  final int Function() now;
  final Future<void> Function(Duration) wait;
  Future<void>? _pending;
  void Function()? _cancel;
  int _last = -1000;
  int reads = 0, negatives = 0;
  bool get pending => _pending != null;

  Future<bool> observe({
    required bool Function() current,
    required RetainedOperation<bool> Function(Duration) read,
    required Future<void> Function() prepare,
  }) {
    require(!pending, 'worker_observation_busy');
    final cancelled = Completer<void>();
    RetainedOperation<bool>? request;
    final until = now() + 60000;
    reads = 0;
    negatives = 0;
    void cancel() {
      if (!cancelled.isCompleted) cancelled.complete();
      request?.cancel();
    }

    _cancel = cancel;
    final settled = Completer<void>();
    final slot = settled.future;
    _pending = slot;
    bool valid() => !cancelled.isCompleted && current();
    final loop = () async {
      await prepare();
      while (valid()) {
        var left = until - now();
        if (left <= 0) return false;
        final delay = 1000 - (now() - _last);
        if (delay > 0) {
          await wait(Duration(milliseconds: delay < left ? delay : left));
        }
        if (!valid()) return false;
        left = until - now();
        if (left <= 0) return false;
        _last = now();
        request = read(Duration(milliseconds: left.clamp(1, 5000)));
        reads++;
        final eligible = await request!.result;
        if (!valid() || now() >= until) return false;
        if (eligible) return true;
        negatives++;
      }
      return false;
    }();
    final timer = Timer(const Duration(seconds: 60), cancel);
    final result = Future.any<bool>([
      loop,
      cancelled.future.then((_) => false),
    ]);
    final completion = () async {
      try {
        await loop;
      } catch (_) {
      } finally {
        await request?.completion;
      }
    }();
    completion.then((_) {
      if (identical(_pending, slot)) {
        _pending = null;
        _cancel = null;
      }
      settled.complete();
    });
    return result.whenComplete(() {
      timer.cancel();
      cancel();
    });
  }

  void cancel() => _cancel?.call();
  Future<void> drain() async {
    await _pending;
  }
}

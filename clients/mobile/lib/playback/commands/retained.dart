// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import 'dart:async';
import 'wire.dart';

class RetainedOperation<T> {
  RetainedOperation._(this.result, this.completion, this._cancel);
  final Future<T> result;
  final Future<void> completion;
  final void Function()? _cancel;
  bool _finished = false, _cancelled = false;
  void cancel() {
    if (_finished || _cancelled) return;
    _cancelled = true;
    try {
      _cancel?.call();
    } catch (_) {}
  }

  RetainedOperation<R> map<R>(R Function(T) convert) =>
      RetainedOperation._(result.then(convert), completion, cancel);
}

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
  }) => retain(action, timeout: timeout, cancel: cancel).result;

  RetainedOperation<T> retain<T>(
    Future<T> Function() action, {
    Duration timeout = const Duration(seconds: 5),
    void Function()? cancel,
  }) {
    if (closed || count >= limit) {
      return RetainedOperation._(
        Future.error(const VodFailure('busy')),
        Future.value(),
        null,
      );
    }
    final observed = Completer<T>();
    final completion = Completer<void>();
    final operation = RetainedOperation<T>._(
      observed.future,
      completion.future,
      cancel,
    );
    final retained = completion.future;
    _pending.add(retained);
    final timer = Timer(timeout, () {
      operation.cancel();
      if (!observed.isCompleted) {
        observed.completeError(const VodFailure('observation_timeout'));
      }
    });
    void finished() {
      operation._finished = true;
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
    return operation;
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

// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import 'dart:async';
import 'package:flutter_test/flutter_test.dart';
import 'package:duskcue_mobile/playback/commands/retained.dart';
import 'package:duskcue_mobile/playback/commands/wire.dart';
import 'package:duskcue_mobile/playback/commands/worker_observer.dart';

void main() {
  test('successful negatives are paced until positive evidence', () async {
    var now = 0;
    final starts = <int>[], owner = RetainedOperations(1);
    final observer = WorkerEvidenceObserver(() => now, (delay) async {
      now += delay.inMilliseconds;
    });
    final result = await observer.observe(
      current: () => true,
      prepare: () async {},
      read: (timeout) {
        starts.add(now);
        expect(timeout.inMilliseconds, lessThanOrEqualTo(5000));
        return owner.retain(() async => starts.length == 3, timeout: timeout);
      },
    );
    await observer.drain();
    expect(result, isTrue);
    expect(starts, [0, 1000, 2000]);
    expect(observer.negatives, 2);
    expect(owner.count, 0);
  });

  test('deadline bounds count and the final read observation', () async {
    var now = 0;
    final timeouts = <int>[], owner = RetainedOperations(1);
    final observer = WorkerEvidenceObserver(() => now, (delay) async {
      now += delay.inMilliseconds;
    });
    expect(
      await observer.observe(
        current: () => true,
        prepare: () async {},
        read: (timeout) {
          timeouts.add(timeout.inMilliseconds);
          return owner.retain(() async => false, timeout: timeout);
        },
      ),
      isFalse,
    );
    await observer.drain();
    expect(timeouts.length, 60);
    expect(timeouts.last, 1000);
    expect(observer.negatives, 60);
    expect(now, 60000);
  });

  test('late positive evidence cannot authorize recovery', () async {
    var now = 0;
    final owner = RetainedOperations(1);
    final observer = WorkerEvidenceObserver(() => now, (_) async {});
    expect(
      await observer.observe(
        current: () => true,
        prepare: () async {},
        read: (timeout) => owner.retain(() async {
          now = 60000;
          return true;
        }, timeout: timeout),
      ),
      isFalse,
    );
    await observer.drain();
    expect(observer.reads, 1);
  });

  test('cancelled read retains both slots until actual completion', () async {
    final source = Completer<bool>(), issued = Completer<void>();
    final owner = RetainedOperations(1);
    var cancellations = 0;
    final observer = WorkerEvidenceObserver(() => 0, (_) async {});
    final work = observer.observe(
      current: () => true,
      prepare: () async {},
      read: (timeout) {
        issued.complete();
        return owner.retain(
          () => source.future,
          timeout: timeout,
          cancel: () => cancellations++,
        );
      },
    );
    await issued.future;
    observer.cancel();
    expect(await work, isFalse);
    expect(cancellations, 1);
    expect(owner.count, 1);
    expect(observer.pending, isTrue);
    expect(
      () => observer.observe(
        current: () => true,
        prepare: () async {},
        read: (_) => owner.retain(() async => true),
      ),
      throwsA(isA<VodFailure>()),
    );
    source.complete(true);
    await observer.drain();
    expect(owner.count, 0);
    expect(observer.pending, isFalse);
  });

  test('cancel during pacing issues no later read', () async {
    final waiting = Completer<void>(), release = Completer<void>();
    final owner = RetainedOperations(1);
    final observer = WorkerEvidenceObserver(() => 0, (_) {
      waiting.complete();
      return release.future;
    });
    final work = observer.observe(
      current: () => true,
      prepare: () async {},
      read: (timeout) => owner.retain(() async => false, timeout: timeout),
    );
    await waiting.future;
    observer.cancel();
    expect(await work, isFalse);
    expect(observer.pending, isTrue);
    release.complete();
    await observer.drain();
    expect(observer.reads, 1);
  });

  test('read failure stops without another read', () async {
    final owner = RetainedOperations(1);
    final observer = WorkerEvidenceObserver(() => 0, (_) async {});
    await expectLater(
      observer.observe(
        current: () => true,
        prepare: () async {},
        read: (timeout) => owner.retain(() async {
          throw const VodFailure('network_uncertain');
        }, timeout: timeout),
      ),
      throwsA(isA<VodFailure>()),
    );
    await observer.drain();
    expect(observer.reads, 1);
  });

  test('changed authority rejects a positive read', () async {
    var current = true;
    final owner = RetainedOperations(1);
    final observer = WorkerEvidenceObserver(() => 0, (_) async {});
    expect(
      await observer.observe(
        current: () => current,
        prepare: () async {},
        read: (timeout) => owner.retain(() async {
          current = false;
          return true;
        }, timeout: timeout),
      ),
      isFalse,
    );
    await observer.drain();
    expect(observer.reads, 1);
  });

  test('preparation failure never reads evidence', () async {
    final observer = WorkerEvidenceObserver(() => 0, (_) async {});
    await expectLater(
      observer.observe(
        current: () => true,
        prepare: () async {
          throw const VodFailure('native_cleanup_pending');
        },
        read: (_) => throw StateError('must not read'),
      ),
      throwsA(isA<VodFailure>()),
    );
    await observer.drain();
    expect(observer.reads, 0);
  });
}

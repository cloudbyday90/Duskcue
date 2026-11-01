// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import 'dart:async';
import 'package:flutter_test/flutter_test.dart';
import 'package:uuid/uuid.dart';
import 'package:duskcue_mobile/playback/commands/clock.dart';
import 'package:duskcue_mobile/playback/commands/controller.dart';
import 'package:duskcue_mobile/playback/commands/selection.dart';
import 'playback_commands_test.dart' show CommandFixture;
import 'playback_native_test.dart' show until;

Future<void> admitted(CommandFixture f) async {
  await f.commands.open(androidSelection(f.item));
  f.commands.observe(f.output, 600, 'playing');
  await f.commands.checkpoints.flush(force: true);
}

void main() {
  test('late old-viewer completion cannot overwrite a fresh Start', () async {
    final f = CommandFixture(), gate = Completer<void>();
    await f.open();
    try {
      await admitted(f);
      final old = f.commands.record!.start;
      f.beforeConfiguration = () => gate.future;
      final work = f.commands.observeWorkerFailure(
        f.output,
        active: () => true,
        detach: () async {},
        unavailable: () => fail('old viewer callback'),
      );
      await until(() => f.configurationReads == 1);
      await f.commands.close();
      await work;
      await f.commands.open(androidSelection(f.item));
      final fresh = f.commands.record!.start;
      gate.complete();
      await f.commands.workerObserver.drain();
      expect(fresh, isNot(old));
      expect(f.commands.record!.start, fresh);
      expect(f.commands.deliverable, isTrue);
      expect(f.commands.error, isNull);
      expect(f.changes, 0);
    } finally {
      if (!gate.isCompleted) gate.complete();
      await f.close();
    }
  });
  test('a refreshed profile-context revision refuses held evidence', () async {
    final f = CommandFixture(), gate = Completer<void>();
    await f.open();
    try {
      await admitted(f);
      f.beforeConfiguration = () => gate.future;
      final work = f.commands.observeWorkerFailure(
        f.output,
        active: () => true,
        detach: () async {},
        unavailable: () {},
      );
      await until(() => f.configurationReads == 1);
      f.contextRevision = '2';
      await f.transport.context();
      f.workerRecovery = true;
      gate.complete();
      await work;
      expect(f.commands.error!.code, 'scope_changed');
      expect(f.commands.workerObserving, isFalse);
      expect(f.changes, 0);
    } finally {
      if (!gate.isCompleted) gate.complete();
      await f.close();
    }
  });
  for (final payload in ['{', '{}', 'x' * 32769]) {
    test(
      'malformed or over-limit evidence stops without retry (${payload.length})',
      () async {
        final f = CommandFixture();
        await f.open();
        try {
          await admitted(f);
          f.configurationRaw = payload;
          await f.commands.observeWorkerFailure(
            f.output,
            active: () => true,
            detach: () async {},
            unavailable: () {},
          );
          await f.commands.workerObserver.drain();
          expect(f.configurationReads, 1);
          expect(f.changes, 0);
          expect(f.commands.error, isNotNull);
          expect(f.commands.workerObserving, isFalse);
        } finally {
          await f.close();
        }
      },
    );
  }
  test(
    'positive proof without a confirmed checkpoint requires explicit review',
    () async {
      final f = CommandFixture();
      await f.open();
      try {
        await f.commands.open(androidSelection(f.item));
        f.workerRecovery = true;
        var reviews = 0;
        await f.commands.observeWorkerFailure(
          f.output,
          active: () => true,
          detach: () async {},
          unavailable: () => reviews++,
        );
        expect(f.commands.error!.code, 'checkpoint_required');
        expect(reviews, 1);
        expect(f.changes, 0);
        expect(f.starts, 1);
        expect(f.commands.workerObserver.pending, isFalse);
      } finally {
        await f.close();
      }
    },
  );
  test(
    'cancelling a read leaves concurrent renewal owned and running',
    () async {
      var now = 0;
      final wall = DateTime.now().millisecondsSinceEpoch;
      final f = CommandFixture(),
          read = Completer<void>(),
          renewal = Completer<void>();
      await f.open(
        clock: PlaybackClock(now: () => now, wall: () => wall + now),
      );
      try {
        await admitted(f);
        f.beforeConfiguration = () => read.future;
        f.beforeRenewal = () => renewal.future;
        final work = f.commands.observeWorkerFailure(
          f.output,
          active: () => true,
          detach: () async {},
          unavailable: () {},
        );
        await until(() => f.configurationReads == 1);
        now = 15000;
        f.commands.lease.tick();
        await until(() => f.renewals == 1);
        f.commands.cancelWorkerObservation();
        await work;
        renewal.complete();
        await f.commands.lease.drain();
        expect(f.commands.lease.running, isTrue);
        expect(f.commands.error!.code, 'worker_status_required');
        read.complete();
        await f.commands.workerObserver.drain();
        expect(f.changes, 0);
      } finally {
        if (!read.isCompleted) read.complete();
        if (!renewal.isCompleted) renewal.complete();
        await f.close();
      }
    },
  );
  test(
    'changed transport authority ends checking without late recovery',
    () async {
      final f = CommandFixture(), gate = Completer<void>();
      await f.open();
      try {
        await admitted(f);
        f.beforeConfiguration = () => gate.future;
        final work = f.commands.observeWorkerFailure(
          f.output,
          active: () => true,
          detach: () async {},
          unavailable: () {},
        );
        await until(() => f.configurationReads == 1);
        f.identity = null;
        f.workerRecovery = true;
        gate.complete();
        await work;
        expect(f.commands.error!.code, 'scope_changed');
        expect(f.commands.workerObserving, isFalse);
        expect(f.changes, 0);
      } finally {
        if (!gate.isCompleted) gate.complete();
        await f.close();
      }
    },
  );
  test('credential invalidation fences a held positive completion', () async {
    final f = CommandFixture(), gate = Completer<void>();
    await f.open();
    try {
      await admitted(f);
      f.beforeConfiguration = () => gate.future;
      final work = f.commands.observeWorkerFailure(
        f.output,
        active: () => true,
        detach: () async {},
        unavailable: () => fail('late observer callback'),
      );
      await until(() => f.configurationReads == 1);
      f.identity = null;
      f.commands.invalidate();
      await work;
      f.workerRecovery = true;
      gate.complete();
      await f.commands.workerObserver.drain();
      expect(f.changes, 0);
      expect(f.commands.cleanupOnly, isFalse);
      expect(f.commands.workerObserving, isFalse);
      expect(f.commands.record, isNull);
      expect(f.commands.state, 'scope_changed');
      expect(f.commands.error, isNull);
    } finally {
      if (!gate.isCompleted) gate.complete();
      await f.close();
    }
  });
  test(
    'loopback false-to-true proof sends exactly one conditional Recover',
    () async {
      final f = CommandFixture();
      await f.open();
      try {
        await admitted(f);
        final first = f.output, history = f.commands.admitted!.history;
        var detached = 0, unavailable = 0;
        f.beforeConfiguration = () async {
          if (f.configurationReads >= 2) f.workerRecovery = true;
        };
        await f.commands.observeWorkerFailure(
          first,
          active: () => true,
          detach: () async {
            detached++;
          },
          unavailable: () => unavailable++,
        );
        await f.commands.workerObserver.drain();
        expect(f.commands.error, isNull);
        expect(f.starts, 1);
        expect(f.changes, 1);
        expect(f.commands.workerObserver.negatives, 1);
        expect(f.commands.workerObserver.reads, 2);
        expect(f.output, isNot(first));
        expect(f.commands.admitted!.history, history);
        expect(detached, 1);
        expect(unavailable, 0);
        expect(f.commands.deliverable, isTrue);
        await f.commands.close();
      } finally {
        await f.close();
      }
    },
  );

  for (final evidence in [
    'output',
    'changing',
    'retiring',
    'uncertain',
    'pending',
    'refused',
  ]) {
    test(
      '$evidence evidence stops without a mutation or automatic replay',
      () async {
        final f = CommandFixture();
        await f.open();
        try {
          await admitted(f);
          if (evidence == 'output') f.configurationOutput = const Uuid().v7();
          if (['changing', 'retiring', 'uncertain'].contains(evidence)) {
            f.configurationState = evidence;
          }
          if (evidence == 'pending') f.pendingConfiguration = const Uuid().v7();
          if (evidence == 'refused') f.configurationStatus = 503;
          var review = 0;
          await f.commands.observeWorkerFailure(
            f.output,
            active: () => true,
            detach: () async {},
            unavailable: () => review++,
          );
          await f.commands.workerObserver.drain();
          expect(f.changes, 0);
          expect(f.configurationReads, 1);
          expect(review, 1);
          expect(f.commands.state, 'uncertain');
          expect(f.commands.workerObserving, isFalse);
        } finally {
          await f.close();
        }
      },
    );
  }

  test(
    'Close fences a held configuration read and its late positive result',
    () async {
      final f = CommandFixture(), gate = Completer<void>();
      await f.open();
      try {
        await admitted(f);
        f.beforeConfiguration = () => gate.future;
        final work = f.commands.observeWorkerFailure(
          f.output,
          active: () => true,
          detach: () async {},
          unavailable: () => fail('stale failure was published'),
        );
        await until(() => f.configurationReads == 1);
        await f.commands.close();
        await work;
        f.workerRecovery = true;
        gate.complete();
        await f.commands.workerObserver.drain();
        expect(f.commands.record, isNull);
        expect(f.commands.state, 'settled');
        expect(f.commands.error, isNull);
        expect(f.changes, 0);
        expect(f.withdrawals, 1);
      } finally {
        if (!gate.isCompleted) gate.complete();
        await f.close();
      }
    },
  );

  test('renewal runs independently of the observer read', () async {
    var now = 0;
    final wall = DateTime.now().millisecondsSinceEpoch;
    final f = CommandFixture(), gate = Completer<void>();
    await f.open(
      clock: PlaybackClock(now: () => now, wall: () => wall + now),
    );
    try {
      await admitted(f);
      f.beforeConfiguration = () => gate.future;
      final work = f.commands.observeWorkerFailure(
        f.output,
        active: () => true,
        detach: () async {},
        unavailable: () {},
      );
      await until(() => f.configurationReads == 1);
      now = 15000;
      f.commands.lease.tick();
      await until(() => f.renewals == 1);
      await f.commands.lease.drain();
      expect(f.commands.workerObserving, isTrue);
      expect(f.commands.lease.running, isTrue);
      f.workerRecovery = true;
      gate.complete();
      await work;
      expect(f.changes, 1);
      expect(f.commands.error, isNull);
      await f.commands.close();
    } finally {
      if (!gate.isCompleted) gate.complete();
      await f.close();
    }
  });

  test(
    'lease failure preserves the explicit-play fence and never recovers',
    () async {
      final f = CommandFixture(), gate = Completer<void>();
      await f.open();
      try {
        await admitted(f);
        f.beforeConfiguration = () => gate.future;
        var review = 0;
        final work = f.commands.observeWorkerFailure(
          f.output,
          active: () => true,
          detach: () async {},
          unavailable: () => review++,
        );
        await until(() => f.configurationReads == 1);
        f.commands.deadline = DateTime.now().toUtc().subtract(
          const Duration(seconds: 2),
        );
        f.commands.lease.tick();
        await work;
        gate.complete();
        await f.commands.workerObserver.drain();
        expect(review, 1);
        expect(f.commands.error!.code, 'lease_uncertain');
        expect(f.changes, 0);
        expect(f.commands.lease.running, isFalse);
      } finally {
        if (!gate.isCompleted) gate.complete();
        await f.close();
      }
    },
  );

  test(
    'uncertain conditional Recover is retained and never replayed',
    () async {
      final f = CommandFixture();
      await f.open();
      try {
        await admitted(f);
        f.workerRecovery = true;
        f.lostChange = true;
        await f.commands.observeWorkerFailure(
          f.output,
          active: () => true,
          detach: () async {},
          unavailable: () {},
        );
        expect(f.changes, 1);
        expect(f.commands.record!.operation, isNotNull);
        expect(f.commands.error, isNotNull);
        await Future<void>.delayed(const Duration(milliseconds: 25));
        expect(f.changes, 1);
        expect(f.starts, 1);
        await f.commands.close();
      } finally {
        await f.close();
      }
    },
  );
}

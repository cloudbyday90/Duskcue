// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import 'dart:async';
import 'dart:convert';
import 'package:flutter_test/flutter_test.dart';
import 'package:duskcue_mobile/playback/commands/checkpoints.dart';
import 'package:duskcue_mobile/playback/commands/lease.dart';
import 'package:duskcue_mobile/playback/commands/selection.dart';
import 'package:duskcue_mobile/playback/commands/wire.dart';
import 'playback_commands_test.dart' show CommandFixture;

void main() {
  test(
    'reserved cleanup remains usable while regular operations are unresolved',
    () async {
      final fixture = CommandFixture();
      await fixture.open();
      addTearDown(fixture.close);
      await fixture.commands.open(androidSelection(fixture.item));
      final gate = Completer<void>();
      final held = List.generate(
        3,
        (_) => fixture.transport.regular.run(() => gate.future),
      );
      try {
        await fixture.commands.close();
        expect(fixture.withdrawals, 1);
        expect(fixture.commands.record, isNull);
        expect(fixture.transport.regular.count, 3);
      } finally {
        gate.complete();
        await Future.wait(held);
      }
    },
  );
  test(
    'renewal remains single-flight without native position callbacks',
    () async {
      var now = 0, renewals = 0, uncertain = 0;
      var gate = Completer<void>();
      final lease = PlaybackLease(
        remaining: () => 60000,
        now: () => now,
        uncertain: () => uncertain++,
        renew: () {
          renewals++;
          return gate.future;
        },
      );
      lease.start();
      now = 15000;
      lease.tick();
      now = 30000;
      lease.tick();
      expect(renewals, 1);
      gate.complete();
      await lease.drain();
      gate = Completer<void>();
      now = 45000;
      lease.tick();
      expect(renewals, 2);
      gate.complete();
      await lease.drain();
      lease.stop();
      expect(uncertain, 0);
    },
  );
  test(
    'checkpoint bursts coalesce and cannot adopt an older output acknowledgement',
    () async {
      final first = Completer<Wire>();
      final sent = <Wire>[];
      var output = 'old';
      final checkpoints = PlaybackCheckpoints(
        now: () => 0,
        current: (value) => value == output,
        onUncertain: () {},
        read: () async => {},
        send: (body) {
          sent.add(body);
          return sent.length == 1 ? first.future : Future.value(body);
        },
      );
      checkpoints.observe('old', 1000, 'playing');
      final pending = checkpoints.flush(force: true);
      final observers = <Future<Wire?>>[];
      for (var index = 0; index < 50; index++) {
        checkpoints.observe('old', 2000 + index, 'paused');
        observers.add(checkpoints.flush(force: true));
      }
      first.complete(sent.single);
      await pending;
      await Future.wait(observers);
      expect(sent.length, 2);
      expect(sent.last['position_ms'], 2049);
      output = 'new';
      checkpoints.reset();
      expect(checkpoints.confirmed, isNull);
      expect(checkpoints.observe('old', 9000, 'playing'), isFalse);
    },
  );
  test('corrupt journal refuses downgrade and dispatches no Start', () async {
    final fixture = CommandFixture()..supported = false;
    await fixture.open();
    addTearDown(fixture.close);
    fixture.storage.value = jsonEncode({'version': 1, 'token': 'forbidden'});
    expect(await fixture.commands.open(androidSelection(fixture.item)), isTrue);
    expect(fixture.starts, 0);
    expect(fixture.commands.error, isNotNull);
  });
  test(
    'Close fences an intended Start while its journal write is still pending',
    () async {
      final fixture = CommandFixture();
      await fixture.open();
      addTearDown(fixture.close);
      fixture.storage.gate = Completer<void>();
      final open = fixture.commands.open(androidSelection(fixture.item));
      final end = DateTime.now().add(const Duration(seconds: 2));
      while (!fixture.storage.calls.contains('write') &&
          DateTime.now().isBefore(end)) {
        await Future<void>.delayed(const Duration(milliseconds: 10));
      }
      expect(fixture.storage.calls, contains('write'));
      await fixture.commands.close();
      fixture.storage.gate!.complete();
      await open;
      await fixture.commands.journal.operations.drain();
      expect(fixture.starts, 0);
      expect(fixture.commands.record, isNull);
      expect(fixture.commands.busy, isFalse);
    },
  );
}

// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:duskcue_mobile/playback/commands/controller.dart';
import 'package:duskcue_mobile/playback/commands/journal.dart';
import 'package:duskcue_mobile/playback/commands/wire.dart';
import 'package:duskcue_mobile/playback/coordinator.dart';

Future<void> runLostOutcomes(
  WidgetTester tester,
  IntegrationTestWidgetsFlutterBinding binding,
  MethodChannel channel,
  Wire fixture,
  PlaybackCommands commands,
  PlaybackCoordinator coordinator,
) async {
  Future<void> until(bool Function() condition) async {
    final end = DateTime.now().add(const Duration(seconds: 30));
    while (!condition() && DateTime.now().isBefore(end)) {
      await tester.pump(const Duration(milliseconds: 100));
    }
    expect(
      condition(),
      isTrue,
      reason:
          'Lost-outcome state=${commands.state}, code=${commands.error?.code}, attaching=${coordinator.attaching}',
    );
  }

  bool attached() =>
      commands.deliverable &&
      !coordinator.attaching &&
      coordinator.native.controller?.value.isInitialized == true;
  await tester.pumpWidget(const MaterialApp(home: SizedBox()));
  try {
    expect(await coordinator.open(entity(fixture['item'])), isTrue);
    expect(commands.state, 'uncertain');
    expect(coordinator.native.generation, isNull);
    final start = commands.record!.start;
    expect(await SecureJournalStorage().read(), contains(start));
    await commands.check();
    await until(attached);
    final history = commands.admitted!.history;
    await coordinator.pause();
    await commands.checkpoints.flush(force: true);
    final old = coordinator.native.generation!;
    await coordinator.seek(2000);
    expect(commands.state, 'uncertain');
    expect(commands.record!.operation, isNotNull);
    await until(() => old.disposed && !coordinator.attaching);
    expect(coordinator.native.generation, isNull);
    await commands.check();
    await until(attached);
    expect(commands.admitted!.output, isNot(old.output));
    expect(coordinator.native.controller!.value.isPlaying, isFalse);
    expect(commands.record!.start, start);
    expect(commands.admitted!.history, history);
    debugPrint('ANDROID_PHASE renew-loss');
    await until(() => commands.error?.code == 'lease_uncertain');
    await until(() => coordinator.native.generation == null);
    expect(commands.lease.running, isFalse);
    await commands.check();
    await until(attached);
    expect(coordinator.native.controller!.value.isPlaying, isFalse);
    expect(commands.record!.start, start);
    expect(commands.admitted!.history, history);
    await coordinator.close();
    expect(commands.cleanupOnly, isTrue);
    expect(commands.record!.start, start);
    expect(coordinator.native.generation, isNull);
    expect(await SecureJournalStorage().read(), contains(start));
    await commands.check();
    expect(commands.record, isNull);
    expect(await SecureJournalStorage().read(), isNull);
    await channel.invokeMethod<void>('clearBootstrap');
    binding.reportData = {
      'scenario': 'lost-outcomes',
      'start': start,
      'lost_start_reconciled': true,
      'lost_change_reconciled': true,
      'lost_renew_reconciled': true,
      'lost_withdraw_settled': true,
      'same_viewer': true,
      'paused_intent': true,
      'confirmed_cleanup': true,
    };
  } finally {
    await coordinator.close();
    coordinator.dispose();
  }
}

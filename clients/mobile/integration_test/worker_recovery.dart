// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:duskcue_mobile/playback/commands/controller.dart';
import 'package:duskcue_mobile/playback/commands/wire.dart';
import 'package:duskcue_mobile/playback/coordinator.dart';

Future<void> runWorkerRecovery(
  WidgetTester tester,
  IntegrationTestWidgetsFlutterBinding binding,
  MethodChannel channel,
  Wire fixture,
  PlaybackCommands commands,
  PlaybackCoordinator coordinator,
) async {
  Future<void> until(bool Function() predicate, {int seconds = 30}) async {
    final end = DateTime.now().add(Duration(seconds: seconds));
    while (!predicate() && DateTime.now().isBefore(end)) {
      await tester.pump(const Duration(milliseconds: 100));
    }
    expect(
      predicate(),
      isTrue,
      reason:
          'Worker recovery state=${commands.state}, code=${commands.error?.code}, attaching=${coordinator.attaching}',
    );
  }

  await tester.pumpWidget(const MaterialApp(home: SizedBox()));
  try {
    expect(await coordinator.open(entity(fixture['worker_item'])), isTrue);
    await until(
      () =>
          !coordinator.attaching &&
          coordinator.native.controller?.value.isInitialized == true,
    );
    final first = coordinator.native.generation!;
    final start = commands.record!.start, history = commands.admitted!.history;
    final audio = commands.admitted!.audio,
        subtitle = commands.admitted!.subtitle;
    await until(() => first.player.value.position.inMilliseconds >= 500);
    await commands.checkpoints.flush(force: true);
    await until(
      () =>
          commands.checkpoints.confirmed?['output_revision'] == first.output &&
          integer(commands.checkpoints.confirmed?['position_ms'] ?? 0) >= 500,
    );
    final initialConfig = await commands.transport.call(
      'GET',
      commandPath(start, '/configuration'),
      commands.record!.scope,
    );
    final checkpoint = commands.checkpoints.confirmed!;
    expect(checkpoint['output_revision'], first.output);
    expect(integer(checkpoint['position_ms']), greaterThanOrEqualTo(500));
    await until(
      () =>
          commands.record?.output != first.output &&
          commands.deliverable &&
          coordinator.native.ready?.output == commands.record?.output &&
          !coordinator.attaching &&
          coordinator.native.controller?.value.isInitialized == true,
      seconds: 90,
    );
    expect(first.disposed, isTrue);
    expect(coordinator.playingIntent, isTrue);
    expect(coordinator.native.controller!.value.isPlaying, isTrue);
    expect(commands.record!.start, start);
    expect(commands.admitted!.history, history);
    expect(commands.admitted!.audio, audio);
    expect(commands.admitted!.subtitle, subtitle);
    expect(
      commands.admitted!.position,
      greaterThanOrEqualTo(integer(checkpoint['position_ms'])),
    );
    final config = await commands.transport.call(
      'GET',
      commandPath(start, '/configuration'),
      commands.record!.scope,
    );
    expect(config.body['configuration'], initialConfig.body['configuration']);
    expect(object(config.body['recovery_budget'])['attempts'], 1);
    expect(object(config.body['recovery_budget'])['restarted_stall'], isTrue);
    await coordinator.close();
    expect(commands.record, isNull);
    expect(coordinator.native.generation, isNull);
    await channel.invokeMethod<void>('clearBootstrap');
    binding.reportData = {
      'scenario': 'worker-recovery',
      'start': start,
      'native_checkpoint': true,
      'automatic_worker_recovery': true,
      'same_viewer': true,
      'preserved_configuration': true,
      'one_recovery': true,
      'confirmed_cleanup': true,
    };
  } finally {
    await coordinator.close();
    coordinator.dispose();
  }
}

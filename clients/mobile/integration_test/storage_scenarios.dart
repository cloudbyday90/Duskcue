// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import 'dart:convert';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_secure_storage/flutter_secure_storage.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:duskcue_mobile/playback/commands/controller.dart';
import 'package:duskcue_mobile/playback/commands/journal.dart';
import 'package:duskcue_mobile/playback/commands/selection.dart';
import 'package:duskcue_mobile/playback/commands/wire.dart';
import 'package:duskcue_mobile/playback/coordinator.dart';

const _default = FlutterSecureStorage(
  aOptions: AndroidOptions(
    resetOnError: false,
    migrateOnAlgorithmChange: false,
    migrateWithBackup: false,
  ),
);
const _other = FlutterSecureStorage(
  aOptions: AndroidOptions(
    storageNamespace: 'duskcue.qualification.other.v1',
    preferencesKeyPrefix: 'duskcue_qualification_other_v1',
    resetOnError: false,
    migrateOnAlgorithmChange: false,
    migrateWithBackup: false,
  ),
);

Future<void> _sentinels({bool seed = false}) async {
  if (seed) {
    expect(await _default.read(key: 'record'), isNull);
    expect(await _other.read(key: 'record'), isNull);
    await _default.write(key: 'record', value: 'qualification-default-v1');
    await _other.write(key: 'record', value: 'qualification-other-v1');
  }
  expect(await _default.read(key: 'record'), 'qualification-default-v1');
  expect(await _other.read(key: 'record'), 'qualification-other-v1');
}

Future<void> runStorageScenario(
  WidgetTester tester,
  IntegrationTestWidgetsFlutterBinding binding,
  MethodChannel channel,
  Wire fixture,
  PlaybackCommands commands,
  PlaybackCoordinator coordinator,
) async {
  final scenario = text(fixture['scenario']);
  final journal = SecureJournalStorage();
  final item = entity(fixture['item']);
  await tester.pumpWidget(const MaterialApp(home: SizedBox()));
  var retained = false;
  try {
    if (scenario == 'cold-retain' || scenario == 'corrupt-retain') {
      await _sentinels(seed: scenario == 'cold-retain');
      expect(await journal.read(), isNull);
      expect(await coordinator.open(item), isTrue);
      final end = DateTime.now().add(const Duration(seconds: 30));
      while ((coordinator.native.controller?.value.isInitialized != true ||
              coordinator.attaching) &&
          DateTime.now().isBefore(end)) {
        await tester.pump(const Duration(milliseconds: 100));
      }
      expect(coordinator.native.controller?.value.isInitialized, isTrue);
      await coordinator.pause();
      final raw = await journal.read();
      expect(raw, isNotNull);
      expect(utf8.encode(raw!).length, lessThanOrEqualTo(4096));
      expect(decodeWire(utf8.encode(raw))['start'], commands.record!.start);
      await _sentinels();
      if (scenario == 'corrupt-retain') {
        final probe = await channel.invokeMapMethod<String, dynamic>(
          'storageCorrupt',
        );
        expect(probe?['backed_up'], isTrue);
      }
      binding.reportData = {
        'scenario': scenario,
        'start': commands.record!.start,
        'journal_read_back': true,
        'namespaces_intact': true,
        'retained_for_process_death': true,
      };
      retained = true;
      return;
    }
    await _sentinels();
    if (scenario == 'corrupt-refuse') {
      final before = await channel.invokeMapMethod<String, dynamic>(
        'storageProbe',
      );
      expect(before?['backed_up'], isTrue);
      expect(await commands.open(androidSelection(item)), isTrue);
      expect(commands.error?.code, 'storage_unavailable');
      expect(commands.record, isNull);
      expect(coordinator.native.generation, isNull);
      await coordinator.close();
      expect(commands.error?.code, 'storage_unavailable');
      expect(
        await channel.invokeMapMethod<String, dynamic>('storageProbe'),
        before,
      );
      expect(await commands.open(androidSelection(item)), isTrue);
      expect(commands.error?.code, 'storage_unavailable');
      expect(commands.record, isNull);
      await coordinator.close();
      expect(
        await channel.invokeMapMethod<String, dynamic>('storageProbe'),
        before,
      );
      await _sentinels();
      await channel.invokeMethod<void>('storageRestore');
      binding.reportData = {
        'scenario': scenario,
        'corruption_refused': true,
        'close_preserved_ciphertext': true,
        'namespaces_intact': true,
      };
      return;
    }
    require(scenario == 'cold-restore' || scenario == 'corrupt-restore');
    expect(await coordinator.open(item), isTrue);
    expect(commands.record!.start, entity(fixture['expected_start']));
    expect(commands.cleanupOnly, isTrue);
    expect(commands.receipt, isNull);
    expect(commands.lease.running, isFalse);
    expect(coordinator.native.generation, isNull);
    expect(coordinator.native.controller, isNull);
    await coordinator.close();
    expect(commands.record, isNull);
    expect(commands.error, isNull);
    expect(await journal.read(), isNull);
    await _sentinels();
    if (scenario == 'corrupt-restore') {
      await _default.delete(key: 'record');
      await _other.delete(key: 'record');
      expect(await _default.read(key: 'record'), isNull);
      expect(await _other.read(key: 'record'), isNull);
    }
    binding.reportData = {
      'scenario': scenario,
      'start': entity(fixture['expected_start']),
      'cleanup_only_restore': true,
      'namespaces_intact': true,
      'confirmed_cleanup': true,
    };
  } finally {
    if (!retained) {
      await coordinator.close();
      coordinator.dispose();
    }
  }
}

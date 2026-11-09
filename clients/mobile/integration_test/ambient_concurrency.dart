// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:duskcue_mobile/l10n/app_strings.dart';
import 'package:duskcue_mobile/playback/commands/clock.dart';
import 'package:duskcue_mobile/playback/commands/controller.dart';
import 'package:duskcue_mobile/playback/commands/journal.dart';
import 'package:duskcue_mobile/playback/commands/scope.dart';
import 'package:duskcue_mobile/playback/commands/transport.dart';
import 'package:duskcue_mobile/playback/commands/wire.dart';
import 'package:duskcue_mobile/playback/coordinator.dart';
import 'package:duskcue_mobile/playback/provider.dart';
import 'package:duskcue_mobile/screens/command_playback_screen.dart';
import 'ambient_support.dart';

Future<void> ambientConcurrency(WidgetTester tester, Wire fixture, AmbientHarness ambient) async {
  final identity = PlaybackIdentity(origin: Uri.parse(text(fixture['http_origin'])), token: text(fixture['token']), credentialEpoch: 1,
    profileEpoch: 1, user: entity(fixture['user']), profile: entity(fixture['profile']), ready: true);
  final commands = PlaybackCommands(PlaybackTransport(identity: () => identity, clock: PlaybackClock()), PlaybackJournal(SecureJournalStorage()));
  final interactive = PlaybackCoordinator(commands);
  Future<void> until(bool Function() predicate) async {
    final deadline = DateTime.now().add(const Duration(seconds: 30));
    while (!predicate() && DateTime.now().isBefore(deadline)) { await tester.pump(const Duration(milliseconds: 100)); }
    expect(predicate(), isTrue, reason: 'Independent interactive evidence incomplete');
  }
  try {
    await ambient.open();
    final ambientStart = ambient.status.requestId!;
    await tester.pumpWidget(ProviderScope(overrides: [playbackCoordinatorProvider.overrideWithValue(interactive)], child: MaterialApp(
      localizationsDelegates: [const AppStringsDelegate()], supportedLocales: AppStrings.supportedLocales,
      home: CommandPlaybackScreen(itemId: entity(fixture['item'])))));
    await until(() => commands.deliverable && !interactive.attaching && interactive.native.controller?.value.isInitialized == true);
    final interactiveStart = commands.record!.start;
    expect(interactiveStart, isNot(ambientStart));
    await ambient.service.pause();
    await ambient.stop();
    expect(commands.deliverable, isTrue);
    expect(commands.record?.start, interactiveStart);
    await interactive.play();
    await until(() => interactive.native.controller!.value.position.inMilliseconds > 250);
    await interactive.close();
    expect(commands.record, isNull);
  } finally {
    await interactive.close();
    await tester.pumpWidget(const SizedBox.shrink());
  }
}

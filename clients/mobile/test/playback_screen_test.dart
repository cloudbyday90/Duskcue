// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:uuid/uuid.dart';
import 'package:duskcue_mobile/l10n/app_strings.dart';
import 'package:duskcue_mobile/playback/commands/clock.dart';
import 'package:duskcue_mobile/playback/commands/controller.dart';
import 'package:duskcue_mobile/playback/commands/journal.dart';
import 'package:duskcue_mobile/playback/commands/models.dart';
import 'package:duskcue_mobile/playback/commands/scope.dart';
import 'package:duskcue_mobile/playback/commands/transport.dart';
import 'package:duskcue_mobile/playback/commands/wire.dart';
import 'package:duskcue_mobile/playback/coordinator.dart';
import 'package:duskcue_mobile/playback/provider.dart';
import 'package:duskcue_mobile/screens/command_playback_screen.dart';
import 'playback_commands_test.dart' show MemoryJournal;

class ScreenCommands extends PlaybackCommands {
  ScreenCommands()
    : super(
        PlaybackTransport(identity: () => null, clock: PlaybackClock()),
        PlaybackJournal(MemoryJournal()),
      );
  int stops = 0;
  @override
  Future<bool> open(Wire selection, {bool resume = false}) async {
    record = VodRecord(
      scope: PlaybackScope(
        origin: Uri.parse('http://localhost'),
        user: const Uuid().v7(),
        session: const Uuid().v7(),
        credential: '1',
        profile: const Uuid().v7(),
        revision: '1',
        context: 'sc1.${'a' * 43}',
      ),
      start: const Uuid().v7(),
    );
    state = 'uncertain';
    error = const VodFailure('network_uncertain');
    progressUncertain = true;
    notifyListeners();
    return true;
  }

  @override
  Future<void> close() async {
    stops++;
    cleanupOnly = true;
    state = 'pending';
    notifyListeners();
  }
}

void main() {
  testWidgets(
    'large RTL layout exposes status and keeps unconfirmed cleanup on screen',
    (tester) async {
      tester.view.physicalSize = const Size(320, 640);
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.resetPhysicalSize);
      addTearDown(tester.view.resetDevicePixelRatio);
      final commands = ScreenCommands();
      final service = PlaybackCoordinator(commands);
      await tester.pumpWidget(
        ProviderScope(
          overrides: [playbackCoordinatorProvider.overrideWithValue(service)],
          child: MaterialApp(
            localizationsDelegates: [const AppStringsDelegate()],
            supportedLocales: AppStrings.supportedLocales,
            builder: (context, child) => MediaQuery(
              data: MediaQuery.of(
                context,
              ).copyWith(textScaler: const TextScaler.linear(2)),
              child: Directionality(
                textDirection: TextDirection.rtl,
                child: child!,
              ),
            ),
            home: CommandPlaybackScreen(itemId: const Uuid().v7()),
          ),
        ),
      );
      await tester.pump();
      expect(tester.takeException(), isNull);
      expect(
        find.byWidgetPredicate(
          (widget) =>
              widget is Semantics && widget.properties.liveRegion == true,
        ),
        findsAtLeastNWidgets(2),
      );
      final stop = find.text('Stop and close');
      await tester.scrollUntilVisible(stop, 200, maxScrolls: 20);
      await tester.pumpAndSettle();
      await tester.tap(stop);
      await tester.pump();
      expect(commands.stops, 1);
      expect(find.byType(CommandPlaybackScreen), findsOneWidget);
      expect(find.text('Waiting for confirmed cleanup…'), findsOneWidget);
      expect(commands.record, isNotNull);
      expect(tester.takeException(), isNull);
      await tester.pumpWidget(const SizedBox());
      await tester.pump();
      expect(commands.stops, 1);
      service.dispose();
    },
  );
}

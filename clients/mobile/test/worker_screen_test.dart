// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:uuid/uuid.dart';
import 'package:duskcue_mobile/l10n/app_strings.dart';
import 'package:duskcue_mobile/playback/coordinator.dart';
import 'package:duskcue_mobile/playback/provider.dart';
import 'package:duskcue_mobile/playback/commands/wire.dart';
import 'package:duskcue_mobile/screens/command_playback_screen.dart';
import 'playback_screen_test.dart' show ScreenCommands;

class WaitingCommands extends ScreenCommands {
  @override
  Future<bool> open(Wire selection, {bool resume = false}) async {
    await super.open(selection, resume: resume);
    error = null;
    progressUncertain = false;
    state = 'checking_worker';
    workerObserving = true;
    notifyListeners();
    return true;
  }

  @override
  Future<void> close() async {
    workerObserving = false;
    await super.close();
  }
}

void main() {
  testWidgets(
    'worker wait exposes status, Pause and Close in large RTL layout',
    (tester) async {
      tester.view.physicalSize = const Size(320, 640);
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.resetPhysicalSize);
      addTearDown(tester.view.resetDevicePixelRatio);
      final commands = WaitingCommands();
      final coordinator = PlaybackCoordinator(commands);
      await tester.pumpWidget(
        ProviderScope(
          overrides: [
            playbackCoordinatorProvider.overrideWithValue(coordinator),
          ],
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
      expect(find.text('Checking playback status…'), findsOneWidget);
      expect(
        find.byWidgetPredicate(
          (w) => w is Semantics && w.properties.liveRegion == true,
        ),
        findsWidgets,
      );
      final pause = find.widgetWithText(FilledButton, 'Pause');
      await tester.scrollUntilVisible(pause, 150);
      expect(tester.widget<FilledButton>(pause).onPressed, isNotNull);
      await tester.tap(pause);
      await tester.pump();
      expect(coordinator.playingIntent, isFalse);
      expect(commands.workerObserving, isTrue);
      for (final label in [
        'Check status',
        'Audio',
        'Subtitles',
        '480p',
        '720p',
        '1080p',
      ]) {
        expect(
          tester
              .widget<OutlinedButton>(
                find.widgetWithText(OutlinedButton, label),
              )
              .onPressed,
          isNull,
        );
      }
      final stop = find.text('Stop and close');
      await tester.scrollUntilVisible(stop, 150);
      await tester.tap(stop);
      await tester.pump();
      expect(commands.stops, 1);
      expect(tester.takeException(), isNull);
      await tester.pumpWidget(const SizedBox());
      coordinator.dispose();
    },
  );
}

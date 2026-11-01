// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import 'dart:async';
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
import 'package:duskcue_mobile/screens/legacy_playback_entry_screen.dart';
import 'playback_commands_test.dart' show MemoryJournal;

class ScreenCommands extends PlaybackCommands {
  ScreenCommands({PlaybackTransport? transport})
    : super(
        transport ??
            PlaybackTransport(identity: () => null, clock: PlaybackClock()),
        PlaybackJournal(MemoryJournal()),
      );
  int stops = 0;
  int opens = 0;
  int checks = 0;
  @override
  Future<void> check({bool once = false}) async {
    checks++;
  }

  @override
  Future<bool> open(Wire selection, {bool resume = false}) async {
    opens++;
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

class TrackTransport extends PlaybackTransport {
  TrackTransport() : super(identity: () => null, clock: PlaybackClock());
  final file = const Uuid().v7();
  @override
  bool current(PlaybackScope expected, {bool forCleanup = false}) => true;
  @override
  Future<List<Wire>> files(String item, PlaybackScope scope) async => [
    {
      'id': file,
      'additional_streams': {
        'audio': [
          {'index': 1, 'language': 'en', 'codec': 'aac'},
        ],
      },
    },
  ];
}

class TrackScreenCommands extends ScreenCommands {
  TrackScreenCommands(TrackTransport transport) : super(transport: transport);
  @override
  Future<bool> open(Wire selection, {bool resume = false}) async {
    await super.open(selection);
    final ready = VodReady(
      const Uuid().v7(),
      const Uuid().v7(),
      const Uuid().v7(),
      (transport as TrackTransport).file,
      0,
      0,
      1,
      null,
    );
    admitted = ready;
    record = record!.copy(output: ready.output);
    state = 'unavailable';
    error = null;
    notifyListeners();
    return true;
  }
}

class HeldScreenCommands extends ScreenCommands {
  final result = Completer<bool>();
  @override
  Future<bool> open(Wire selection, {bool resume = false}) => result.future;
  Future<void> seed() async {
    await super.open({});
  }
}

Widget screenApp(PlaybackCoordinator service, String item) => ProviderScope(
  overrides: [playbackCoordinatorProvider.overrideWithValue(service)],
  child: MaterialApp(
    localizationsDelegates: [const AppStringsDelegate()],
    supportedLocales: AppStrings.supportedLocales,
    home: CommandPlaybackScreen(itemId: item),
  ),
);

void main() {
  testWidgets(
    'stale dialog and status callbacks cannot pop or act on a newer route/viewer',
    (tester) async {
      final commands = TrackScreenCommands(TrackTransport()),
          key = GlobalKey<NavigatorState>();
      final service = PlaybackCoordinator(commands);
      await tester.pumpWidget(
        ProviderScope(
          overrides: [playbackCoordinatorProvider.overrideWithValue(service)],
          child: MaterialApp(
            navigatorKey: key,
            localizationsDelegates: [const AppStringsDelegate()],
            supportedLocales: AppStrings.supportedLocales,
            home: CommandPlaybackScreen(itemId: const Uuid().v7()),
          ),
        ),
      );
      await tester.pump();
      final status = tester
          .widget<OutlinedButton>(
            find.widgetWithText(OutlinedButton, 'Check status'),
          )
          .onPressed!;
      await tester.tap(find.text('Audio'));
      await tester.pumpAndSettle();
      final select = tester
          .widget<SimpleDialogOption>(find.byType(SimpleDialogOption).first)
          .onPressed!;
      unawaited(
        key.currentState!.push(
          MaterialPageRoute<void>(
            builder: (_) => const Scaffold(body: Text('New route')),
          ),
        ),
      );
      await tester.pumpAndSettle();
      select();
      await tester.pumpAndSettle();
      expect(find.text('New route'), findsOneWidget);
      commands.invalidate();
      await commands.open({});
      final newer = commands.record;
      status();
      await tester.pumpAndSettle();
      expect(commands.checks, 0);
      expect(commands.record, same(newer));
      expect(commands.error, isNull);
      expect(find.text('New route'), findsOneWidget);
      await tester.pumpWidget(const SizedBox.shrink());
      expect(commands.stops, 0);
      service.dispose();
    },
  );
  testWidgets(
    'stale negotiation cannot select legacy, publish an error or Stop a newer viewer',
    (tester) async {
      final commands = HeldScreenCommands(), item = const Uuid().v7();
      final service = PlaybackCoordinator(commands);
      await tester.pumpWidget(screenApp(service, item));
      commands.invalidate();
      await commands.seed();
      final newer = commands.record;
      commands.error = null;
      commands.result.complete(false);
      await tester.pump();
      await tester.pump();
      expect(service.commands.record, same(newer));
      expect(commands.error, isNull);
      expect(find.byType(LegacyPlaybackEntryScreen), findsNothing);
      await tester.pumpWidget(const SizedBox.shrink());
      await tester.pump();
      expect(commands.stops, 0);
      service.dispose();
    },
  );
  testWidgets('a second screen cannot adopt or stop an existing viewer', (
    tester,
  ) async {
    final commands = ScreenCommands();
    final owner = PlaybackCoordinator(commands);
    await commands.open({});
    final existing = commands.record;
    commands.error = null;
    await tester.pumpWidget(screenApp(owner, const Uuid().v7()));
    await tester.pump();
    expect(commands.opens, 1);
    expect(commands.error, isNull);
    expect(commands.record, same(existing));
    expect(find.text('Audio'), findsNothing);
    await tester.pumpWidget(const SizedBox.shrink());
    expect(commands.stops, 0);
    owner.dispose();
  });
  testWidgets(
    'item-route replacement retains uncertain cleanup instead of starting the next item',
    (tester) async {
      final commands = ScreenCommands();
      final service = PlaybackCoordinator(commands);
      await tester.pumpWidget(screenApp(service, const Uuid().v7()));
      await tester.pump();
      final original = commands.record;
      await tester.pumpWidget(screenApp(service, const Uuid().v7()));
      await tester.pump();
      expect(commands.opens, 1);
      expect(commands.stops, 1);
      expect(commands.record, same(original));
      expect(commands.cleanupOnly, isTrue);
      await tester.pumpWidget(const SizedBox.shrink());
      service.dispose();
    },
  );
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

// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import 'dart:async';
import 'package:flutter_test/flutter_test.dart';
import 'package:video_player/video_player.dart';
import 'package:duskcue_mobile/playback/coordinator.dart';
import 'package:duskcue_mobile/playback/native_output.dart';
import 'package:duskcue_mobile/playback/commands/selection.dart';
import 'package:duskcue_mobile/playback/commands/wire.dart';
import 'playback_commands_test.dart' show CommandFixture;

class DelayedPlayer extends VideoPlayerController {
  DelayedPlayer(super.uri) : super.networkUrl();
  final initialized = Completer<void>();
  Completer<void>? disposal;
  int plays = 0, pauses = 0;
  bool disposed = false;
  @override
  Future<void> initialize() async {
    await initialized.future;
    value = value.copyWith(
      isInitialized: true,
      duration: const Duration(seconds: 20),
    );
  }

  @override
  Future<void> play() async {
    plays++;
    value = value.copyWith(isPlaying: true);
  }

  @override
  Future<void> pause() async {
    pauses++;
    value = value.copyWith(isPlaying: false);
  }

  @override
  Future<void> seekTo(Duration position) async {
    value = value.copyWith(position: position);
  }

  @override
  Future<void> dispose() async {
    await disposal?.future;
    disposed = true;
    await super.dispose();
  }
}

Future<void> until(bool Function() condition) async {
  final end = DateTime.now().add(const Duration(seconds: 3));
  while (!condition() && DateTime.now().isBefore(end)) {
    await Future<void>.delayed(const Duration(milliseconds: 5));
  }
  expect(condition(), isTrue);
}

void main() {
  test('Pause during initialization survives late native completion', () async {
    final fixture = CommandFixture();
    await fixture.open();
    final players = <DelayedPlayer>[];
    final coordinator = PlaybackCoordinator(
      fixture.commands,
      createPlayer: (uri) {
        final player = DelayedPlayer(uri);
        players.add(player);
        return player;
      },
    );
    try {
      await fixture.commands.open(androidSelection(fixture.item));
      await until(() => players.isNotEmpty);
      await coordinator.pause();
      players.single.initialized.complete();
      await until(() => !coordinator.attaching);
      expect(fixture.commands.error, isNull);
      expect(players.single.plays, 0);
      expect(players.single.value.isPlaying, isFalse);
      await coordinator.close();
      expect(players.single.disposed, isTrue);
    } finally {
      coordinator.dispose();
      await fixture.close(disposeCommands: false);
    }
  });
  test(
    'new output survives a stale attachment failure and waits for disposal',
    () async {
      final fixture = CommandFixture();
      await fixture.open();
      final players = <DelayedPlayer>[];
      final coordinator = PlaybackCoordinator(
        fixture.commands,
        createPlayer: (uri) {
          final player = DelayedPlayer(uri);
          players.add(player);
          return player;
        },
      );
      try {
        await fixture.commands.open(androidSelection(fixture.item));
        await until(() => players.isNotEmpty);
        final old = players.single;
        old.disposal = Completer<void>();
        await coordinator.seek(2000);
        final output = fixture.commands.record!.output;
        old.initialized.complete();
        await Future<void>.delayed(const Duration(milliseconds: 20));
        expect(players.length, 1);
        expect(coordinator.native.operations.count, 1);
        old.disposal!.complete();
        await until(() => players.length == 2);
        players.last.initialized.complete();
        await until(() => !coordinator.attaching);
        expect(fixture.commands.error, isNull);
        expect(coordinator.native.ready!.output, output);
        expect(old.plays, 0);
        expect(old.disposed, isTrue);
        expect(fixture.starts, 1);
        expect(fixture.changes, 1);
        await coordinator.close();
      } finally {
        coordinator.dispose();
        await fixture.close(disposeCommands: false);
      }
    },
  );
  test(
    'scope invalidation fences pending initialization and preserves cleanup only',
    () async {
      final fixture = CommandFixture();
      await fixture.open();
      late DelayedPlayer player;
      final coordinator = PlaybackCoordinator(
        fixture.commands,
        createPlayer: (uri) => player = DelayedPlayer(uri),
      );
      try {
        await fixture.commands.open(androidSelection(fixture.item));
        await until(() => coordinator.native.generation != null);
        coordinator.invalidate();
        player.initialized.complete();
        await until(() => !coordinator.attaching && player.disposed);
        expect(player.plays, 0);
        expect(coordinator.native.generation, isNull);
        expect(fixture.commands.cleanupOnly, isTrue);
        expect(fixture.starts, 1);
        await coordinator.close();
      } finally {
        coordinator.dispose();
        await fixture.close(disposeCommands: false);
      }
    },
  );
  test(
    'lost Start preserves the requested resume seek before first attachment',
    () async {
      final fixture = CommandFixture()..lostStart = true;
      await fixture.open();
      final players = <DelayedPlayer>[];
      final coordinator = PlaybackCoordinator(
        fixture.commands,
        createPlayer: (uri) {
          expect(fixture.changes, 1);
          final player = DelayedPlayer(uri);
          players.add(player);
          return player;
        },
      );
      try {
        await coordinator.open(fixture.item);
        expect(fixture.commands.state, 'uncertain');
        expect(players, isEmpty);
        await fixture.commands.check(once: true);
        await until(() => players.isNotEmpty);
        players.single.initialized.complete();
        await until(() => !coordinator.attaching);
        await fixture.commands.check(once: true);
        expect(fixture.starts, 1);
        expect(fixture.changes, 1);
        expect(fixture.commands.error, isNull);
        await coordinator.close();
      } finally {
        coordinator.dispose();
        await fixture.close(disposeCommands: false);
      }
    },
  );
  test(
    'native timeout retains ownership until initialization and disposal finish',
    () async {
      final fixture = CommandFixture();
      await fixture.open();
      await fixture.commands.open(androidSelection(fixture.item));
      late DelayedPlayer player;
      final native = NativeOutput(
        fixture.transport,
        (_) => true,
        (_, _, _) {},
        () {},
        createPlayer: (uri) => player = DelayedPlayer(uri),
      );
      try {
        final attaching = native.attach(
          fixture.commands.record!,
          fixture.commands.receipt!,
          play: () => true,
        );
        await expectLater(
          attaching,
          throwsA(
            isA<VodFailure>().having(
              (error) => error.code,
              'code',
              'observation_timeout',
            ),
          ),
        );
        expect(native.operations.count, 1);
        expect(native.quarantined, isTrue);
        expect(player.disposed, isFalse);
        await expectLater(
          native.attach(
            fixture.commands.record!,
            fixture.commands.receipt!,
            play: () => true,
          ),
          throwsA(isA<VodFailure>()),
        );
        player.initialized.complete();
        await native.retire();
        await native.operations.drain();
        expect(player.plays, 0);
        expect(player.disposed, isTrue);
        expect(native.generation, isNull);
        expect(native.operations.count, 0);
      } finally {
        await native.retire();
        await fixture.close();
      }
    },
    timeout: const Timeout(Duration(seconds: 45)),
  );
}

// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import 'dart:async';
import 'package:flutter_test/flutter_test.dart';
import 'package:duskcue_mobile/playback/coordinator.dart';
import 'package:duskcue_mobile/playback/commands/selection.dart';
import 'playback_commands_test.dart' show CommandFixture;
import 'playback_native_test.dart' show DelayedPlayer, until;

class WorkerNativeFixture {
  final backend = CommandFixture();
  final players = <DelayedPlayer>[];
  late final PlaybackCoordinator coordinator;
  Future<void> open() async {
    await backend.open();
    coordinator = PlaybackCoordinator(
      backend.commands,
      createPlayer: (uri) {
        final player = DelayedPlayer(uri);
        players.add(player);
        return player;
      },
    );
    await backend.commands.open(androidSelection(backend.item));
    await until(() => players.isNotEmpty);
    players.single.initialized.complete();
    await until(() => !coordinator.attaching);
    backend.commands.observe(backend.output, 600, 'playing');
    await backend.commands.checkpoints.flush(force: true);
  }

  void failPlayer() {
    players.first.value = players.first.value.copyWith(
      errorDescription: 'generic native failure',
    );
  }

  Future<void> finishReplacement() async {
    await until(() => players.length == 2);
    players.last.initialized.complete();
    await until(() => !coordinator.attaching);
  }

  Future<void> close() async {
    for (final player in players) {
      if (!player.initialized.isCompleted) player.initialized.complete();
      if (player.disposal != null && !player.disposal!.isCompleted) {
        player.disposal!.complete();
      }
    }
    await coordinator.close();
    coordinator.dispose();
    await backend.close(disposeCommands: false);
  }
}

void main() {
  test(
    'native error waits for proof and replaces only the failed generation',
    () async {
      final f = WorkerNativeFixture();
      await f.open();
      try {
        final history = f.backend.commands.admitted!.history;
        f.backend.beforeConfiguration = () async {
          if (f.backend.configurationReads >= 2) {
            f.backend.workerRecovery = true;
          }
        };
        f.failPlayer();
        await f.finishReplacement();
        expect(f.players.first.disposed, isTrue);
        expect(f.players.last.value.isPlaying, isTrue);
        expect(f.backend.commands.workerObserver.negatives, 1);
        expect(f.backend.starts, 1);
        expect(f.backend.changes, 1);
        expect(f.backend.commands.admitted!.history, history);
        expect(f.backend.commands.error, isNull);
      } finally {
        await f.close();
      }
    },
  );
  test(
    'Pause during worker observation preserves a paused replacement',
    () async {
      final f = WorkerNativeFixture(), gate = Completer<void>();
      await f.open();
      try {
        f.backend.beforeConfiguration = () => gate.future;
        f.failPlayer();
        await until(() => f.backend.configurationReads == 1);
        await f.coordinator.pause();
        expect(f.backend.commands.workerObserving, isTrue);
        f.backend.workerRecovery = true;
        gate.complete();
        await f.finishReplacement();
        expect(f.players.last.plays, 0);
        expect(f.players.last.value.isPlaying, isFalse);
        expect(f.backend.changes, 1);
      } finally {
        if (!gate.isCompleted) gate.complete();
        await f.close();
      }
    },
  );
  test(
    'background cancels observation and status cannot attach without Play',
    () async {
      final f = WorkerNativeFixture(), gate = Completer<void>();
      await f.open();
      try {
        f.backend.beforeConfiguration = () => gate.future;
        f.failPlayer();
        await until(() => f.backend.configurationReads == 1);
        f.coordinator.lifecycle(false);
        await until(() => !f.backend.commands.workerObserving);
        gate.complete();
        await f.backend.commands.workerObserver.drain();
        expect(f.coordinator.requiresPlay, isTrue);
        expect(f.backend.commands.lease.running, isTrue);
        expect(f.backend.changes, 0);
        f.coordinator.lifecycle(true);
        await until(() => f.backend.commands.canAct);
        await f.backend.commands.check(once: true);
        expect(f.players.length, 1);
        final play = f.coordinator.play();
        await f.finishReplacement();
        await play;
        expect(f.players.last.value.isPlaying, isTrue);
        expect(f.backend.starts, 1);
      } finally {
        if (!gate.isCompleted) gate.complete();
        await f.close();
      }
    },
  );
  test(
    'unconfirmed native cleanup blocks all evidence and replacement',
    () async {
      final f = WorkerNativeFixture();
      await f.open();
      try {
        f.players.first.disposal = Completer<void>();
        f.failPlayer();
        await Future<void>.delayed(const Duration(milliseconds: 5200));
        expect(f.backend.commands.error!.code, 'native_cleanup_pending');
        expect(f.coordinator.requiresPlay, isTrue);
        expect(f.backend.configurationReads, 0);
        expect(f.backend.changes, 0);
        f.players.first.disposal!.complete();
        await f.coordinator.native.retire();
        expect(f.players.length, 1);
        expect(f.players.first.disposed, isTrue);
      } finally {
        await f.close();
      }
    },
  );
}

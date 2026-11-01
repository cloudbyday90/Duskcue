// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import 'dart:async';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:duskcue_mobile/models/auth_models.dart';
import 'package:duskcue_mobile/playback/coordinator.dart';
import 'package:duskcue_mobile/playback/provider.dart';
import 'package:duskcue_mobile/playback/commands/wire.dart';
import 'package:duskcue_mobile/services/api_client.dart';
import 'package:duskcue_mobile/services/service_providers.dart';
import 'package:duskcue_mobile/stores/session_store.dart';
import 'playback_commands_test.dart' show CommandFixture;
import 'playback_native_test.dart' show DelayedPlayer, until;

class ProviderFixture {
  final backend = CommandFixture(), api = DuskcueApiClient();
  final players = <DelayedPlayer>[];
  late final ProviderContainer container;
  PlaybackCoordinator get current =>
      container.read(playbackCoordinatorProvider);
  Future<void> open() async {
    await backend.open();
    api.configure(backend.identity!.origin, bearerToken: 'test-secret');
    container = ProviderContainer(
      overrides: [
        apiClientProvider.overrideWithValue(api),
        playbackJournalStorageProvider.overrideWithValue(backend.storage),
        playbackPlayerFactoryProvider.overrideWithValue((uri) {
          final player = DelayedPlayer(uri);
          players.add(player);
          return player;
        }),
      ],
    );
    final session = container.read(sessionProvider.notifier);
    session.setAuthenticated(
      UserSummary(
        id: backend.user,
        username: 'fixture',
        displayName: 'Fixture',
        role: 'user',
        capabilities: const [],
        hasAllLibraryAccess: true,
        activeProfileId: backend.profile,
      ),
    );
    session.resolveProfileScope(
      activeProfileId: backend.profile,
      profileSelectionRequired: false,
    );
  }

  Future<void> attach() async {
    await current.open(backend.item);
    await until(
      () => players.isNotEmpty && !players.last.initialized.isCompleted,
    );
    players.last.initialized.complete();
    await until(() => !current.attaching);
  }

  Future<void> close() async {
    for (final player in players) {
      if (!player.initialized.isCompleted) player.initialized.complete();
      if (player.disposal != null && !player.disposal!.isCompleted) {
        player.disposal!.complete();
      }
    }
    backend.storage.gate = null;
    await current.close();
    container.dispose();
    await backend.close();
  }
}

void main() {
  test(
    'unsupported context cannot select legacy while native cleanup remains owned',
    () async {
      final f = ProviderFixture();
      await f.open();
      final claim = f.current.commands.lifetime.native.acquire(() async {});
      try {
        f.backend.beforeReply = (request) async {
          if (request.uri.path.endsWith('/session-context')) {
            request.response.statusCode = 404;
          }
        };
        expect(await f.current.open(f.backend.item), isTrue);
        expect(f.current.commands.error!.code, 'native_cleanup_pending');
        expect(f.backend.starts, 0);
        claim.release();
        expect(await f.current.open(f.backend.item), isFalse);
        expect(f.backend.starts, 0);
      } finally {
        claim.release();
        await f.close();
      }
    },
  );
  test(
    'recreated transports share admission and preserve reserved cleanup capacity',
    () async {
      final f = ProviderFixture(), gate = Completer<void>();
      await f.open();
      final lifetime = f.current.commands.lifetime;
      final held = List.generate(
        3,
        (_) => lifetime.regular.run(() => gate.future),
      );
      try {
        final old = f.current;
        f.container.invalidate(playbackCoordinatorProvider);
        final fresh = f.current;
        await fresh.open(f.backend.item);
        expect(fresh.commands.error!.code, 'busy');
        expect(f.backend.starts, 0);
        expect(lifetime.regular.count, 3);
        expect(lifetime.regular.closed, isFalse);
        await old.commands.transport.dispose();
        await fresh.commands.transport.context(forCleanup: true);
        expect(lifetime.regular.count, 3);
        expect(lifetime.cleanup.count, 0);
        gate.complete();
        await Future.wait(held);
        await f.attach();
        expect(f.backend.starts, 1);
      } finally {
        if (!gate.isCompleted) gate.complete();
        await Future.wait(held);
        await f.close();
      }
    },
  );
  test(
    'native command completion remains owned across coordinator replacement',
    () async {
      final f = ProviderFixture(), gate = Completer<void>();
      await f.open();
      try {
        await f.attach();
        final first = f.players.single, old = f.current;
        first.pauseGate = gate;
        final paused = old.pause();
        await until(() => first.pauses > 0);
        f.api.setBearerToken('test-secret');
        f.container.invalidate(playbackCoordinatorProvider);
        final fresh = f.current, lifetime = fresh.commands.lifetime;
        await lifetime.journal.drain();
        await fresh.open(f.backend.item);
        expect(fresh.commands.error!.code, 'native_cleanup_pending');
        expect(first.disposed, isFalse);
        expect(f.backend.starts, 1);
        gate.complete();
        await paused;
        await lifetime.native.drain();
        expect(first.disposed, isTrue);
        expect(f.backend.starts, 1);
        await f.attach();
        expect(f.backend.starts, 2);
        expect(fresh.commands.error, isNull);
      } finally {
        if (!gate.isCompleted) gate.complete();
        await f.close();
      }
    },
  );
  test(
    'actual provider retains journal cleanup across credential and coordinator replacement',
    () async {
      final f = ProviderFixture(), gate = Completer<void>();
      await f.open();
      try {
        final old = f.current,
            lifetime = f.container.read(playbackLifetimeProvider);
        f.backend.storage.gate = gate;
        final pending = old.open(f.backend.item);
        final observed = expectLater(pending, throwsA(isA<VodFailure>()));
        await until(() => f.backend.storage.calls.contains('write'));
        f.api.setBearerToken('test-secret');
        f.container.invalidate(playbackCoordinatorProvider);
        final fresh = f.current;
        expect(identical(fresh.commands.lifetime, lifetime), isTrue);
        await fresh.open(f.backend.item);
        expect(fresh.commands.error!.code, 'storage_busy');
        expect(f.backend.starts, 0);
        gate.complete();
        await observed;
        await lifetime.journal.drain();
        expect(f.backend.storage.value, isNull);
        expect(f.backend.starts, 0);
        expect(lifetime.regular.closed, isFalse);
        await f.attach();
        expect(f.backend.starts, 1);
        expect(fresh.commands.error, isNull);
        expect(fresh.commands.deliverable, isTrue);
      } finally {
        if (!gate.isCompleted) gate.complete();
        await f.close();
      }
    },
  );
  test(
    'actual provider refuses a new viewer until old native disposal completes',
    () async {
      final f = ProviderFixture(), gate = Completer<void>();
      await f.open();
      try {
        await f.attach();
        final old = f.current, first = f.players.single;
        final lifetime = f.container.read(playbackLifetimeProvider);
        first.disposal = gate;
        f.api.setBearerToken('test-secret');
        f.container.invalidate(playbackCoordinatorProvider);
        final fresh = f.current;
        await lifetime.journal.drain();
        await fresh.open(f.backend.item);
        expect(fresh.commands.error!.code, 'native_cleanup_pending');
        expect(f.backend.starts, 1);
        expect(lifetime.native.held, isTrue);
        expect(identical(old, fresh), isFalse);
        gate.complete();
        await lifetime.native.drain();
        expect(f.backend.starts, 1);
        expect(first.disposed, isTrue);
        await f.attach();
        expect(f.backend.starts, 2);
        expect(fresh.commands.error, isNull);
        expect(lifetime.regular.closed, isFalse);
      } finally {
        if (!gate.isCompleted) gate.complete();
        await f.close();
      }
    },
  );
}

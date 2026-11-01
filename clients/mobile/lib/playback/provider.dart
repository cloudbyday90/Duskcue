// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import 'dart:async';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:video_player/video_player.dart';
import '../services/service_providers.dart';
import '../stores/session_store.dart';
import 'commands/clock.dart';
import 'commands/controller.dart';
import 'commands/journal.dart';
import 'commands/scope.dart';
import 'commands/transport.dart';
import 'coordinator.dart';
import 'lifetime.dart';

final playbackJournalStorageProvider = Provider<JournalStorage>(
  (ref) => SecureJournalStorage(),
);
final playbackPlayerFactoryProvider =
    Provider<VideoPlayerController Function(Uri)?>((ref) => null);
final playbackLifetimeProvider = Provider<PlaybackLifetime>((ref) {
  final lifetime = PlaybackLifetime(
    PlaybackJournal(ref.watch(playbackJournalStorageProvider)),
  );
  ref.onDispose(() {
    unawaited(lifetime.close().catchError((Object _) {}));
  });
  return lifetime;
});

final playbackCoordinatorProvider = Provider<PlaybackCoordinator>((ref) {
  final api = ref.watch(apiClientProvider);
  final lifetime = ref.watch(playbackLifetimeProvider);
  PlaybackIdentity? identity() {
    final session = ref.read(sessionProvider),
        origin = api.serverOrigin,
        token = api.bearerToken;
    final user = session.user;
    if (origin == null ||
        token == null ||
        user == null ||
        !session.isAuthenticated) {
      return null;
    }
    return PlaybackIdentity(
      origin: origin,
      token: token,
      credentialEpoch: api.credentialEpoch,
      profileEpoch: api.profileEpoch,
      user: user.id,
      profile: user.activeProfileId,
      ready: session.isProfileScopeReady,
    );
  }

  final transport = PlaybackTransport(
    identity: identity,
    clock: PlaybackClock(),
    regularBudget: lifetime.regular,
    cleanupBudget: lifetime.cleanup,
  );
  final service = PlaybackCoordinator(
    PlaybackCommands(transport, lifetime.journal, lifetime: lifetime),
    createPlayer: ref.watch(playbackPlayerFactoryProvider),
  );
  final subscription = api.scopeChanges.listen((_) => service.invalidate());
  ref.listen(sessionProvider, (previous, next) {
    if (previous?.user?.id != next.user?.id ||
        previous?.user?.activeProfileId != next.user?.activeProfileId ||
        previous?.isProfileScopeReady != next.isProfileScopeReady ||
        previous?.server != next.server) {
      service.invalidate();
    }
  });
  ref.onDispose(() {
    unawaited(subscription.cancel());
    service.dispose();
  });
  return service;
});

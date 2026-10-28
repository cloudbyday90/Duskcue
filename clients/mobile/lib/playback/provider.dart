// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import 'dart:async';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import '../services/service_providers.dart';
import '../stores/session_store.dart';
import 'commands/clock.dart';
import 'commands/controller.dart';
import 'commands/journal.dart';
import 'commands/scope.dart';
import 'commands/transport.dart';
import 'coordinator.dart';

final playbackCoordinatorProvider = Provider<PlaybackCoordinator>((ref) {
  final api = ref.watch(apiClientProvider);
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
  );
  final service = PlaybackCoordinator(
    PlaybackCommands(transport, PlaybackJournal(SecureJournalStorage())),
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

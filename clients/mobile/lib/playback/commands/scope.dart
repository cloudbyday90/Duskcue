// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import 'wire.dart';

class PlaybackIdentity {
  const PlaybackIdentity({
    required this.origin,
    required this.token,
    required this.credentialEpoch,
    required this.profileEpoch,
    required this.user,
    required this.profile,
    required this.ready,
  });
  final Uri origin;
  final String token;
  final int credentialEpoch;
  final int profileEpoch;
  final String user;
  final String profile;
  final bool ready;

  bool matches(PlaybackIdentity? other, {bool cleanup = false}) =>
      other != null &&
      origin.origin == other.origin.origin &&
      token == other.token &&
      credentialEpoch == other.credentialEpoch &&
      user == other.user &&
      (cleanup ||
          ready &&
              other.ready &&
              profile == other.profile &&
              profileEpoch == other.profileEpoch);
}

class PlaybackScope {
  const PlaybackScope({
    required this.origin,
    required this.user,
    required this.session,
    required this.credential,
    required this.profile,
    required this.revision,
    required this.context,
  });
  final Uri origin;
  final String user;
  final String session;
  final String credential;
  final String profile;
  final String revision;
  final String context;

  factory PlaybackScope.fromJson(Wire value) {
    exactKeys(value, {
      'origin',
      'user',
      'session',
      'credential',
      'profile',
      'revision',
      'context',
      'transport',
    });
    final raw = text(value['origin'], max: 2048);
    final origin = Uri.tryParse(raw);
    require(
      origin != null &&
          const ['http', 'https'].contains(origin.scheme) &&
          origin.host.isNotEmpty &&
          origin.userInfo.isEmpty &&
          origin.origin == raw &&
          value['transport'] == 'bearer',
    );
    final context = text(value['context'], max: 47);
    require(RegExp(r'^sc1\.[A-Za-z0-9_-]{43}$').hasMatch(context));
    return PlaybackScope(
      origin: origin!,
      user: entity(value['user']),
      session: entity(value['session']),
      credential: revisionValue(value['credential']),
      profile: entity(value['profile']),
      revision: revisionValue(value['revision']),
      context: context,
    );
  }

  bool matches(PlaybackScope other, {bool cleanup = false}) =>
      origin.origin == other.origin.origin &&
      user == other.user &&
      session == other.session &&
      credential == other.credential &&
      (cleanup ||
          profile == other.profile &&
              revision == other.revision &&
              context == other.context);

  Wire toJson() => {
    'origin': origin.origin,
    'user': user,
    'session': session,
    'credential': credential,
    'transport': 'bearer',
    'profile': profile,
    'revision': revision,
    'context': context,
  };
}

class PlaybackContext {
  const PlaybackContext(
    this.scope,
    this.identity,
    this.observedAt,
    this.expiresAt,
    this.began,
  );
  final PlaybackScope scope;
  final PlaybackIdentity identity;
  final DateTime observedAt;
  final DateTime expiresAt;
  final int began;

  factory PlaybackContext.parse(
    Wire value,
    PlaybackIdentity local,
    int began, {
    bool cleanup = false,
  }) {
    require(
      value['transport'] == 'bearer' &&
          value['user_id'] == local.user &&
          value['profile_selection_required'] is bool,
    );
    if (local.ready && !cleanup) {
      require(
        value['profile_selection_required'] == false &&
            value['profile_id'] == local.profile,
        'scope_changed',
      );
    }
    final scope = PlaybackScope.fromJson({
      'origin': local.origin.origin,
      'user': value['user_id'],
      'session': value['session_id'],
      'credential': value['credential_revision'],
      'transport': value['transport'],
      'profile': value['profile_id'],
      'revision': value['profile_context_revision'],
      'context': value['context_id'],
    });
    final observed = instant(value['observed_at']);
    final expires = instant(value['session_expires_at']);
    require(expires.isAfter(observed));
    return PlaybackContext(scope, local, observed, expires, began);
  }
}

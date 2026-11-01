// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import 'dart:async';
import 'dart:convert';
import 'dart:io';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:video_player/video_player.dart';
import 'package:duskcue_mobile/models/auth_models.dart';
import 'package:duskcue_mobile/playback/commands/journal.dart';
import 'package:duskcue_mobile/playback/commands/wire.dart';
import 'package:duskcue_mobile/playback/coordinator.dart';
import 'package:duskcue_mobile/playback/provider.dart';
import 'package:duskcue_mobile/services/api_client.dart';
import 'package:duskcue_mobile/services/service_providers.dart';
import 'package:duskcue_mobile/stores/session_store.dart';

class AcknowledgedStorage implements JournalStorage {
  final delegate = SecureJournalStorage();
  Completer<void>? writeGate, written;
  @override
  Future<String?> read() => delegate.read();
  @override
  Future<void> clear() => delegate.clear();
  @override
  Future<void> write(String value) async {
    await delegate.write(value);
    final gate = writeGate;
    if (gate != null) {
      written!.complete();
      await gate.future;
      writeGate = null;
    }
  }
}

class AcknowledgedPlayer extends VideoPlayerController {
  AcknowledgedPlayer(super.uri)
    : super.networkUrl(
        formatHint: VideoFormat.hls,
        videoPlayerOptions: VideoPlayerOptions(allowBackgroundPlayback: true),
      );
  Completer<void>? gate;
  bool delegateDisposed = false;
  @override
  Future<void> dispose() async {
    await super.dispose();
    delegateDisposed = true;
    await gate?.future;
  }
}

class ScopeHarness {
  ScopeHarness(this.tester, this.fixture) {
    api.configure(
      Uri.parse(text(fixture['origin'])),
      bearerToken: text(fixture['token']),
    );
    container = ProviderContainer(
      overrides: [
        apiClientProvider.overrideWithValue(api),
        playbackJournalStorageProvider.overrideWithValue(storage),
        playbackPlayerFactoryProvider.overrideWithValue((uri) {
          final player = AcknowledgedPlayer(uri);
          players.add(player);
          return player;
        }),
      ],
    );
    adopt(
      entity(fixture['user']),
      entity(
        fixture['scenario'] == 'scope-session'
            ? fixture['peer_profile']
            : fixture['profile'],
      ),
    );
  }
  final WidgetTester tester;
  final Wire fixture;
  final api = DuskcueApiClient(), storage = AcknowledgedStorage();
  final players = <AcknowledgedPlayer>[];
  late final ProviderContainer container;
  PlaybackCoordinator get current =>
      container.read(playbackCoordinatorProvider);
  void adopt(String user, String profile) {
    final session = container.read(sessionProvider.notifier);
    session.setAuthenticated(
      UserSummary(
        id: user,
        username: 'qualification',
        displayName: 'Qualification',
        role: 'member',
        capabilities: const ['play_media'],
        hasAllLibraryAccess: true,
        activeProfileId: profile,
      ),
    );
    session.resolveProfileScope(
      activeProfileId: profile,
      profileSelectionRequired: false,
    );
  }

  Future<void> until(bool Function() condition) async {
    final deadline = DateTime.now().add(const Duration(seconds: 30));
    while (!condition() && DateTime.now().isBefore(deadline)) {
      await tester.pump(const Duration(milliseconds: 100));
    }
    expect(
      condition(),
      isTrue,
      reason:
          'Scope evidence incomplete: ${current.commands.state}/${current.commands.error?.code}',
    );
  }

  Future<void> open() async {
    await current.open(entity(fixture['item']));
    await until(
      () =>
          current.commands.deliverable &&
          !current.attaching &&
          current.native.controller?.value.isInitialized == true,
    );
    await until(
      () => current.native.controller!.value.position.inMilliseconds >= 250,
    );
    expect(current.commands.error, isNull);
  }

  Future<void> close() async {
    for (final player in players) {
      if (player.gate != null && !player.gate!.isCompleted) {
        player.gate!.complete();
      }
    }
    if (storage.writeGate != null && !storage.writeGate!.isCompleted) {
      storage.writeGate!.complete();
    }
    await current.close();
    await tester.pumpWidget(const SizedBox.shrink());
    container.dispose();
  }
}

Future<(int, Wire)> scopeHttp(
  Uri origin,
  String token,
  String path, {
  Wire? body,
  String? context,
}) async {
  require(origin.host == '127.0.0.1' && origin.scheme == 'http');
  final client = HttpClient()..autoUncompress = false;
  final deadline = Timer(
    const Duration(seconds: 5),
    () => client.close(force: true),
  );
  try {
    final request = await client.openUrl(
      body == null ? 'GET' : 'POST',
      origin.resolve(path),
    );
    request.followRedirects = false;
    request.headers.set('authorization', 'Bearer $token');
    request.headers.set('accept-encoding', 'identity');
    request.headers.set('accept', 'application/json, application/problem+json');
    if (context != null) request.headers.set('x-session-context', context);
    if (body != null) {
      request.headers.contentType = ContentType.json;
      request.add(utf8.encode(jsonEncode(body)));
    }
    final response = await request.close().timeout(const Duration(seconds: 5));
    final coding = response.headers.value('content-encoding');
    require(coding == null || coding == 'identity');
    final bytes = <int>[];
    await for (final chunk in response) {
      require(bytes.length + chunk.length <= 32768);
      bytes.addAll(chunk);
    }
    return (
      response.statusCode,
      bytes.isEmpty ? <String, Object?>{} : decodeWire(bytes),
    );
  } finally {
    deadline.cancel();
    client.close(force: true);
  }
}

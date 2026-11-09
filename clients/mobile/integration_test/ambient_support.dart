// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import 'dart:io';
import 'dart:typed_data';
import 'package:dio/dio.dart';
import 'package:dio/io.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:duskcue_mobile/models/auth_models.dart';
import 'package:duskcue_mobile/models/profile_models.dart';
import 'package:duskcue_mobile/playback/commands/wire.dart';
import 'package:duskcue_mobile/screens/ambient_player_screen.dart';
import 'package:duskcue_mobile/services/ambient_playback_service.dart';
import 'package:duskcue_mobile/services/api_client.dart';
import 'package:duskcue_mobile/services/service_providers.dart';
import 'package:duskcue_mobile/stores/session_store.dart';
import 'scope_support.dart';

class AmbientHarness {
  AmbientHarness._(this.tester, this.fixture, this.api, this.dio) {
    container = ProviderContainer(overrides: [apiClientProvider.overrideWithValue(api)]);
    adopt(entity(fixture['profile']));
    service = container.read(ambientPlaybackServiceProvider);
  }
  static Future<AmbientHarness> create(WidgetTester tester, MethodChannel channel, Wire fixture) async {
    final ca = await channel.invokeMethod<Uint8List>('ambientFixtureCa');
    require(ca != null && ca.length <= 4096);
    final trust = SecurityContext(withTrustedRoots: true)..setTrustedCertificatesBytes(ca!);
    final dio = Dio()..httpClientAdapter = IOHttpClientAdapter(createHttpClient: () => HttpClient(context: trust));
    final api = DuskcueApiClient(dio: dio)..configure(Uri.parse(text(fixture['origin'])), bearerToken: text(fixture['token']));
    return AmbientHarness._(tester, fixture, api, dio);
  }
  final WidgetTester tester;
  final Wire fixture;
  final DuskcueApiClient api;
  final Dio dio;
  late final ProviderContainer container;
  late final AmbientPlaybackService service;
  NativeAmbientPlaybackStatus status = const NativeAmbientPlaybackStatus(isActive: false);

  void adopt(String profile) {
    container.read(sessionProvider.notifier).setAuthenticated(UserSummary(id: entity(fixture['user']), username: 'qualification',
      displayName: 'Qualification', role: 'member', capabilities: const ['play_media'], hasAllLibraryAccess: true, activeProfileId: profile));
    container.read(sessionProvider.notifier).resolveProfileScope(activeProfileId: profile, profileSelectionRequired: false);
  }
  Future<void> screen() => tester.pumpWidget(UncontrolledProviderScope(container: container, child: const MaterialApp(home: AmbientPlayerScreen())));
  Future<void> until(bool Function(NativeAmbientPlaybackStatus) predicate, {int seconds = 30}) async {
    final deadline = DateTime.now().add(Duration(seconds: seconds));
    do {
      status = await service.status();
      if (predicate(status)) return;
      await tester.pump(const Duration(milliseconds: 200));
    } while (DateTime.now().isBefore(deadline));
    fail('Ambient evidence incomplete: ${status.state}/${status.error}');
  }
  Future<void> open({String? channel}) async {
    await service.start(AmbientChannelSummary(id: channel ?? entity(fixture['channel']), name: 'Qualification', audience: 'standard', isEnabled: true, itemCount: 2));
    await screen();
    await until((s) => s.state == 'ready' && !s.busy && s.videoFrames > 2 && s.positionMs >= 250);
    expect(status.protocol, 'duskcue-vod-command/2');
    expect(status.error, isNull);
  }
  Future<Wire> serverStatus(String start, {String suffix = ''}) async {
    final origin = Uri.parse(text(fixture['http_origin']));
    final token = api.bearerToken!;
    final context = (await scopeHttp(origin, token, '/api/v1/auth/session-context')).$2;
    final reply = await scopeHttp(origin, token, '/api/v1/playback-commands/$start$suffix', context: text(context['context_id']));
    expect(reply.$1, 200);
    return reply.$2;
  }
  Future<void> stop() async {
    final start = status.requestId;
    await service.stop();
    await until((s) => !s.isActive && !s.cleanupPending);
    if (start != null) expect((await serverStatus(start, suffix: '/settlement'))['status'], 'settled');
  }
  Future<void> dispose({bool retain = false}) async {
    try { if (!retain) await stop(); }
    finally { await tester.pumpWidget(const SizedBox.shrink()); container.dispose(); dio.close(force: true); }
  }
}

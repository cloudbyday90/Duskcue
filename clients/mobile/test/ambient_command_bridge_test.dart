// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import 'dart:async';
import 'package:duskcue_mobile/models/profile_models.dart';
import 'package:duskcue_mobile/services/ambient_playback_service.dart';
import 'package:duskcue_mobile/services/api_client.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();
  const channel = MethodChannel('duskcue/ambient_player');
  final messenger = TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger;
  final services = <AmbientPlaybackService>[];
  const lineup = AmbientChannelSummary(id: '019a6000-0000-7000-8000-000000000001', name: 'Channel',
    audience: 'standard', isEnabled: true, itemCount: 1);
  DuskcueApiClient api() => DuskcueApiClient()..configure(Uri.parse('https://duskcue.example'), bearerToken: 'fixture-token');
  AmbientPlaybackService create(DuskcueApiClient client, {bool android = false, AmbientClientScope? Function()? scope}) {
    final value = AmbientPlaybackService(apiClient: client, channel: channel, nativeAndroid: android, scope: scope);
    services.add(value);
    return value;
  }
  tearDown(() {
    for (final service in services) { service.dispose(); }
    services.clear();
    messenger.setMockMethodCallHandler(channel, null);
  });

  test('Stop has independent admission while Start acknowledgement is pending', () async {
    final held = Completer<Object?>();
    final dispatched = Completer<void>();
    final calls = <String>[];
    messenger.setMockMethodCallHandler(channel, (call) async {
      calls.add(call.method);
      if (call.method == 'start') { dispatched.complete(); return held.future; }
      return null;
    });
    final service = create(api());
    final start = service.start(lineup);
    await dispatched.future;
    await service.stop();
    expect(calls, ['start', 'stop']);
    held.complete(null);
    await start;
  });

  test('overlapping status requests share one actual platform operation', () async {
    final held = Completer<Object?>();
    var count = 0;
    messenger.setMockMethodCallHandler(channel, (_) async { count++; return held.future; });
    final service = create(api());
    final first = service.status(), second = service.status();
    await Future<void>.delayed(Duration.zero);
    expect(count, 1);
    held.complete({'is_active': false});
    expect((await first).isActive, isFalse);
    expect((await second).isActive, isFalse);
  });

  test('a late status reply cannot enter a newer account scope', () async {
    final held = Completer<Object?>();
    messenger.setMockMethodCallHandler(channel, (_) async => held.future);
    final client = api();
    final service = create(client);
    final read = service.status();
    final refused = expectLater(read, throwsStateError);
    client.setBearerToken('replacement-token');
    held.complete({'is_active': true, 'channel_name': 'Old channel'});
    await refused;
  });

  test('Android binds exact current runtime and cannot reactivate a logout epoch', () async {
    final calls = <MethodCall>[];
    messenger.setMockMethodCallHandler(channel, (call) async { calls.add(call); return null; });
    final client = api();
    final service = create(client, android: true, scope: () => const AmbientClientScope(
      '019a6000-0000-7000-8000-000000000002', '019a6000-0000-7000-8000-000000000003', ready: true));
    await service.start(lineup);
    expect(calls.map((call) => call.method), ['bind', 'start']);
    final runtime = calls.last.arguments as Map;
    expect(runtime['scope_epoch'], '1:0');
    expect(runtime['user_id'], '019a6000-0000-7000-8000-000000000002');
    await service.fenceForLogout();
    expect(calls.where((call) => call.method == 'fence').last.arguments, isNull);
    await expectLater(service.start(lineup), throwsStateError);
    expect(calls.where((call) => call.method == 'start').length, 1);
  });

  test('status rejects oversized metadata and invalid positions', () {
    expect(() => NativeAmbientPlaybackStatus.fromJson({'position_ms': -1}), throwsFormatException);
    expect(() => NativeAmbientPlaybackStatus.fromJson({'channel_name': 'x' * 257}), throwsFormatException);
    expect(() => NativeAmbientPlaybackStatus.fromJson({'state': 'invented'}), throwsFormatException);
    expect(() => NativeAmbientPlaybackStatus.fromJson({'cleanup_pending': 'false'}), throwsFormatException);
  });

  test('a new scope cannot start through an older pending bind', () async {
    final held = Completer<Object?>();
    final current = Completer<Object?>();
    final calls = <MethodCall>[];
    messenger.setMockMethodCallHandler(channel, (call) async {
      calls.add(call);
      if (call.method == 'bind' && calls.length == 1) return held.future;
      if (call.method == 'bind') return current.future;
      return null;
    });
    final client = api();
    final service = create(client, android: true, scope: () => const AmbientClientScope(
      '019a6000-0000-7000-8000-000000000002', '019a6000-0000-7000-8000-000000000003', ready: true));
    client.setBearerToken('replacement-token');
    final pending = service.start(lineup);
    final refused = expectLater(pending, throwsStateError);
    held.complete(null);
    await refused;
    await Future<void>.delayed(Duration.zero);
    expect(calls.where((call) => call.method == 'start'), isEmpty);
    expect((calls.last.arguments as Map)['bearer_token'], 'replacement-token');
    current.complete(null);
    await Future<void>.delayed(Duration.zero);
    await service.start(lineup);
    expect(calls.where((call) => call.method == 'start').length, 1);
  });

  test('controls carry the exact currently bound native runtime', () async {
    final calls = <MethodCall>[];
    messenger.setMockMethodCallHandler(channel, (call) async { calls.add(call); return call.method == 'status' ? {'is_active': false} : null; });
    final service = create(api(), android: true, scope: () => const AmbientClientScope(
      '019a6000-0000-7000-8000-000000000002', '019a6000-0000-7000-8000-000000000003', ready: true));
    await service.pause();
    await service.seek(1234);
    await service.status();
    for (final call in calls.where((call) => call.method != 'bind')) {
      final runtime = (call.arguments as Map)['runtime'] as Map;
      expect(runtime['scope_epoch'], '1:0');
      expect(runtime['user_id'], '019a6000-0000-7000-8000-000000000002');
    }
    expect((calls.firstWhere((call) => call.method == 'seek').arguments as Map)['position_ms'], 1234);
  });

  for (final change in ['account', 'profile', 'server', 'logout']) {
    test('$change fences native playback before the old bind settles', () async {
      final held = Completer<Object?>();
      final calls = <MethodCall>[];
      var binds = 0;
      messenger.setMockMethodCallHandler(channel, (call) async {
        calls.add(call);
        if (call.method == 'bind' && binds++ == 0) return held.future;
        return null;
      });
      final client = api();
      var user = '019a6000-0000-7000-8000-000000000002';
      final service = create(client, android: true, scope: () => AmbientClientScope(
        user, '019a6000-0000-7000-8000-000000000003', ready: true));
      try {
        await Future<void>.delayed(Duration.zero);
        switch (change) {
          case 'account':
            user = '019a6000-0000-7000-8000-000000000004';
            client.setBearerToken('new-account-token');
          case 'profile': client.fenceProfile();
          case 'server': client.configure(Uri.parse('https://replacement.example'), bearerToken: 'new-server-token');
          case 'logout': await service.fenceForLogout();
        }
        await Future<void>.delayed(Duration.zero);
        expect(held.isCompleted, isFalse);
        expect(calls.map((call) => call.method), ['bind', 'fence']);
        final runtime = calls.last.arguments as Map?;
        if (change == 'logout') expect(runtime, isNull);
        if (change == 'account') expect(runtime?['user_id'], user);
        if (change == 'profile') expect(runtime?['scope_epoch'], '1:1');
        if (change == 'server') expect(runtime?['server_origin'], 'https://replacement.example');
      } finally {
        held.complete(null);
        await Future<void>.delayed(Duration.zero);
      }
    });
  }

  test('coalesced fences keep restoration closed until the newest fence is acknowledged', () async {
    final first = Completer<Object?>(), last = Completer<Object?>();
    final calls = <MethodCall>[];
    var fences = 0;
    messenger.setMockMethodCallHandler(channel, (call) async {
      calls.add(call);
      if (call.method == 'fence') return ++fences == 1 ? first.future : last.future;
      return null;
    });
    final client = api();
    create(client, android: true, scope: () => const AmbientClientScope(
      '019a6000-0000-7000-8000-000000000002', '019a6000-0000-7000-8000-000000000003', ready: true));
    await Future<void>.delayed(Duration.zero);
    client.setBearerToken('intermediate-token');
    client.setBearerToken('latest-token');
    await Future<void>.delayed(Duration.zero);
    expect(fences, 1);
    first.complete(null);
    await Future<void>.delayed(Duration.zero);
    expect(fences, 2);
    expect(calls.where((call) => call.method == 'bind').length, 1);
    expect((calls.last.arguments as Map)['bearer_token'], 'latest-token');
    last.complete(null);
    await Future<void>.delayed(Duration.zero);
    expect(calls.where((call) => call.method == 'bind').length, 2);
  });

  test('unchanged wrapper recreation binds without forcing a native fence', () async {
    final calls = <String>[];
    messenger.setMockMethodCallHandler(channel, (call) async { calls.add(call.method); return null; });
    final client = api();
    AmbientClientScope scope() => const AmbientClientScope(
      '019a6000-0000-7000-8000-000000000002', '019a6000-0000-7000-8000-000000000003', ready: true);
    final previous = create(client, android: true, scope: scope);
    await Future<void>.delayed(Duration.zero);
    previous.dispose();
    services.remove(previous);
    create(client, android: true, scope: scope);
    await Future<void>.delayed(Duration.zero);
    expect(calls, ['bind', 'bind']);
  });

  test('logout waits for its null fence rather than an older acknowledgement', () async {
    final older = Completer<Object?>(), logout = Completer<Object?>();
    final calls = <MethodCall>[];
    var fences = 0;
    messenger.setMockMethodCallHandler(channel, (call) async {
      calls.add(call);
      if (call.method == 'fence') return ++fences == 1 ? older.future : logout.future;
      return null;
    });
    final client = api();
    final service = create(client, android: true, scope: () => const AmbientClientScope(
      '019a6000-0000-7000-8000-000000000002', '019a6000-0000-7000-8000-000000000003', ready: true));
    await Future<void>.delayed(Duration.zero);
    client.fenceProfile();
    var confirmed = false;
    final pending = service.fenceForLogout().then((_) => confirmed = true);
    older.complete(null);
    await Future<void>.delayed(Duration.zero);
    expect(fences, 2);
    expect(calls.last.arguments, isNull);
    expect(confirmed, isFalse);
    logout.complete(null);
    await pending;
    expect(confirmed, isTrue);
  });
}

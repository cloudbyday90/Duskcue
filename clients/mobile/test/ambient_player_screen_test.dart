// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import 'dart:async';
import 'package:duskcue_mobile/models/profile_models.dart';
import 'package:duskcue_mobile/screens/ambient_player_screen.dart';
import 'package:duskcue_mobile/services/ambient_playback_service.dart';
import 'package:duskcue_mobile/services/api_client.dart';
import 'package:duskcue_mobile/services/service_providers.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:go_router/go_router.dart';

class FakeAmbient extends AmbientPlaybackService {
  FakeAmbient() : super(apiClient: DuskcueApiClient(), nativeAndroid: false);
  bool refuseStop = true, refuseRead = false;
  int reads = 0, stops = 0;
  Completer<void>? stopGate;
  @override Future<NativeAmbientPlaybackStatus> status() async {
    reads++;
    if (refuseRead) throw StateError('Unavailable');
    return const NativeAmbientPlaybackStatus(isActive: true, protocol: 'duskcue-vod-command/2', state: 'ready',
      channelName: 'Channel', positionMs: 1000, durationMs: 9000);
  }
  @override Future<void> stop() async { stops++; await stopGate?.future; if (refuseStop) throw StateError('Unconfirmed'); }
  @override Future<void> check() async { }
}

void main() {
  Future<GoRouter> show(WidgetTester tester, FakeAmbient service, {double scale = 1, bool rtl = false}) async {
    final router = GoRouter(initialLocation: '/ambient/player', routes: [
      GoRoute(path: '/ambient/player', builder: (_, _) => const AmbientPlayerScreen()),
      GoRoute(path: '/ambient', builder: (_, _) => const Scaffold(body: Text('Channels page'))),
    ]);
    await tester.pumpWidget(ProviderScope(overrides: [ambientPlaybackServiceProvider.overrideWithValue(service)],
      child: MaterialApp.router(routerConfig: router, builder: (context, child) => MediaQuery(
        data: MediaQuery.of(context).copyWith(textScaler: TextScaler.linear(scale)), child: Directionality(textDirection: rtl ? TextDirection.rtl : TextDirection.ltr, child: child!)))));
    await tester.pumpAndSettle();
    return router;
  }

  testWidgets('failed Stop stays visible; confirmed Stop permits navigation', (tester) async {
    final service = FakeAmbient();
    final router = await show(tester, service);
    await tester.ensureVisible(find.text('Stop channel'));
    await tester.tap(find.text('Stop channel'));
    await tester.pumpAndSettle();
    expect(router.routeInformationProvider.value.uri.path, '/ambient/player');
    expect(find.text('Channel cleanup is not yet confirmed. Check status or try Stop again.'), findsOneWidget);
    service.refuseStop = false;
    await tester.ensureVisible(find.text('Stop channel'));
    await tester.tap(find.text('Stop channel'));
    await tester.pumpAndSettle();
    expect(router.routeInformationProvider.value.uri.path, '/ambient');
    expect(service.stops, 2);
    await tester.pumpWidget(const SizedBox());
    router.dispose(); service.dispose();
  });

  testWidgets('failed status stops automatic polling and clears the channel', (tester) async {
    final service = FakeAmbient()..refuseRead = true;
    final router = await show(tester, service);
    expect(find.text('Playback status unavailable'), findsOneWidget);
    await tester.pump(const Duration(seconds: 10));
    expect(service.reads, 1);
    expect(find.text('Channel'), findsNothing);
    service.refuseRead = false;
    await tester.ensureVisible(find.text('Check status'));
    await tester.tap(find.text('Check status'));
    await tester.pumpAndSettle();
    expect(find.text('Channel'), findsOneWidget);
    await tester.pumpWidget(const SizedBox());
    router.dispose(); service.dispose();
  });

  testWidgets('large text keeps controls scrollable without overflow', (tester) async {
    tester.view.physicalSize = const Size(320, 600);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.resetPhysicalSize);
    addTearDown(tester.view.resetDevicePixelRatio);
    final service = FakeAmbient();
    final router = await show(tester, service, scale: 2.4);
    await tester.ensureVisible(find.text('Stop channel'));
    expect(tester.takeException(), isNull);
    final semantics = tester.ensureSemantics();
    await expectLater(tester, meetsGuideline(labeledTapTargetGuideline));
    semantics.dispose();
    await tester.pumpWidget(const SizedBox());
    router.dispose(); service.dispose();
  });

  testWidgets('late confirmed Stop cannot navigate a newer viewing session', (tester) async {
    final service = FakeAmbient()..refuseStop = false..stopGate = Completer<void>();
    final router = await show(tester, service);
    await tester.ensureVisible(find.text('Stop channel'));
    await tester.tap(find.text('Stop channel'));
    await tester.pump();
    service.scopeChanged();
    service.stopGate!.complete();
    await tester.pumpAndSettle();
    expect(router.routeInformationProvider.value.uri.path, '/ambient/player');
    expect(find.text('The viewing session changed. Check status to review any remaining cleanup.'), findsOneWidget);
    await tester.pumpWidget(const SizedBox());
    router.dispose(); service.dispose();
  });

  testWidgets('RTL status recovery preserves the focused control', (tester) async {
    final service = FakeAmbient();
    final router = await show(tester, service, rtl: true);
    await tester.ensureVisible(find.text('Check status'));
    final focus = Focus.of(tester.element(find.text('Check status')));
    focus.requestFocus();
    await tester.pump();
    expect(focus.hasFocus, isTrue);
    await tester.tap(find.text('Check status'));
    await tester.pumpAndSettle();
    expect(focus.hasFocus, isTrue);
    expect(tester.takeException(), isNull);
    await tester.pumpWidget(const SizedBox());
    router.dispose(); service.dispose();
  });
}

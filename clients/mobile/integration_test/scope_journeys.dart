// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import 'dart:async';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:duskcue_mobile/playback/commands/wire.dart';
import 'package:duskcue_mobile/playback/provider.dart';
import 'package:duskcue_mobile/services/service_providers.dart';
import 'package:duskcue_mobile/stores/session_store.dart';
import 'scope_support.dart';

Future<void> runScopeJourney(
  WidgetTester tester,
  IntegrationTestWidgetsFlutterBinding binding,
  MethodChannel channel,
  Wire fixture,
) async {
  final f = ScopeHarness(tester, fixture);
  try {
    final origin = f.api.serverOrigin!, token = f.api.bearerToken!;
    final lifetime = f.container.read(playbackLifetimeProvider);
    if (fixture['scenario'] == 'scope-profile') {
      await f.open();
      final old = f.current, start = f.current.commands.record!.start;
      final output = old.commands.receipt!.ready!.output;
      await scopeHttp(
        origin,
        token,
        '/qualification/scope',
        body: {'action': 'arm'},
      );
      final pending = old.commands.check(once: true);
      var held = false;
      for (var i = 0; i < 30 && !held; i++) {
        held =
            (await scopeHttp(
              origin,
              token,
              '/qualification/scope',
            )).$2['held'] ==
            true;
        if (!held) await tester.pump(const Duration(milliseconds: 100));
      }
      expect(held, isTrue);
      final selected = await f.container
          .read(profileServiceProvider)
          .switchProfile(entity(fixture['peer_profile']));
      f.container
          .read(sessionProvider.notifier)
          .resolveProfileScope(
            activeProfileId: selected.activeProfile.id,
            profileSelectionRequired: false,
          );
      await pending;
      await lifetime.native.drain();
      expect(old.commands.cleanupOnly, isTrue);
      expect(old.commands.record!.start, start);
      expect(old.commands.receipt, isNull);
      expect(old.commands.lease.running, isFalse);
      final context = await old.commands.transport.context(forCleanup: true);
      final refused = await scopeHttp(
        origin,
        token,
        '/api/v1/playback-commands/$start/outputs/$output/hls/manifest.m3u8',
        context: context.scope.context,
      );
      expect(refused.$1, isNot(200));
      await old.close();
      expect(old.commands.record, isNull);
      f.container.invalidate(playbackCoordinatorProvider);
      expect(identical(f.current.commands.lifetime, lifetime), isTrue);
      await f.open();
      expect(f.current.commands.record!.start, isNot(start));
      expect(f.current.commands.record!.scope.profile, fixture['peer_profile']);
      await f.current.close();
      final witness = await scopeHttp(origin, token, '/qualification/scope');
      expect(witness.$2['cancelled'], isTrue);
      binding.reportData = {
        'scenario': fixture['scenario'],
        'real_provider': true,
        'old_output_refused': true,
        'no_replay': true,
        'confirmed_cleanup': true,
        'profile_cleanup_only': true,
        'http_cancelled': true,
      };
    } else {
      final old = f.current;
      f.storage.writeGate = Completer<void>();
      f.storage.written = Completer<void>();
      final opening = old.open(entity(fixture['item']));
      final observed = expectLater(opening, throwsA(isA<VodFailure>()));
      await f.storage.written!.future;
      await f.api.post('/api/v1/auth/logout');
      f.api.setBearerToken(text(fixture['peer_token']));
      f.adopt(entity(fixture['user']), entity(fixture['peer_profile']));
      f.container.invalidate(playbackCoordinatorProvider);
      await f.current.open(entity(fixture['item']));
      expect(f.current.commands.error!.code, 'storage_busy');
      f.storage.writeGate!.complete();
      await observed;
      await lifetime.journal.drain();
      expect(await f.storage.read(), isNull);
      expect(
        (await scopeHttp(origin, token, '/api/v1/auth/session-context')).$1,
        401,
      );
      await f.open();
      final outgoing = f.current,
          start = outgoing.commands.record!.start,
          output = outgoing.commands.record!.output!;
      final player = f.players.last;
      player.gate = Completer<void>();
      await f.api.post('/api/v1/auth/logout');
      f.api.configure(
        Uri.parse(text(fixture['alternate_origin'])),
        bearerToken: text(fixture['other_token']),
      );
      f.adopt(entity(fixture['other_user']), entity(fixture['other_profile']));
      f.container.invalidate(playbackCoordinatorProvider);
      final fresh = f.current;
      await lifetime.journal.drain();
      await fresh.open(entity(fixture['item']));
      expect(fresh.commands.error!.code, 'native_cleanup_pending');
      expect(lifetime.native.held, isTrue);
      await f.until(() => player.delegateDisposed);
      expect(lifetime.native.held, isTrue);
      player.gate!.complete();
      await lifetime.native.drain();
      expect(fresh.commands.record, isNull);
      final refused = await scopeHttp(
        origin,
        text(fixture['peer_token']),
        '/api/v1/playback-commands/$start/outputs/$output/hls/manifest.m3u8',
      );
      expect(refused.$1, 401);
      await f.open();
      expect(fresh.commands.record!.scope.user, fixture['other_user']);
      expect(fresh.commands.record!.scope.origin, f.api.serverOrigin);
      await fresh.close();
      expect(fresh.commands.record, isNull);
      binding.reportData = {
        'scenario': fixture['scenario'],
        'real_provider': true,
        'old_output_refused': true,
        'no_replay': true,
        'confirmed_cleanup': true,
        'unsent_cleared': true,
        'native_barrier': true,
        'strong_scope_clear': true,
        'storage_delegate_completed_before_ack': true,
        'native_delegate_completed_before_ack': true,
      };
    }
    await channel.invokeMethod<void>('clearBootstrap');
  } finally {
    await f.close();
  }
}

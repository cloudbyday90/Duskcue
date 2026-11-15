// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import 'dart:async';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:duskcue_mobile/playback/commands/wire.dart';
import 'package:duskcue_mobile/services/service_providers.dart';
import 'package:duskcue_mobile/stores/session_store.dart';
import 'ambient_support.dart';
import 'ambient_concurrency.dart';
import 'scope_support.dart';

Future<void> runAmbientJourney(
  WidgetTester tester,
  IntegrationTestWidgetsFlutterBinding binding,
  MethodChannel channel,
  Wire fixture,
) async {
  final f = await AmbientHarness.create(tester, channel, fixture);
  final scenario = text(fixture['scenario']);
  var retain = false;
  final evidence = <String, Object?>{
    'scenario': scenario,
    'native_owner': true,
  };
  try {
    switch (scenario) {
      case 'ambient-background':
        final probes = await channel.invokeMapMethod<String, Object?>(
          'ambientProbe',
        );
        for (final key in [
          'strict_wire',
          'uuidv7_clock',
          'atomic_hls',
          'retained_operations',
          'joined_loaders',
          'atomic_journal',
          'native_http',
          'native_tls',
          'retained_io',
          'terminal_publication',
          'notification_identity',
          'tls_name_refused',
          'unsupported_sources_refused',
          'strict_formats',
        ]) {
          expect(probes?[key], isTrue, reason: key);
        }
        expect(probes?['lost_post_count'], 1);
        await f.open();
        final start = f.status.requestId!;
        final before = await f.serverStatus(start);
        await tester.pumpWidget(const SizedBox.shrink());
        debugPrint('ANDROID_PHASE ambient-background');
        await tester.pump(const Duration(seconds: 22));
        await f.until((s) => s.requestId == start && s.isPlaying);
        final after = await f.serverStatus(start);
        expect(
          DateTime.parse(
            text(after['lease_deadline']),
          ).isAfter(DateTime.parse(text(before['lease_deadline']))),
          isTrue,
        );
        await f.screen();
        evidence.addAll({
          'native_http_tls': true,
          'negative_tls': true,
          'owned_probe_completion': true,
          'background_renewal': true,
          'unsupported_sources_refused': true,
          'strict_formats': true,
        });
      case 'ambient-controls':
        await f.open();
        final start = f.status.requestId!, output = f.status.outputRevision!;
        final originalNotification = await _captureNotification(
          tester,
          channel,
          start,
          output,
        );
        await _sendNotification(channel, originalNotification);
        await f.until((s) => !s.isPlaying && !s.busy);
        final before = await f.serverStatus(start);
        await tester.pump(const Duration(seconds: 16));
        final after = await f.serverStatus(start);
        expect(
          DateTime.parse(
            text(after['lease_deadline']),
          ).isAfter(DateTime.parse(text(before['lease_deadline']))),
          isTrue,
        );
        try {
          await f.service.seek(5000);
        } on TimeoutException {
          final pending = await f.service.status();
          expect(pending.requestId, start);
          expect(pending.busy || pending.state == 'ready', isTrue);
          expect(pending.error, isNull);
        } catch (_) {
          final refused = await f.service.status();
          fail('Ambient seek observation: ${refused.state}/${refused.error}');
        }
        await f.until(
          (s) =>
              s.outputRevision != output &&
              !s.busy &&
              s.state == 'ready' &&
              !s.isPlaying,
        );
        expect(f.status.requestId, start);
        final replacementOutput = f.status.outputRevision!;
        final currentNotification = await _captureNotification(
          tester,
          channel,
          start,
          replacementOutput,
        );
        expect(currentNotification, isNot(originalNotification));
        await _sendNotification(channel, originalNotification);
        await _sendNotification(channel, originalNotification, dismiss: true);
        await tester.pump(const Duration(seconds: 1));
        await f.until(
          (s) =>
              s.requestId == start &&
              s.outputRevision == replacementOutput &&
              s.isActive &&
              !s.isPlaying &&
              !s.busy,
        );
        await _sendNotification(channel, currentNotification);
        await f.until(
          (s) => s.isPlaying && s.positionMs >= 5000 && s.videoFrames > 2,
        );
        await _sendNotification(channel, currentNotification, dismiss: true);
        await f.until((s) => !s.isActive && !s.cleanupPending);
        expect(
          (await f.serverStatus(start, suffix: '/settlement'))['status'],
          'settled',
        );
        await channel.invokeMethod<void>('ambientNotificationClear');
        evidence.addAll({
          'paused_renewal': true,
          'same_viewer_seek': true,
          'paused_intent': true,
          'stale_sdk_notifications_refused': true,
          'current_sdk_notifications_applied': true,
        });
      case 'ambient-advance':
        await f.open(channel: entity(fixture['next_channel']));
        final first = f.status.requestId!;
        await f.until(
          (s) =>
              s.requestId != null &&
              s.requestId != first &&
              s.state == 'ready' &&
              !s.busy &&
              s.videoFrames > 2,
          seconds: 40,
        );
        final second = f.status.requestId!;
        await f.service.pause();
        expect(
          (await f.serverStatus(first, suffix: '/settlement'))['status'],
          'settled',
        );
        await f.open();
        expect(f.status.requestId, isNot(second));
        expect(
          (await f.serverStatus(second, suffix: '/settlement'))['status'],
          'settled',
        );
        evidence.addAll({'natural_advance': true, 'replacement_cleanup': true});
      case 'ambient-cold-retain':
        await f.open();
        retain = true;
        evidence.addAll({
          'start': f.status.requestId!,
          'retained_for_process_death': true,
        });
      case 'ambient-cold-restore':
        await f.until(
          (s) => s.cleanupPending && s.requestId == fixture['expected_start'],
        );
        expect(f.status.isPlaying, isFalse);
        expect(f.status.outputRevision, isNotNull);
        final old = await f.serverStatus(f.status.requestId!);
        expect(old['status'], isNot('ready'));
        evidence.addAll({
          'start': f.status.requestId!,
          'cleanup_only_restore': true,
          'missed_lease_refused': true,
        });
      case 'ambient-scope':
        f.api.setBearerToken(text(fixture['peer_token']));
        await tester.pump(const Duration(seconds: 1));
        await f.open();
        final old = f.status.requestId!;
        final selected = await f.container
            .read(profileServiceProvider)
            .switchProfile(entity(fixture['peer_profile']));
        f.container
            .read(sessionProvider.notifier)
            .resolveProfileScope(
              activeProfileId: selected.activeProfile.id,
              profileSelectionRequired: false,
            );
        await f.until((s) => s.cleanupPending && !s.isPlaying);
        await f.stop();
        final settled = await f.api.get<Map<String, dynamic>>(
          '/api/v1/playback-commands/$old/settlement',
        );
        expect(settled.data?['status'], 'settled');
        await f.open();
        final next = f.status.requestId!;
        await f.api.post('/api/v1/auth/logout');
        f.api.setBearerToken(text(fixture['token']));
        await f.until((s) => !s.isPlaying && !s.isActive && !s.cleanupPending);
        final refused = await scopeHttp(
          Uri.parse(text(fixture['http_origin'])),
          text(fixture['peer_token']),
          '/api/v1/playback-commands/$next',
        );
        expect(refused.$1, 401);
        evidence.addAll({
          'profile_cleanup_only': true,
          'strong_scope_clear': true,
        });
      case 'ambient-concurrency':
        await ambientConcurrency(tester, fixture, f);
        evidence['independent_interactive'] = true;
      default:
        fail('Unknown ambient journey');
    }
    if (!retain) {
      await f.stop();
      evidence['confirmed_cleanup'] = true;
    }
    binding.reportData = evidence;
  } finally {
    await f.dispose(retain: retain);
  }
}

Future<int> _captureNotification(
  WidgetTester tester,
  MethodChannel channel,
  String start,
  String output,
) async {
  final deadline = DateTime.now().add(const Duration(seconds: 5));
  do {
    final index = await channel
        .invokeMethod<int>('ambientNotificationCapture', {
          'start': start,
          'output': output,
        })
        .timeout(const Duration(seconds: 5));
    if (index != null) return index;
    await tester.pump(const Duration(milliseconds: 100));
  } while (DateTime.now().isBefore(deadline));
  fail('The exact current SDK media notification was not published.');
}

Future<void> _sendNotification(
  MethodChannel channel,
  int index, {
  bool dismiss = false,
}) => channel
    .invokeMethod<void>('ambientNotificationSend', {
      'index': index,
      'dismiss': dismiss,
    })
    .timeout(const Duration(seconds: 5));

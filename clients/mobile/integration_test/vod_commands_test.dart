// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import 'dart:convert';
import 'dart:ui' as ui;
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:duskcue_mobile/l10n/app_strings.dart';
import 'package:duskcue_mobile/playback/commands/clock.dart';
import 'package:duskcue_mobile/playback/commands/controller.dart';
import 'package:duskcue_mobile/playback/commands/journal.dart';
import 'package:duskcue_mobile/playback/commands/scope.dart';
import 'package:duskcue_mobile/playback/commands/transport.dart';
import 'package:duskcue_mobile/playback/commands/wire.dart';
import 'package:duskcue_mobile/playback/coordinator.dart';
import 'package:duskcue_mobile/playback/provider.dart';
import 'package:duskcue_mobile/playback/tracks.dart';
import 'package:duskcue_mobile/screens/command_playback_screen.dart';
import 'package:video_player/video_player.dart';
import 'authored_frame.dart';
import 'storage_scenarios.dart';
import 'lost_outcomes.dart';
import 'worker_recovery.dart';

void main() {
  final binding = IntegrationTestWidgetsFlutterBinding.ensureInitialized();
  const channel = MethodChannel('duskcue/qualification');
  testWidgets('actual Android encoded HLS retains viewer and confirms cleanup', (
    tester,
  ) async {
    final raw = await channel.invokeMethod<String>('bootstrap');
    require(raw != null && utf8.encode(raw).length <= 4096);
    final fixture = decodeWire(utf8.encode(raw!));
    final origin = Uri.parse(text(fixture['origin']));
    require(origin.host == '127.0.0.1' && origin.scheme == 'http');
    final identity = PlaybackIdentity(
      origin: origin,
      token: text(fixture['token'], max: 2048),
      credentialEpoch: 1,
      profileEpoch: 1,
      user: entity(fixture['user']),
      profile: entity(fixture['profile']),
      ready: true,
    );
    final commands = PlaybackCommands(
      PlaybackTransport(identity: () => identity, clock: PlaybackClock()),
      PlaybackJournal(SecureJournalStorage()),
    );
    final coordinator = PlaybackCoordinator(commands);
    if (fixture['scenario'] == 'worker-recovery') {
      await runWorkerRecovery(
        tester,
        binding,
        channel,
        fixture,
        commands,
        coordinator,
      );
      return;
    }
    if (fixture['scenario'] == 'lost-outcomes') {
      await runLostOutcomes(
        tester,
        binding,
        channel,
        fixture,
        commands,
        coordinator,
      );
      return;
    }
    if (fixture['scenario'] != 'baseline') {
      await runStorageScenario(
        tester,
        binding,
        channel,
        fixture,
        commands,
        coordinator,
      );
      return;
    }
    final attachmentClock = Stopwatch();
    var maximumAttachmentMs = 0;
    void observeAttachment() {
      if (coordinator.attaching && !attachmentClock.isRunning) {
        attachmentClock
          ..reset()
          ..start();
      } else if (!coordinator.attaching && attachmentClock.isRunning) {
        attachmentClock.stop();
        if (attachmentClock.elapsedMilliseconds > maximumAttachmentMs) {
          maximumAttachmentMs = attachmentClock.elapsedMilliseconds;
        }
      }
    }

    coordinator.addListener(observeAttachment);
    Future<void> until(bool Function() condition, {int seconds = 30}) async {
      final end = DateTime.now().add(Duration(seconds: seconds));
      while (!condition() && DateTime.now().isBefore(end)) {
        await tester.pump(const Duration(milliseconds: 100));
      }
      expect(
        condition(),
        isTrue,
        reason:
            'Bounded native observation failed: state=${commands.state}, '
            'code=${commands.error?.code}, busy=${commands.busy}, '
            'monitoring=${commands.monitoring}, attaching=${coordinator.attaching}, '
            'attachmentMs=${attachmentClock.elapsedMilliseconds}, '
            'nativeInitialized=${coordinator.native.controller?.value.isInitialized}',
      );
    }

    try {
      await tester.pumpWidget(
        ProviderScope(
          overrides: [
            playbackCoordinatorProvider.overrideWithValue(coordinator),
          ],
          child: MaterialApp(
            localizationsDelegates: [const AppStringsDelegate()],
            supportedLocales: AppStrings.supportedLocales,
            home: CommandPlaybackScreen(itemId: entity(fixture['item'])),
          ),
        ),
      );
      await until(
        () =>
            commands.deliverable &&
            !coordinator.native.quarantined &&
            coordinator.native.generation?.disposed == false &&
            coordinator.native.controller?.value.isInitialized == true &&
            !coordinator.attaching,
      );
      final firstGeneration = coordinator.native.generation!;
      final start = commands.record!.start,
          history = commands.receipt!.ready!.history;
      await until(
        () =>
            identical(coordinator.native.generation, firstGeneration) &&
            !firstGeneration.disposed &&
            firstGeneration.player.value.position.inMilliseconds >= 500,
      );
      await binding.convertFlutterSurfaceToImage();
      await tester.pump();
      final beforeMs = firstGeneration.player.value.position.inMilliseconds;
      final frame = await binding.takeScreenshot('android-decoded-frame');
      final afterMs = firstGeneration.player.value.position.inMilliseconds;
      require(
        identical(coordinator.native.generation, firstGeneration) &&
            !firstGeneration.disposed,
      );
      require(frame.length <= 16 * 1024 * 1024);
      final codec = await ui.instantiateImageCodec(Uint8List.fromList(frame));
      final image = (await codec.getNextFrame()).image;
      final pixels = await image.toByteData(format: ui.ImageByteFormat.rawRgba);
      require(pixels != null);
      final rect = tester.getRect(find.byType(VideoPlayer));
      final scale = tester.view.devicePixelRatio;
      final decoded = verifyAuthoredFrame(
        pixels!,
        width: image.width,
        height: image.height,
        video: rect,
        scale: scale,
        beforeMs: beforeMs,
        afterMs: afterMs,
      );
      image.dispose();
      codec.dispose();
      await coordinator.pause();
      await tester.pump();
      expect(coordinator.native.controller!.value.isPlaying, isFalse);
      await commands.checkpoints.flush(force: true);
      final previous = commands.record!.output;
      await coordinator.seek(2000);
      await until(
        () =>
            !coordinator.attaching &&
            coordinator.native.ready?.output == commands.record?.output &&
            commands.record?.output != previous,
      );
      expect(coordinator.native.controller!.value.isPlaying, isFalse);
      expect(commands.record!.start, start);
      expect(commands.receipt!.ready!.history, history);
      final audio = await playbackTracks(commands, audio: true);
      final subtitles = await playbackTracks(commands, audio: false);
      expect(audio.length, 2);
      expect(subtitles.length, 2);
      final changedFrom = commands.record!.output;
      final selectedAudio = audio
          .firstWhere((track) => track.index != commands.admitted!.audio)
          .index;
      await coordinator.configure(
        audio: selectedAudio,
        subtitle: subtitles.last.index,
        resolution: 480,
      );
      await until(
        () =>
            !coordinator.attaching &&
            coordinator.native.ready?.output == commands.record?.output &&
            commands.record?.output != changedFrom,
      );
      expect(commands.admitted!.audio, selectedAudio);
      expect(commands.admitted!.subtitle, subtitles.last.index);
      expect(coordinator.native.controller!.value.isPlaying, isFalse);
      expect(commands.record!.start, start);
      expect(commands.receipt!.ready!.history, history);
      final oldOwner = coordinator.native.generation!;
      var observedBackground = false;
      void observeLifecycle() {
        if (coordinator.background) observedBackground = true;
      }

      coordinator.addListener(observeLifecycle);
      debugPrint('ANDROID_PHASE background');
      await until(
        () => observedBackground && !coordinator.background,
        seconds: 15,
      );
      await until(() => oldOwner.disposed, seconds: 15);
      coordinator.removeListener(observeLifecycle);
      expect(coordinator.requiresPlay, isTrue);
      expect(coordinator.native.generation, isNull);
      await until(() => commands.canAct);
      await coordinator.play();
      await until(() => coordinator.native.controller?.value.isPlaying == true);
      await coordinator.close();
      expect(commands.record, isNull);
      expect(coordinator.native.generation, isNull);
      await channel.invokeMethod<void>('clearBootstrap');
      binding.reportData = {
        ...?binding.reportData,
        'scenario': 'baseline',
        'authored_picture': decoded,
        'protocol_two': true,
        'decoded_authored_frame': true,
        'same_viewer': true,
        'paused_seek': true,
        'paused_tracks_quality': true,
        'explicit_background_play': true,
        'confirmed_cleanup': true,
        'maximum_native_attachment_ms': maximumAttachmentMs,
      };
    } finally {
      coordinator.removeListener(observeAttachment);
      await coordinator.close();
      coordinator.dispose();
    }
  }, timeout: const Timeout(Duration(minutes: 3)));
}

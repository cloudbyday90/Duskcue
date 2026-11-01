// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import 'dart:ui' as ui;
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:video_player/video_player.dart';
import 'package:duskcue_mobile/l10n/app_strings.dart';
import 'package:duskcue_mobile/playback/commands/controller.dart';
import 'package:duskcue_mobile/playback/commands/wire.dart';
import 'package:duskcue_mobile/playback/coordinator.dart';
import 'package:duskcue_mobile/playback/provider.dart';
import 'package:duskcue_mobile/screens/command_playback_screen.dart';
import 'authored_frame.dart';
import 'relay_stress.dart';

Future<void> runRelayJourney(
  WidgetTester tester,
  IntegrationTestWidgetsFlutterBinding binding,
  MethodChannel channel,
  Wire fixture,
  PlaybackCommands commands,
  PlaybackCoordinator coordinator,
) async {
  Future<void> until(bool Function() predicate) async {
    final end = DateTime.now().add(const Duration(seconds: 30));
    while (!predicate() && DateTime.now().isBefore(end)) {
      await tester.pump(const Duration(milliseconds: 100));
    }
    expect(
      predicate(),
      isTrue,
      reason:
          'Relay journey state=${commands.state}, error=${commands.error?.code}, attaching=${coordinator.attaching}',
    );
  }

  final formats = fixture['scenario'] == 'relay-formats';
  final frames = <Map<String, Object>>[];
  try {
    await tester.pumpWidget(
      ProviderScope(
        overrides: [playbackCoordinatorProvider.overrideWithValue(coordinator)],
        child: MaterialApp(
          localizationsDelegates: [const AppStringsDelegate()],
          supportedLocales: AppStrings.supportedLocales,
          home: CommandPlaybackScreen(
            itemId: entity(fixture[formats ? 'item' : 'boundary_item']),
          ),
        ),
      ),
    );
    await until(
      () =>
          commands.deliverable &&
          !coordinator.attaching &&
          coordinator.native.controller?.value.isInitialized == true,
    );
    final start = commands.record!.start, history = commands.admitted!.history;
    await until(
      () => coordinator.native.controller!.value.position.inMilliseconds >= 500,
    );
    if (!formats) {
      await coordinator.pause();
      await nativeRelayStress(coordinator.native.generation!.relay);
    } else {
      await binding.convertFlutterSurfaceToImage();
      for (final height in [1080, 720, 480]) {
        if (height != 1080) {
          final previous = commands.record!.output;
          await commands.checkpoints.flush(force: true);
          await coordinator.configure(resolution: height);
          await until(
            () =>
                !coordinator.attaching &&
                commands.deliverable &&
                coordinator.native.ready?.output == commands.record?.output &&
                commands.record?.output != previous &&
                coordinator.native.controller?.value.isInitialized == true,
          );
          await coordinator.play();
        }
        final generation = coordinator.native.generation!;
        await until(
          () =>
              identical(coordinator.native.generation, generation) &&
              generation.player.value.position.inMilliseconds >=
                  (height == 1080 ? 500 : 250),
        );
        await coordinator.pause();
        await tester.pump();
        final ready = commands.admitted!;
        final response = await commands.transport.call(
          'POST',
          '/api/v1/qualification/relay-output',
          commands.record!.scope,
          body: {'start': start, 'output': ready.output, 'height': height},
        );
        expect(response.body['height'], height);
        expect(response.body['channels'], 2);
        expect(response.body['generated_output'], isTrue);
        expect(generation.player.value.size.height.round(), height);
        final before =
            generation.player.value.position.inMilliseconds + ready.offset;
        final screenshot = await binding.takeScreenshot(
          'relay-authored-$height',
        );
        final after =
            generation.player.value.position.inMilliseconds + ready.offset;
        require(
          screenshot.length <= 16 * 1024 * 1024 &&
              identical(coordinator.native.generation, generation),
        );
        final codec = await ui.instantiateImageCodec(
          Uint8List.fromList(screenshot),
        );
        final image = (await codec.getNextFrame()).image;
        final pixels = await image.toByteData(
          format: ui.ImageByteFormat.rawRgba,
        );
        final decoded = verifyAuthoredFrame(
          pixels!,
          width: image.width,
          height: image.height,
          video: tester.getRect(find.byType(VideoPlayer)),
          scale: tester.view.devicePixelRatio,
          beforeMs: before,
          afterMs: after,
        );
        frames.add({'height': height, ...decoded});
        image.dispose();
        codec.dispose();
        expect(commands.record!.start, start);
        expect(commands.admitted!.history, history);
      }
    }
    await coordinator.close();
    expect(commands.record, isNull);
    expect(coordinator.native.generation, isNull);
    await channel.invokeMethod<void>('clearBootstrap');
    binding.reportData = {
      'scenario': fixture['scenario'],
      'same_viewer': true,
      'confirmed_cleanup': true,
      if (formats) ...{
        'generated_outputs': true,
        'three_authored_frames': frames.length == 3,
        'explicit_quality_changes': true,
        'frames': frames,
      } else ...{
        'eight_socket_gate': true,
        'native_backpressure': true,
      },
    };
  } finally {
    await coordinator.close();
    coordinator.dispose();
  }
}

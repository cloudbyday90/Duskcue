// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import 'dart:async';
import 'package:flutter/foundation.dart';
import 'package:video_player/video_player.dart';
import 'commands/models.dart';
import 'commands/retained.dart';
import 'commands/transport.dart';
import 'commands/wire.dart';
import 'relay/service.dart';

class NativeGeneration {
  NativeGeneration(this.output, this.relay, this.player) {
    initialized = player.initialize();
    initialized.then((_) {}, onError: (Object _) {});
  }
  final String output;
  final PlaybackRelay relay;
  final VideoPlayerController player;
  late final Future<void> initialized;
  Future<void>? _closing;
  bool disposed = false;
  Future<void> close() => _closing ??= () async {
    final network = relay.close();
    final native = () async {
      try {
        await initialized;
      } catch (_) {}
      await player.dispose();
    }();
    await Future.wait([network, native]);
    disposed = true;
  }();
}

class NativeOutput extends ChangeNotifier {
  NativeOutput(this.transport, this.current, this.onSample, this.onFailure);
  final PlaybackTransport transport;
  final bool Function(String) current;
  final void Function(String, int, String) onSample;
  final void Function() onFailure;
  final operations = RetainedOperations(1);
  final commands = RetainedOperations(2);
  NativeGeneration? generation;
  VodReady? ready;
  bool quarantined = false;
  int _epoch = 0;
  bool _failed = false;
  Future<void>? _retiring;
  VideoPlayerController? get controller => generation?.player;
  Future<void> attach(
    VodRecord record,
    VodReceipt receipt, {
    required bool play,
  }) => operations.run(
    () async {
      require(_retiring == null && !quarantined, 'native_cleanup_pending');
      await retire();
      final value = receipt.ready!;
      final epoch = ++_epoch;
      require(
        current(value.output) && transport.current(record.scope),
        'scope_changed',
      );
      final relay = await PlaybackRelay.bind(
        origin: record.scope.origin,
        prefix: commandPath(record.start, '/outputs/${value.output}/hls/'),
        token: transport.bound!.identity.token,
        context: record.scope.context,
        current: () =>
            epoch == _epoch &&
            current(value.output) &&
            transport.current(record.scope),
      );
      if (epoch != _epoch || !current(value.output)) {
        await relay.close();
        throw const VodFailure('scope_changed');
      }
      final player = VideoPlayerController.networkUrl(
        relay.uri,
        formatHint: VideoFormat.hls,
        videoPlayerOptions: VideoPlayerOptions(allowBackgroundPlayback: true),
      );
      final owner = NativeGeneration(value.output, relay, player);
      generation = owner;
      ready = value;
      _failed = false;
      player.addListener(_sample);
      try {
        await owner.initialized;
        require(epoch == _epoch && current(value.output), 'scope_changed');
        final relative = (value.position - value.offset).clamp(0, 2147483647);
        if (relative > 0) await player.seekTo(Duration(milliseconds: relative));
        require(epoch == _epoch && current(value.output), 'scope_changed');
        if (play) await player.play();
        notifyListeners();
      } catch (_) {
        await owner.close();
        if (identical(generation, owner)) {
          generation = null;
          ready = null;
          quarantined = false;
        }
        rethrow;
      }
    },
    timeout: const Duration(seconds: 30),
    cancel: () {
      quarantined = true;
      unawaited(retire().catchError((Object _) {}));
    },
  );
  void _sample() {
    final owner = generation, source = ready;
    if (owner == null ||
        source == null ||
        owner.disposed ||
        !current(source.output)) {
      return;
    }
    final value = owner.player.value;
    if (value.hasError && !_failed) {
      _failed = true;
      onFailure();
    }
    if (!value.isInitialized || value.hasError) return;
    final absolute = value.position.inMilliseconds + source.offset;
    if (absolute < 0 || absolute > 2147483647) return;
    onSample(
      source.output,
      absolute,
      value.isBuffering
          ? 'buffering'
          : value.isPlaying
          ? 'playing'
          : 'paused',
    );
  }

  Future<void> setPlaying(bool value) => commands.run(
    () async {
      final owner = generation;
      require(
        owner != null && current(owner.output) && !quarantined,
        'native_unavailable',
      );
      if (value) {
        await owner!.player.play();
      } else {
        await owner!.player.pause();
      }
    },
    cancel: () {
      quarantined = true;
      unawaited(retire().catchError((Object _) {}));
    },
  );
  Future<void> retire() {
    if (_retiring != null) return _retiring!;
    _epoch++;
    final owner = generation;
    if (owner == null) return Future.value();
    owner.player.removeListener(_sample);
    final work = () async {
      try {
        await owner.close();
        if (identical(generation, owner)) {
          generation = null;
          ready = null;
        }
        quarantined = false;
      } catch (_) {
        quarantined = true;
        rethrow;
      }
    }();
    _retiring = work;
    work.then((_) {
      if (identical(_retiring, work)) _retiring = null;
    }, onError: (Object _) {});
    return work;
  }
}

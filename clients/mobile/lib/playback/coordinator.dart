// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import 'dart:async';
import 'package:flutter/foundation.dart';
import 'package:video_player/video_player.dart';
import 'commands/controller.dart';
import 'commands/selection.dart';
import 'commands/wire.dart';
import 'native_output.dart';

class PlaybackCoordinator extends ChangeNotifier {
  PlaybackCoordinator(
    this.commands, {
    VideoPlayerController Function(Uri)? createPlayer,
  }) {
    native = NativeOutput(
      commands.transport,
      (output) =>
          commands.deliverable &&
          !background &&
          !requiresPlay &&
          commands.receipt?.ready?.output == output,
      commands.observe,
      _workerFailure,
      createPlayer: createPlayer,
    );
    commands.addListener(_changed);
    native.addListener(notifyListeners);
  }
  final PlaybackCommands commands;
  late final NativeOutput native;
  bool background = false, requiresPlay = false, playingIntent = true;
  bool _attaching = false, _recovering = false, _disposed = false;
  bool _opening = false, _resuming = false;
  String? _resumeStart;
  Future<void>? _closing;
  int height = 1080;
  bool get attaching => _attaching;
  bool get configurable =>
      commands.canAct &&
      commands.admitted != null &&
      !commands.cleanupOnly &&
      const {'ready', 'unavailable'}.contains(commands.state) &&
      commands.record != null &&
      commands.transport.current(commands.record!.scope);
  void _notify() {
    if (!_disposed) notifyListeners();
  }

  Future<bool> open(String item) async {
    require(!_opening, 'command_busy');
    _opening = true;
    try {
      final selected = await commands.open(
        androidSelection(item),
        resume: true,
      );
      if (selected && !commands.cleanupOnly && commands.resumePosition > 0) {
        _resumeStart = commands.record?.start;
        if (commands.deliverable && commands.canAct) await _resume();
      }
      return selected;
    } finally {
      _opening = false;
      _changed();
    }
  }

  Future<void> _resume() async {
    _resuming = true;
    _resumeStart = null;
    try {
      await seek(commands.resumePosition);
    } finally {
      _resuming = false;
      _changed();
    }
  }

  void _changed() {
    if (!commands.deliverable || background || requiresPlay) {
      unawaited(
        native.retire().catchError(
          (Object _) =>
              commands.fail(const VodFailure('native_cleanup_pending')),
        ),
      );
    } else if (!_opening && !_resuming && !_attaching && commands.canAct) {
      if (_resumeStart == commands.record?.start) {
        unawaited(_resume());
      } else if (native.ready?.output != commands.receipt!.ready!.output) {
        unawaited(_attach());
      }
    }
    _notify();
  }

  Future<void> _attach() async {
    if (_attaching) return;
    final record = commands.record!, receipt = commands.receipt!;
    _attaching = true;
    _notify();
    try {
      await native.attach(record, receipt, play: () => playingIntent);
    } catch (failure) {
      if (!_disposed &&
          commands.record?.start == record.start &&
          commands.receipt?.ready?.output == receipt.ready?.output &&
          commands.transport.current(record.scope)) {
        commands.fail(failure);
      }
    } finally {
      _attaching = false;
      if (!_disposed) _changed();
    }
  }

  Future<void> play() async {
    require(!background && !_attaching, 'command_busy');
    await commands.check(once: true);
    require(commands.deliverable, 'viewer_unavailable');
    requiresPlay = false;
    playingIntent = true;
    if (native.ready?.output != commands.receipt!.ready!.output) {
      await _attach();
    } else {
      await native.setPlaying(true);
    }
    _notify();
  }

  Future<void> pause() async {
    playingIntent = false;
    await native.setPlaying(false);
    _notify();
  }

  void lifecycle(bool active) {
    if (!active) {
      background = true;
      requiresPlay = true;
      playingIntent = false;
      unawaited(
        native.retire().catchError(
          (Object _) =>
              commands.fail(const VodFailure('native_cleanup_pending')),
        ),
      );
    } else {
      background = false;
      if (commands.record != null && commands.canAct && !commands.cleanupOnly) {
        unawaited(commands.check(once: true).catchError(commands.fail));
      }
    }
    _notify();
  }

  Future<void> seek(int position) =>
      commands.change({'kind': 'seek', 'position_ms': integer(position)});
  Future<void> configure({int? audio, int? subtitle, int? resolution}) async {
    final ready = commands.admitted;
    final sample = commands.checkpoints.confirmed;
    require(configurable && ready != null, 'viewer_unavailable');
    require(
      sample == null || sample['output_revision'] == ready!.output,
      'checkpoint_required',
    );
    final chosen = resolution ?? height;
    final selection = androidSelection(
      ready!.item,
      file: ready.file,
      audio: audio ?? ready.audio,
      subtitle: subtitle,
      height: chosen,
    );
    await commands.change({
      'kind': 'reconfigure',
      'position_ms': sample == null
          ? ready.position
          : integer(sample['position_ms']),
      'selection': selection,
    });
    if (commands.error == null) height = chosen;
  }

  void _workerFailure() {
    if (_recovering || !commands.canAct) return;
    _recovering = true;
    () async {
      try {
        if (!await commands.recoverWorker()) {
          commands.fail(const VodFailure('media_unavailable'));
        }
      } catch (failure) {
        commands.fail(failure);
      } finally {
        _recovering = false;
        _notify();
      }
    }();
  }

  Future<void> close() => _closing ??=
      () async {
        final local = native.retire();
        local.then((_) {}, onError: (Object _) {});
        await commands.close();
        try {
          await local.timeout(const Duration(seconds: 5));
        } catch (_) {
          commands.fail(const VodFailure('native_cleanup_pending'));
        }
      }().whenComplete(() {
        _closing = null;
      });
  void invalidate() {
    requiresPlay = true;
    playingIntent = false;
    commands.invalidate();
    _changed();
  }

  @override
  void dispose() {
    _disposed = true;
    commands.removeListener(_changed);
    native.removeListener(notifyListeners);
    unawaited(native.retire().catchError((Object _) {}));
    commands.dispose();
    super.dispose();
  }
}

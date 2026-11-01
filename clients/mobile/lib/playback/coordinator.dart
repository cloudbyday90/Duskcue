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
      ownership: commands.lifetime.native,
    );
    commands.retireLocal = native.retire;
    commands.addListener(_changed);
    native.addListener(notifyListeners);
    commands.lifetime.addListener(_notify);
  }
  final PlaybackCommands commands;
  late final NativeOutput native;
  bool background = false, requiresPlay = false, playingIntent = true;
  bool _attaching = false, _disposed = false;
  int _recoveryRevision = 0;
  bool _opening = false, _resuming = false;
  String? _resumeStart;
  Future<void>? _closing;
  int height = 1080;
  bool get attaching => _attaching;
  bool get canOpen => !_opening && commands.canAct && commands.record == null;
  int get epoch => commands.epoch;
  bool owns(int token, String? start) =>
      !_disposed &&
      commands.ownsEpoch(token) &&
      (start == null || commands.record?.start == start);
  bool canClose(int token, String? start) =>
      owns(token, start) ||
      !_disposed &&
          start != null &&
          commands.cleanupOnly &&
          commands.record?.start == start &&
          commands.transport.current(commands.record!.scope, forCleanup: true);
  void failIfCurrent(Object failure, int token, String? start) {
    if (owns(token, start)) commands.fail(failure);
  }

  Future<void> _retire() {
    final token = epoch, start = commands.record?.start;
    return native.retire().catchError(
      (Object _) => failIfCurrent(
        const VodFailure('native_cleanup_pending'),
        token,
        start,
      ),
    );
  }

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
    require(canOpen, 'command_busy');
    _recoveryRevision++;
    commands.cancelWorkerObservation();
    _opening = true;
    final opening = commands.open(androidSelection(item), resume: true);
    final token = epoch;
    try {
      final selected = await opening;
      if (owns(token, null) &&
          selected &&
          !commands.cleanupOnly &&
          commands.resumePosition > 0) {
        _resumeStart = commands.record?.start;
        if (commands.deliverable && commands.canAct) await _resume();
      }
      require(owns(token, null), 'scope_changed');
      return selected;
    } finally {
      _opening = false;
      if (owns(token, null)) _changed();
    }
  }

  Future<void> _resume() async {
    final token = epoch, start = commands.record?.start;
    _resuming = true;
    _resumeStart = null;
    try {
      await seek(commands.resumePosition);
    } finally {
      _resuming = false;
      if (owns(token, start)) _changed();
    }
  }

  void _changed() {
    if (!commands.deliverable || background || requiresPlay) {
      unawaited(_retire());
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
    final token = epoch;
    _attaching = true;
    _notify();
    try {
      await native.attach(record, receipt, play: () => playingIntent);
    } catch (failure) {
      if (owns(token, record.start) &&
          commands.deliverable &&
          !background &&
          !requiresPlay &&
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
    final token = epoch, start = commands.record?.start;
    await commands.check(once: true);
    require(owns(token, start) && commands.deliverable, 'viewer_unavailable');
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
    if (!commands.workerObserving) await native.setPlaying(false);
    _notify();
  }

  void lifecycle(bool active) {
    if (!active) {
      _recoveryRevision++;
      commands.cancelWorkerObservation();
      background = true;
      requiresPlay = true;
      playingIntent = false;
      unawaited(_retire());
    } else {
      background = false;
      if (commands.record != null && commands.canAct && !commands.cleanupOnly) {
        final token = epoch, start = commands.record?.start;
        unawaited(
          commands
              .check(once: true)
              .catchError(
                (Object failure) => failIfCurrent(failure, token, start),
              ),
        );
      }
    }
    _notify();
  }

  Future<void> seek(int position) =>
      commands.change({'kind': 'seek', 'position_ms': integer(position)});
  Future<void> configure({int? audio, int? subtitle, int? resolution}) async {
    final token = epoch, start = commands.record?.start;
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
    if (owns(token, start) && commands.error == null) height = chosen;
  }

  void _workerFailure(String output) {
    if (!commands.canAct ||
        commands.workerObserver.pending ||
        _closing != null ||
        background ||
        requiresPlay ||
        commands.record?.output != output) {
      return;
    }
    final revision = ++_recoveryRevision, captured = commands.record!;
    bool current() =>
        !_disposed &&
        revision == _recoveryRevision &&
        commands.record?.start == captured.start &&
        commands.record?.output == output &&
        commands.transport.current(captured.scope);
    () async {
      try {
        await commands.observeWorkerFailure(
          output,
          active: () =>
              current() && !background && !requiresPlay && _closing == null,
          detach: native.retire,
          unavailable: () {
            if (current()) requiresPlay = true;
          },
        );
      } catch (failure) {
        if (current()) {
          requiresPlay = true;
          commands.fail(failure);
        }
      } finally {
        _notify();
      }
    }();
  }

  Future<void> close() => _closing ??=
      () async {
        _recoveryRevision++;
        commands.cancelWorkerObservation();
        final local = native.retire();
        local.then((_) {}, onError: (Object _) {});
        final stopping = commands.close();
        final token = epoch;
        await stopping;
        try {
          await Future.wait([
            local,
            commands.lifetime.native.drain(),
          ]).timeout(const Duration(seconds: 5));
        } catch (_) {
          failIfCurrent(
            const VodFailure('native_cleanup_pending'),
            token,
            null,
          );
        }
      }().whenComplete(() {
        _closing = null;
      });
  void invalidate() {
    _recoveryRevision++;
    requiresPlay = true;
    playingIntent = false;
    commands.invalidate();
    _changed();
  }

  @override
  void dispose() {
    _disposed = true;
    _recoveryRevision++;
    commands.removeListener(_changed);
    native.removeListener(notifyListeners);
    commands.lifetime.removeListener(_notify);
    unawaited(native.retire().catchError((Object _) {}));
    commands.dispose();
    super.dispose();
  }
}

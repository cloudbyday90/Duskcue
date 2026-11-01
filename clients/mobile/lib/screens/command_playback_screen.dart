// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import 'dart:async';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:video_player/video_player.dart';
import '../l10n/app_strings.dart';
import '../playback/coordinator.dart';
import '../playback/provider.dart';
import '../playback/tracks.dart';
import '../playback/commands/wire.dart';
import 'legacy_playback_entry_screen.dart';

class CommandPlaybackScreen extends ConsumerStatefulWidget {
  const CommandPlaybackScreen({required this.itemId, super.key});
  final String itemId;
  @override
  ConsumerState<CommandPlaybackScreen> createState() =>
      _CommandPlaybackScreenState();
}

class _CommandPlaybackScreenState extends ConsumerState<CommandPlaybackScreen>
    with WidgetsBindingObserver {
  late final PlaybackCoordinator service;
  bool legacy = false, _allowPop = false, _leaving = false;
  bool _rejected = false;
  int? _ownerEpoch;
  String? _start;
  int _routeRevision = 0;
  Route<dynamic>? _route;
  DialogRoute<int>? _dialog;
  NavigatorState? _dialogNavigator;
  bool get _owns =>
      !_rejected && _ownerEpoch != null && service.owns(_ownerEpoch!, _start);
  bool get _canClose =>
      !_rejected &&
      _ownerEpoch != null &&
      service.canClose(_ownerEpoch!, _start);
  @override
  void initState() {
    super.initState();
    service = ref.read(playbackCoordinatorProvider);
    service.addListener(_changed);
    WidgetsBinding.instance.addObserver(this);
    unawaited(_open());
  }

  void _changed() {
    if (!mounted) return;
    if (_start == null &&
        _ownerEpoch != null &&
        service.owns(_ownerEpoch!, null)) {
      _start = service.commands.record?.start;
    }
    if (!_owns) _dismissDialog();
    setState(() {});
  }

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    _route ??= ModalRoute.of(context);
  }

  @override
  void didUpdateWidget(CommandPlaybackScreen oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.itemId == widget.itemId) return;
    final revision = ++_routeRevision;
    _dismissDialog();
    unawaited(
      () async {
        if (_canClose) {
          final closing = service.close();
          _ownerEpoch = service.epoch;
          await closing;
        }
        if (!mounted || revision != _routeRevision) return;
        if (service.commands.record != null ||
            service.commands.lifetime.native.held ||
            service.commands.journal.blocked) {
          setState(() {});
          return;
        }
        legacy = false;
        _start = null;
        _allowPop = false;
        await _open();
      }().catchError((Object _) {}),
    );
  }

  void _dismissDialog() {
    final dialog = _dialog, navigator = _dialogNavigator;
    _dialog = null;
    _dialogNavigator = null;
    if (dialog == null || navigator == null) return;
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (navigator.mounted && dialog.isActive) navigator.removeRoute(dialog);
    });
  }

  Future<void> _open() async {
    final revision = _routeRevision;
    if (!service.canOpen) {
      setState(() => _rejected = true);
      return;
    }
    _rejected = false;
    final opening = service.open(widget.itemId);
    final token = service.epoch;
    _ownerEpoch = token;
    try {
      final selected = await opening;
      if (!mounted ||
          revision != _routeRevision ||
          !service.owns(token, null)) {
        return;
      }
      _start = service.commands.record?.start;
      if (!selected) setState(() => legacy = true);
    } catch (failure) {
      if (mounted && revision == _routeRevision) {
        service.failIfCurrent(failure, token, _start);
      }
    }
  }

  @override
  void didChangeAppLifecycleState(AppLifecycleState state) {
    if (!legacy && _owns) service.lifecycle(state == AppLifecycleState.resumed);
  }

  Future<void> _action(Future<void> Function() action) async {
    if (!mounted || !_owns) return;
    final token = service.epoch, start = service.commands.record?.start;
    try {
      await action();
    } catch (failure) {
      service.failIfCurrent(failure, token, start);
    }
  }

  Future<void> _checkStatus() async {
    if (!_owns && !_canClose) return;
    await service.commands.check().catchError((Object _) {});
  }

  Future<void> _close() async {
    if (!mounted || _leaving) return;
    setState(() => _leaving = true);
    final revision = _routeRevision;
    if (!_canClose) {
      _popOwnedRoute(revision);
      return;
    }
    final stopping = service.close();
    final token = service.epoch;
    _ownerEpoch = token;
    try {
      await stopping;
    } catch (failure) {
      service.failIfCurrent(failure, token, _start);
    }
    if (!mounted) return;
    if (!service.owns(token, null)) {
      _popOwnedRoute(revision);
      return;
    }
    if (service.commands.record == null &&
        service.native.generation == null &&
        !service.native.quarantined &&
        !service.commands.lifetime.native.held) {
      _popOwnedRoute(revision);
    } else {
      setState(() => _leaving = false);
    }
  }

  void _popOwnedRoute(int revision) {
    if (!mounted || revision != _routeRevision) return;
    final route = _route, navigator = Navigator.of(context);
    setState(() => _allowPop = true);
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (!mounted ||
          revision != _routeRevision ||
          route == null ||
          !route.isCurrent) {
        return;
      }
      if (navigator.canPop()) navigator.pop();
    });
  }

  Future<void> _tracks(bool audio) async {
    if (!mounted || !_owns) return;
    final strings = AppStrings.of(context);
    await _action(() async {
      final output = service.commands.record?.output;
      final token = service.epoch, start = service.commands.record?.start;
      final tracks = await playbackTracks(service.commands, audio: audio);
      if (!mounted ||
          !_owns ||
          !service.owns(token, start) ||
          output != service.commands.record?.output) {
        return;
      }
      final navigator = Navigator.of(context, rootNavigator: true);
      late final DialogRoute<int> dialog;
      void select(int? selected) {
        if (mounted &&
            _owns &&
            service.owns(token, start) &&
            output == service.commands.record?.output &&
            dialog.isCurrent) {
          navigator.pop(selected);
        }
      }

      dialog = DialogRoute<int>(
        context: context,
        builder: (context) => AnimatedBuilder(
          animation: service,
          builder: (context, _) =>
              !_owns ||
                  !service.owns(token, start) ||
                  output != service.commands.record?.output
              ? const SizedBox.shrink()
              : SimpleDialog(
                  title: Text(audio ? strings.audio : strings.subtitles),
                  children: [
                    if (!audio)
                      SimpleDialogOption(
                        onPressed: () => select(-1),
                        child: Text(strings.trackOff),
                      ),
                    for (final track in tracks)
                      SimpleDialogOption(
                        onPressed: () => select(track.index),
                        child: Text(track.label),
                      ),
                    TextButton(
                      onPressed: () => select(null),
                      child: Text(strings.cancel),
                    ),
                  ],
                ),
        ),
      );
      _dialog = dialog;
      _dialogNavigator = navigator;
      final selected = await navigator.push(dialog);
      if (identical(_dialog, dialog)) {
        _dialog = null;
        _dialogNavigator = null;
      }
      if (selected == null ||
          !mounted ||
          !_owns ||
          !service.owns(token, start)) {
        return;
      }
      require(
        output == service.commands.record?.output && service.configurable,
        'scope_changed',
      );
      final ready = service.commands.admitted!;
      await service.configure(
        audio: audio ? selected : ready.audio,
        subtitle: audio
            ? ready.subtitle
            : selected < 0
            ? null
            : selected,
      );
    });
  }

  @override
  Widget build(BuildContext context) {
    if (legacy && _owns) {
      return LegacyPlaybackEntryScreen(itemId: widget.itemId);
    }
    final strings = AppStrings.of(context),
        commands = service.commands,
        player = service.native.controller;
    if (_rejected || _ownerEpoch != null && !_owns && !_canClose) {
      return Scaffold(
        appBar: AppBar(
          title: Text(strings.playbackEntry),
          leading: IconButton(
            tooltip: strings.close,
            icon: const Icon(Icons.close),
            onPressed: _close,
          ),
        ),
        body: Semantics(
          liveRegion: true,
          child: Text(
            strings.commandError(_rejected ? 'command_busy' : 'scope_changed'),
          ),
        ),
      );
    }
    final available =
        commands.deliverable && !service.attaching && !service.requiresPlay;
    final enabled = _owns && commands.canAct && !_leaving;
    return PopScope(
      canPop: _allowPop,
      onPopInvokedWithResult: (popped, _) {
        if (!popped) unawaited(_close());
      },
      child: Scaffold(
        appBar: AppBar(
          title: Text(strings.playbackEntry),
          leading: IconButton(
            tooltip: strings.close,
            onPressed: _leaving ? null : _close,
            icon: const Icon(Icons.close),
          ),
        ),
        body: SafeArea(
          child: Column(
            children: [
              Padding(
                padding: const EdgeInsets.all(16),
                child: Semantics(
                  liveRegion: true,
                  label:
                      '${strings.commandState(commands.state)}${commands.error == null ? '' : '. ${strings.commandError(commands.error!.code)}'}',
                  child: ExcludeSemantics(
                    child: Text(
                      strings.commandState(commands.state),
                      key: const Key('playback-status'),
                    ),
                  ),
                ),
              ),
              Expanded(
                child: ListView(
                  padding: const EdgeInsets.all(16),
                  children: [
                    if (commands.busy ||
                        commands.monitoring ||
                        commands.workerObserving ||
                        service.attaching ||
                        _leaving)
                      const LinearProgressIndicator(),
                    if (commands.error != null)
                      Padding(
                        padding: const EdgeInsets.symmetric(vertical: 12),
                        child: Semantics(
                          liveRegion: true,
                          child: Text(
                            strings.commandError(commands.error!.code),
                          ),
                        ),
                      ),
                    if (commands.progressUncertain)
                      Text(strings.progressUnconfirmed),
                    if (player != null &&
                        player.value.isInitialized &&
                        available)
                      AspectRatio(
                        aspectRatio: player.value.aspectRatio > 0
                            ? player.value.aspectRatio
                            : 16 / 9,
                        child: VideoPlayer(player),
                      ),
                    if (player != null &&
                        player.value.isInitialized &&
                        available)
                      ValueListenableBuilder<VideoPlayerValue>(
                        valueListenable: player,
                        builder: (context, value, _) {
                          final offset = service.native.ready!.offset;
                          final end = (value.duration.inMilliseconds + offset)
                              .clamp(1, 2147483647);
                          final position =
                              (value.position.inMilliseconds + offset).clamp(
                                0,
                                end,
                              );
                          return Column(
                            children: [
                              Slider(
                                label: Duration(
                                  milliseconds: position,
                                ).toString().split('.').first,
                                semanticFormatterCallback: (value) => Duration(
                                  milliseconds: value.round(),
                                ).toString().split('.').first,
                                value: position.toDouble(),
                                min: 0,
                                max: end.toDouble(),
                                onChanged: enabled ? (_) {} : null,
                                onChangeEnd: enabled
                                    ? (value) => unawaited(
                                        _action(
                                          () => service.seek(value.round()),
                                        ),
                                      )
                                    : null,
                              ),
                              Text(
                                Duration(
                                  milliseconds: position,
                                ).toString().split('.').first,
                              ),
                            ],
                          );
                        },
                      ),
                    Wrap(
                      spacing: 12,
                      runSpacing: 12,
                      children: [
                        FilledButton.icon(
                          onPressed:
                              (enabled ||
                                      commands.workerObserving && !_leaving) &&
                                  commands.record != null &&
                                  !commands.cleanupOnly
                              ? () => unawaited(
                                  _action(
                                    commands.workerObserving ||
                                            available &&
                                                player?.value.isPlaying == true
                                        ? service.pause
                                        : service.play,
                                  ),
                                )
                              : null,
                          icon: Icon(
                            commands.workerObserving ||
                                    available && player?.value.isPlaying == true
                                ? Icons.pause
                                : Icons.play_arrow,
                          ),
                          label: Text(
                            commands.workerObserving ||
                                    available && player?.value.isPlaying == true
                                ? strings.pause
                                : strings.play,
                          ),
                        ),
                        OutlinedButton(
                          onPressed:
                              (enabled || _canClose && commands.canAct) &&
                                  commands.record != null
                              ? () => unawaited(_checkStatus())
                              : null,
                          child: Text(strings.checkPlaybackStatus),
                        ),
                        if (commands.record == null &&
                            (commands.journal.blocked ||
                                commands.lifetime.native.held))
                          OutlinedButton(
                            onPressed: enabled
                                ? () => unawaited(
                                    _action(commands.checkLocalCleanup),
                                  )
                                : null,
                            child: Text(strings.checkDeviceCleanup),
                          ),
                        OutlinedButton(
                          onPressed: !_leaving && commands.record != null
                              ? _close
                              : null,
                          child: Text(strings.stopPlayback),
                        ),
                        OutlinedButton(
                          onPressed: enabled && service.configurable
                              ? () => _tracks(true)
                              : null,
                          child: Text(strings.audio),
                        ),
                        OutlinedButton(
                          onPressed: enabled && service.configurable
                              ? () => _tracks(false)
                              : null,
                          child: Text(strings.subtitles),
                        ),
                        for (final height in [480, 720, 1080])
                          OutlinedButton(
                            onPressed: enabled && service.configurable
                                ? () => unawaited(
                                    _action(
                                      () => service.configure(
                                        resolution: height,
                                        subtitle: commands.admitted!.subtitle,
                                      ),
                                    ),
                                  )
                                : null,
                            child: Text('${height}p'),
                          ),
                      ],
                    ),
                  ],
                ),
              ),
            ],
          ),
        ),
      ),
    );
  }

  @override
  void dispose() {
    _routeRevision++;
    _dismissDialog();
    service.removeListener(_changed);
    WidgetsBinding.instance.removeObserver(this);
    if (!legacy && !_allowPop && _owns && !service.commands.cleanupOnly) {
      unawaited(service.close().catchError((Object _) {}));
    }
    super.dispose();
  }
}

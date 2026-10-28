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
  @override
  void initState() {
    super.initState();
    service = ref.read(playbackCoordinatorProvider);
    service.addListener(_changed);
    WidgetsBinding.instance.addObserver(this);
    unawaited(_open());
  }

  void _changed() {
    if (mounted) setState(() {});
  }

  Future<void> _open() async {
    try {
      final selected = await service.open(widget.itemId);
      if (!selected && mounted) setState(() => legacy = true);
    } catch (failure) {
      service.commands.fail(failure);
    }
  }

  @override
  void didChangeAppLifecycleState(AppLifecycleState state) {
    if (!legacy) service.lifecycle(state == AppLifecycleState.resumed);
  }

  Future<void> _action(Future<void> Function() action) async {
    try {
      await action();
    } catch (failure) {
      service.commands.fail(failure);
    }
  }

  Future<void> _close() async {
    if (_leaving) return;
    setState(() => _leaving = true);
    await _action(service.close);
    if (!mounted) return;
    if (service.commands.record == null &&
        service.native.generation == null &&
        !service.native.quarantined) {
      setState(() => _allowPop = true);
      WidgetsBinding.instance.addPostFrameCallback((_) {
        if (mounted) Navigator.of(context).pop();
      });
    } else {
      setState(() => _leaving = false);
    }
  }

  Future<void> _tracks(bool audio) async {
    final strings = AppStrings.of(context);
    await _action(() async {
      final output = service.commands.record?.output;
      final tracks = await playbackTracks(service.commands, audio: audio);
      if (!mounted) return;
      final selected = await showDialog<int>(
        context: context,
        builder: (context) => SimpleDialog(
          title: Text(audio ? strings.audio : strings.subtitles),
          children: [
            if (!audio)
              SimpleDialogOption(
                onPressed: () => Navigator.pop(context, -1),
                child: Text(strings.trackOff),
              ),
            for (final track in tracks)
              SimpleDialogOption(
                onPressed: () => Navigator.pop(context, track.index),
                child: Text(track.label),
              ),
            TextButton(
              onPressed: () => Navigator.pop(context),
              child: Text(strings.cancel),
            ),
          ],
        ),
      );
      if (selected == null) return;
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
    if (legacy) return LegacyPlaybackEntryScreen(itemId: widget.itemId);
    final strings = AppStrings.of(context),
        commands = service.commands,
        player = service.native.controller;
    final available =
        commands.deliverable && !service.attaching && !service.requiresPlay;
    final enabled = commands.canAct && !_leaving;
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
                              enabled &&
                                  commands.record != null &&
                                  !commands.cleanupOnly
                              ? () => unawaited(
                                  _action(
                                    available && player?.value.isPlaying == true
                                        ? service.pause
                                        : service.play,
                                  ),
                                )
                              : null,
                          icon: Icon(
                            available && player?.value.isPlaying == true
                                ? Icons.pause
                                : Icons.play_arrow,
                          ),
                          label: Text(
                            available && player?.value.isPlaying == true
                                ? strings.pause
                                : strings.play,
                          ),
                        ),
                        OutlinedButton(
                          onPressed: enabled && commands.record != null
                              ? () => unawaited(_action(() => commands.check()))
                              : null,
                          child: Text(strings.checkPlaybackStatus),
                        ),
                        OutlinedButton(
                          onPressed: enabled && commands.record != null
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
    service.removeListener(_changed);
    WidgetsBinding.instance.removeObserver(this);
    if (!legacy && !_allowPop && !service.commands.cleanupOnly) {
      unawaited(service.close().catchError((Object _) {}));
    }
    super.dispose();
  }
}

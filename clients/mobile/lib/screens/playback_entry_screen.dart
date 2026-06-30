import 'dart:async';

import 'package:duskcue_mobile/l10n/app_strings.dart';
import 'package:duskcue_mobile/models/content_models.dart';
import 'package:duskcue_mobile/models/playback_models.dart';
import 'package:duskcue_mobile/services/quality_service.dart';
import 'package:duskcue_mobile/services/service_providers.dart';
import 'package:duskcue_mobile/widgets/mobile_state_views.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:video_player/video_player.dart';

class PlaybackEntryScreen extends ConsumerStatefulWidget {
  const PlaybackEntryScreen({required this.itemId, super.key});

  final String itemId;

  @override
  ConsumerState<PlaybackEntryScreen> createState() => _PlaybackEntryScreenState();
}

class _PlaybackEntryScreenState extends ConsumerState<PlaybackEntryScreen> with WidgetsBindingObserver {
  static const _heartbeatInterval = Duration(seconds: 15);
  static const _qoeInterval = Duration(seconds: 30);
  static const _probeInterval = Duration(minutes: 5);

  VideoPlayerController? _controller;
  MediaItemSummary? _item;
  PlaybackStart? _playback;
  Timer? _heartbeatTimer;
  Timer? _qoeTimer;
  Timer? _probeTimer;
  List<AudioTrack> _audioTracks = const [];
  List<SubtitleTrack> _subtitleTracks = const [];
  List<SegmentSkip> _segments = const [];
  int? _selectedAudioStreamIndex;
  int? _selectedSubtitleStreamIndex;
  QualityMode _qualityMode = QualityMode.auto;
  int _telemetrySampleIndex = 0;
  int _rebufferCount = 0;
  int _rebufferTotalMs = 0;
  int _qualitySwitches = 0;
  DateTime? _startupStartedAt;
  DateTime? _bufferingStartedAt;
  int? _startupTimeMs;
  bool _loading = true;
  bool _buffering = false;
  bool _seeking = false;
  bool _completed = false;
  Object? _error;

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addObserver(this);
    _start();
  }

  @override
  void dispose() {
    WidgetsBinding.instance.removeObserver(this);
    _heartbeatTimer?.cancel();
    _qoeTimer?.cancel();
    _probeTimer?.cancel();
    unawaited(_stopPlayback());
    _controller?.removeListener(_handleVideoState);
    _controller?.dispose();
    super.dispose();
  }

  @override
  void didChangeAppLifecycleState(AppLifecycleState state) {
    if (state == AppLifecycleState.paused || state == AppLifecycleState.inactive || state == AppLifecycleState.detached) {
      unawaited(_sendHeartbeat());
      _controller?.pause();
    } else if (state == AppLifecycleState.resumed) {
      unawaited(_sendHeartbeat());
    }
  }

  Future<void> _start({int? resumeOverrideMs}) async {
    setState(() {
      _loading = true;
      _error = null;
      _completed = false;
    });

    try {
      final content = ref.read(contentServiceProvider);
      final playbackService = ref.read(playbackServiceProvider);
      final qualityService = ref.read(qualityServiceProvider);
      _startupStartedAt = DateTime.now();
      _startupTimeMs = null;
      _bufferingStartedAt = null;
      _telemetrySampleIndex = 0;
      _rebufferCount = 0;
      _rebufferTotalMs = 0;
      _qualitySwitches = 0;

      final item = await content.getMediaItem(widget.itemId);
      final watchData = await playbackService.getWatchData(widget.itemId);
      final selection = await qualityService.selectionForItem(widget.itemId);
      final audioTracks = await playbackService.listAudioTracks(widget.itemId);
      final subtitles = await playbackService.listSubtitles(widget.itemId);
      final segments = await playbackService.listSegments(widget.itemId);
      final deviceProfile = await qualityService.mobileDeviceProfile();
      final playback = await playbackService.startPlayback(
        mediaItemId: widget.itemId,
        audioStreamIndex: _selectedAudioStreamIndex,
        subtitleStreamIndex: _selectedSubtitleStreamIndex,
        qualityMode: selection.mode.apiValue,
        maxStreamingBitrate: selection.mode.maxStreamingBitrate,
        deviceProfile: deviceProfile,
      );

      final controller = await playbackService.createNetworkController(playbackService.streamUri(playback.streamUrl));
      controller.addListener(_handleVideoState);

      final resumeMs = resumeOverrideMs ?? watchData.resumePositionMs;
      if (resumeMs > 0) {
        await controller.seekTo(Duration(milliseconds: resumeMs));
      }
      await controller.play();

      final oldController = _controller;
      _heartbeatTimer?.cancel();
      _qoeTimer?.cancel();
      _probeTimer?.cancel();
      _controller = controller;
      _heartbeatTimer = Timer.periodic(_heartbeatInterval, (_) => unawaited(_sendHeartbeat()));
      _qoeTimer = Timer.periodic(_qoeInterval, (_) => unawaited(_sendQoeReport()));
      _probeTimer = Timer.periodic(_probeInterval, (_) => unawaited(_runBandwidthProbe()));
      await oldController?.dispose();
      _startupTimeMs = _startupStartedAt == null ? null : DateTime.now().difference(_startupStartedAt!).inMilliseconds;
      unawaited(_sendQoeReport());
      unawaited(_runBandwidthProbe());

      if (!mounted) return;
      setState(() {
        _item = item;
        _playback = playback;
        _qualityMode = selection.mode;
        _audioTracks = audioTracks;
        _subtitleTracks = subtitles;
        _segments = segments;
        _loading = false;
      });
    } catch (error) {
      if (!mounted) return;
      setState(() {
        _error = error;
        _loading = false;
      });
    }
  }

  Future<void> _restartWithSelections() async {
    final currentPosition = _positionMs;
    await _stopPlayback();
    await _controller?.dispose();
    _controller = null;
    await _start(resumeOverrideMs: currentPosition);
  }

  void _handleVideoState() {
    final controller = _controller;
    if (controller == null || !mounted) return;
    final value = controller.value;
    final isBuffering = value.isBuffering;
    if (_buffering != isBuffering) {
      if (isBuffering) {
        _bufferingStartedAt = DateTime.now();
      } else if (_bufferingStartedAt != null) {
        _rebufferCount += 1;
        _rebufferTotalMs += DateTime.now().difference(_bufferingStartedAt!).inMilliseconds;
        _bufferingStartedAt = null;
      }
      setState(() => _buffering = isBuffering);
    }
    if (value.hasError) {
      setState(() => _error = StateError(value.errorDescription ?? 'Playback failed'));
    }
    final duration = value.duration;
    final position = value.position;
    if (!_completed && duration.inMilliseconds > 0 && duration - position <= const Duration(seconds: 2)) {
      _completed = true;
      unawaited(_stopPlayback());
    }
  }

  Future<void> _togglePlayPause() async {
    final controller = _controller;
    if (controller == null) return;
    if (controller.value.isPlaying) {
      await controller.pause();
    } else {
      await controller.play();
    }
    await _sendHeartbeat();
    if (mounted) setState(() {});
  }

  Future<void> _seekTo(Duration position) async {
    final controller = _controller;
    final playback = _playback;
    if (controller == null || playback == null) return;
    setState(() => _seeking = true);
    await controller.seekTo(position);
    final result = await ref.read(playbackServiceProvider).seek(
          sessionId: playback.sessionId,
          positionMs: position.inMilliseconds,
        );
    if (result.streamUrl != null && result.streamUrl!.isNotEmpty) {
      _qualitySwitches += 1;
      await _replaceStream(result.streamUrl!, position);
    }
    await _sendHeartbeat();
    if (mounted) setState(() => _seeking = false);
  }

  Future<void> _replaceStream(String streamUrl, Duration position) async {
    final playbackService = ref.read(playbackServiceProvider);
    final next = await playbackService.createNetworkController(playbackService.streamUri(streamUrl));
    next.addListener(_handleVideoState);
    await next.seekTo(position);
    await next.play();
    final old = _controller;
    _controller = next;
    old?.removeListener(_handleVideoState);
    await old?.dispose();
  }

  Future<void> _sendHeartbeat() async {
    final playback = _playback;
    final controller = _controller;
    if (playback == null || controller == null) return;
    try {
      await ref.read(playbackServiceProvider).heartbeat(
            sessionId: playback.sessionId,
            positionMs: controller.value.position.inMilliseconds,
            isPaused: !controller.value.isPlaying,
            isBuffering: controller.value.isBuffering,
          );
      await _sendTelemetry();
    } catch (_) {}
  }

  Future<void> _stopPlayback() async {
    final playback = _playback;
    if (playback == null) return;
    final positionMs = _positionMs;
    _heartbeatTimer?.cancel();
    _qoeTimer?.cancel();
    _probeTimer?.cancel();
    _playback = null;
    try {
      await _sendQoeReport(sessionIdOverride: playback.sessionId);
      await ref.read(playbackServiceProvider).stop(sessionId: playback.sessionId, positionMs: positionMs);
    } catch (_) {}
  }

  Future<void> _sendTelemetry() async {
    final playback = _playback;
    if (playback == null) return;
    await ref.read(qualityServiceProvider).submitSegmentTelemetry(
          sessionId: playback.sessionId,
          sampleIndex: _telemetrySampleIndex++,
          rung: playback.streamDecision,
          rebufferCount: _rebufferCount,
          rebufferTotalMs: _rebufferTotalMs,
        );
  }

  Future<void> _sendQoeReport({String? sessionIdOverride}) async {
    final playback = _playback;
    final controller = _controller;
    final sessionId = sessionIdOverride ?? playback?.sessionId;
    if (sessionId == null) return;
    final elapsedSeconds = _startupStartedAt == null ? 0 : DateTime.now().difference(_startupStartedAt!).inSeconds;
    final playbackSeconds = elapsedSeconds <= 0 ? 1 : elapsedSeconds;
    final rebufferRatio = _rebufferTotalMs / (playbackSeconds * 1000);
    await ref.read(qualityServiceProvider).submitQoeReport(
          sessionId: sessionId,
          startupTimeMs: _startupTimeMs,
          rebufferRatio: rebufferRatio,
          averageBitrateBps: _qualityMode.maxStreamingBitrate,
          switchesPerMinute: _qualitySwitches / (playbackSeconds / 60),
          qualityDrops: 0,
          currentRung: playback?.streamDecision,
          currentBufferSeconds: controller?.value.isBuffering == true ? 0.0 : null,
        );
  }

  Future<void> _runBandwidthProbe() async {
    final playback = _playback;
    if (playback == null) return;
    await ref.read(qualityServiceProvider).runBandwidthProbe(sessionId: playback.sessionId);
  }

  Future<void> _setQualityMode(QualityMode mode) async {
    if (_qualityMode == mode) return;
    setState(() => _qualityMode = mode);
    await ref.read(qualityServiceProvider).saveSelectionForItem(widget.itemId, mode);
    await _restartWithSelections();
  }

  int get _positionMs => _controller?.value.position.inMilliseconds ?? 0;

  SegmentSkip? get _activeSkip {
    final position = _positionMs;
    for (final segment in _segments) {
      if (segment.isActiveAt(position)) return segment;
    }
    return null;
  }

  @override
  Widget build(BuildContext context) {
    final strings = AppStrings.of(context);
    final controller = _controller;
    final item = _item;

    return Scaffold(
      appBar: AppBar(
        title: Text(item?.title ?? strings.playbackEntry),
        actions: [
          IconButton(
            tooltip: strings.stop,
            onPressed: () async {
              await _stopPlayback();
              if (context.mounted) Navigator.of(context).maybePop();
            },
            icon: const Icon(Icons.stop),
          ),
        ],
      ),
      body: SafeArea(
        child: _loading
            ? EmptyState(icon: Icons.play_circle_outline, message: strings.loadingPlayback)
            : _error != null
                ? ErrorState(message: strings.playbackFailed, onRetry: () => _start(resumeOverrideMs: _positionMs))
                : controller == null
                    ? EmptyState(icon: Icons.play_circle_outline, message: strings.playbackTaskNotice)
                    : Column(
                        children: [
                          AspectRatio(
                            aspectRatio: controller.value.aspectRatio == 0 ? 16 / 9 : controller.value.aspectRatio,
                            child: Stack(
                              alignment: Alignment.center,
                              children: [
                                VideoPlayer(controller),
                                if (_buffering) const CircularProgressIndicator(),
                                if (_activeSkip != null)
                                  Positioned(
                                    right: 16,
                                    bottom: 16,
                                    child: FilledButton.tonalIcon(
                                      onPressed: () => _seekTo(Duration(milliseconds: _activeSkip!.skipToMs)),
                                      icon: const Icon(Icons.skip_next),
                                      label: Text(strings.skipSegment(_activeSkip!.segmentType)),
                                    ),
                                  ),
                              ],
                            ),
                          ),
                          Expanded(
                            child: ListView(
                              padding: const EdgeInsets.all(16),
                              children: [
                                _PlaybackControls(
                                  controller: controller,
                                  seeking: _seeking,
                                  onTogglePlayPause: _togglePlayPause,
                                  onSeek: _seekTo,
                                ),
                                if (_buffering) Text(strings.buffering),
                                const SizedBox(height: 16),
                                DropdownButtonFormField<QualityMode>(
                                  value: _qualityMode,
                                  decoration: InputDecoration(labelText: strings.quality, border: const OutlineInputBorder()),
                                  items: QualityMode.values
                                      .map((mode) => DropdownMenuItem<QualityMode>(value: mode, child: Text(mode.label)))
                                      .toList(growable: false),
                                  onChanged: (value) {
                                    if (value != null) unawaited(_setQualityMode(value));
                                  },
                                ),
                                const SizedBox(height: 12),
                                DropdownButtonFormField<int?>(
                                  value: _selectedAudioStreamIndex,
                                  decoration: InputDecoration(labelText: strings.audio, border: const OutlineInputBorder()),
                                  items: [
                                    const DropdownMenuItem<int?>(value: null, child: Text('Default')),
                                    ..._audioTracks.map((track) => DropdownMenuItem<int?>(value: track.index, child: Text(track.label))),
                                  ],
                                  onChanged: (value) {
                                    setState(() => _selectedAudioStreamIndex = value);
                                    unawaited(_restartWithSelections());
                                  },
                                ),
                                const SizedBox(height: 12),
                                DropdownButtonFormField<int?>(
                                  value: _selectedSubtitleStreamIndex,
                                  decoration: InputDecoration(labelText: strings.subtitles, border: const OutlineInputBorder()),
                                  items: [
                                    DropdownMenuItem<int?>(value: null, child: Text(strings.noSubtitle)),
                                    ..._subtitleTracks
                                        .where((track) => track.streamIndex != null)
                                        .map((track) => DropdownMenuItem<int?>(value: track.streamIndex, child: Text(track.label))),
                                  ],
                                  onChanged: (value) {
                                    setState(() => _selectedSubtitleStreamIndex = value);
                                    unawaited(_restartWithSelections());
                                  },
                                ),
                              ],
                            ),
                          ),
                        ],
                      ),
      ),
    );
  }
}

class _PlaybackControls extends StatelessWidget {
  const _PlaybackControls({
    required this.controller,
    required this.seeking,
    required this.onTogglePlayPause,
    required this.onSeek,
  });

  final VideoPlayerController controller;
  final bool seeking;
  final Future<void> Function() onTogglePlayPause;
  final Future<void> Function(Duration position) onSeek;

  @override
  Widget build(BuildContext context) {
    final value = controller.value;
    final durationMs = value.duration.inMilliseconds.toDouble();
    final maxMs = durationMs <= 0 ? 1.0 : durationMs;
    final positionMs = value.position.inMilliseconds.toDouble().clamp(0.0, maxMs) as double;

    return Column(
      children: [
        Row(
          children: [
            IconButton.filled(
              onPressed: seeking ? null : onTogglePlayPause,
              icon: Icon(value.isPlaying ? Icons.pause : Icons.play_arrow),
            ),
            const SizedBox(width: 12),
            Text(_formatDuration(value.position)),
            const Spacer(),
            Text(_formatDuration(value.duration)),
          ],
        ),
        Slider(
          value: positionMs,
          max: maxMs,
          onChanged: seeking ? null : (_) {},
          onChangeEnd: seeking ? null : (value) => onSeek(Duration(milliseconds: value.round())),
        ),
      ],
    );
  }

  String _formatDuration(Duration duration) {
    final totalSeconds = duration.inSeconds;
    final hours = totalSeconds ~/ 3600;
    final minutes = (totalSeconds % 3600) ~/ 60;
    final seconds = totalSeconds % 60;
    if (hours > 0) {
      return '$hours:${minutes.toString().padLeft(2, '0')}:${seconds.toString().padLeft(2, '0')}';
    }
    return '$minutes:${seconds.toString().padLeft(2, '0')}';
  }
}

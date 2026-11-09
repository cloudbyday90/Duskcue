// Copyright (C) 2026-2026 Duskcue Contributors
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

import 'dart:async';
import 'dart:io';

import 'package:duskcue_mobile/models/profile_models.dart';
import 'package:duskcue_mobile/services/service_providers.dart';
import 'package:duskcue_mobile/services/ambient_playback_service.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';

class AmbientPlayerScreen extends ConsumerStatefulWidget {
  const AmbientPlayerScreen({super.key});

  @override
  ConsumerState<AmbientPlayerScreen> createState() =>
      _AmbientPlayerScreenState();
}

class _AmbientPlayerScreenState extends ConsumerState<AmbientPlayerScreen> {
  Timer? _statusTimer;
  NativeAmbientPlaybackStatus _status = const NativeAmbientPlaybackStatus(
    isActive: false,
  );
  bool _stopping = false;
  bool _reading = false, _failedRead = false;
  String? _problem;
  double? _seekDraft;
  int? _seekEpoch;
  late final AmbientPlaybackService _service;

  @override
  void initState() {
    super.initState();
    _service = ref.read(ambientPlaybackServiceProvider);
    _service.addListener(_scopeChanged);
    _refreshStatus();
  }

  @override
  void dispose() {
    _statusTimer?.cancel();
    _service.removeListener(_scopeChanged);
    super.dispose();
  }

  Future<void> _refreshStatus() async {
    if (_reading) return;
    _reading = true;
    final epoch = _service.epoch;
    try {
      final status = await _service.status();
      if (!mounted || epoch != _service.epoch) return;
      setState(() { _status = status; _failedRead = false; _problem = null; });
      _statusTimer?.cancel();
      _statusTimer = Timer(const Duration(seconds: 2), _refreshStatus);
    } catch (_) {
      if (mounted && epoch == _service.epoch) setState(() {
        _failedRead = true;
        _problem = 'Playback status could not be confirmed. Check status or stop the channel.';
        _status = const NativeAmbientPlaybackStatus(isActive: false);
      });
    } finally { _reading = false; }
  }

  void _scopeChanged() {
    _statusTimer?.cancel();
    if (mounted) setState(() {
      _seekDraft = null;
      _seekEpoch = null;
      _status = const NativeAmbientPlaybackStatus(isActive: false);
      _failedRead = true;
      _problem = 'The viewing session changed. Check status to review any remaining cleanup.';
    });
  }

  Future<void> _control(Future<void> Function() action) async {
    final epoch = _service.epoch;
    try {
      await action();
      if (!mounted || epoch != _service.epoch) return;
      setState(() => _problem = null);
      await _refreshStatus();
    } catch (_) {
      if (mounted && epoch == _service.epoch) setState(() {
        _problem = 'That action is not yet confirmed. Check status or stop the channel.';
        _failedRead = true;
      });
      _statusTimer?.cancel();
    }
  }

  Future<void> _stop() async {
    if (_stopping) return;
    final epoch = _service.epoch;
    setState(() => _stopping = true);
    try {
      await _service.stop();
      if (mounted && epoch == _service.epoch) context.go('/ambient');
    } catch (_) {
      if (mounted && epoch == _service.epoch) setState(() {
        _problem = 'Channel cleanup is not yet confirmed. Check status or try Stop again.';
        _failedRead = true;
      });
    } finally {
      if (mounted) setState(() => _stopping = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final platformView = _failedRead ? const ColoredBox(color: Colors.black) : Platform.isAndroid
        ? const AndroidView(viewType: 'duskcue/ambient_player_view')
        : Platform.isIOS
        ? const UiKitView(viewType: 'duskcue/ambient_player_view')
        : const ColoredBox(color: Colors.black);
    return Scaffold(
      appBar: AppBar(
        title: Text(_status.channelName ?? 'Ambient playback'),
        leading: IconButton(
          tooltip: 'Channels',
          onPressed: () => context.go('/ambient'),
          icon: const Icon(Icons.arrow_back),
        ),
      ),
      body: SafeArea(
        child: Column(
          children: [
            Expanded(
              child: ColoredBox(
                color: Colors.black,
                child: SizedBox.expand(child: platformView),
              ),
            ),
            Flexible(child: SingleChildScrollView(child: Padding(
              padding: const EdgeInsets.all(16),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: [
                  Semantics(
                    liveRegion: true,
                    child: Text(
                      _failedRead ? 'Playback status unavailable' : _label(),
                      style: theme.textTheme.titleMedium,
                    ),
                  ),
                  if (_status.error != null || _problem != null) ...[
                    const SizedBox(height: 8),
                    Text(
                      _problem ?? 'Playback needs attention. Check status or stop the channel.',
                      style: TextStyle(color: theme.colorScheme.error),
                    ),
                  ],
                  const SizedBox(height: 12),
                  if (_status.protocol != 'legacy') ...[
                    Wrap(
                      spacing: 12,
                      runSpacing: 8,
                      children: [
                        OutlinedButton.icon(
                          onPressed: _stopping || !_status.isActive || _status.busy || _status.cleanupPending ? null : () => _control(_service.play),
                          icon: const Icon(Icons.play_arrow), label: const Text('Play'),
                        ),
                        OutlinedButton.icon(
                          onPressed: _stopping ? null : () => _control(_service.pause),
                          icon: const Icon(Icons.pause), label: const Text('Pause'),
                        ),
                      ],
                    ),
                    if (!_failedRead && _status.durationMs > 0 && !_status.requiresPlay && !_status.cleanupPending)
                      Semantics(
                        label: 'Playback position',
                        child: Slider(
                          value: (_seekDraft ?? _status.positionMs.toDouble()).clamp(0, _status.durationMs).toDouble(),
                          max: _status.durationMs.toDouble(),
                          label: '${_status.positionMs ~/ 1000} seconds',
                          onChangeStart: _status.busy || _stopping ? null : (_) { _seekEpoch = _service.epoch; },
                          onChanged: _status.busy || _stopping ? null : (value) => setState(() => _seekDraft = value),
                          onChangeEnd: _status.busy || _stopping ? null : (value) {
                            setState(() => _seekDraft = null);
                            final epoch = _seekEpoch;
                            _seekEpoch = null;
                            if (epoch != _service.epoch) return;
                            _control(() => _service.seek(value.round()));
                          },
                        ),
                      ),
                    const SizedBox(height: 8),
                  ],
                  OutlinedButton.icon(
                    onPressed: _stopping || _reading ? null : () => _status.protocol == 'legacy' || _status.busy ? _refreshStatus() : _control(_service.check),
                    icon: const Icon(Icons.refresh), label: const Text('Check status'),
                  ),
                  const SizedBox(height: 8),
                  FilledButton.icon(
                    onPressed: _stopping ? null : _stop,
                    icon: const Icon(Icons.stop_circle_outlined),
                    label: Text(_stopping ? 'Stopping…' : 'Stop channel'),
                  ),
                ],
              ),
            ))),
          ],
        ),
      ),
    );
  }

  String _label() {
    if (_status.cleanupPending && _status.state == 'idle') return 'Channel cleanup is not yet confirmed';
    return switch (_status.state) {
    'ready' => _status.isBuffering ? 'Buffering channel' : _status.isPlaying ? 'Playing' : _status.requiresPlay ? 'Ready. Select Play to continue.' : 'Paused',
    'changing' => 'Changing playback',
    'checking' => 'Checking playback status',
    'starting' || 'preparing' => 'Preparing channel playback',
    'stopping' || 'withdrawing' => 'Stopping channel',
    'cleanup' || 'cleanup_uncertain' => 'Channel cleanup is not yet confirmed',
    'settled' || 'released' || 'withdrawn' || 'idle' => 'Channel stopped',
    _ => 'Playback needs attention',
  };
  }
}

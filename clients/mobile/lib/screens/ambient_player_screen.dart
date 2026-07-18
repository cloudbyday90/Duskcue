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

  @override
  void initState() {
    super.initState();
    _refreshStatus();
    _statusTimer = Timer.periodic(
      const Duration(seconds: 2),
      (_) => _refreshStatus(),
    );
  }

  @override
  void dispose() {
    _statusTimer?.cancel();
    super.dispose();
  }

  Future<void> _refreshStatus() async {
    try {
      final status = await ref.read(ambientPlaybackServiceProvider).status();
      if (!mounted) return;
      setState(() => _status = status);
    } catch (_) {}
  }

  Future<void> _stop() async {
    if (_stopping) return;
    setState(() => _stopping = true);
    try {
      await ref.read(ambientPlaybackServiceProvider).stop();
    } finally {
      if (mounted) context.go('/ambient');
    }
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final platformView = Platform.isAndroid
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
            Padding(
              padding: const EdgeInsets.all(16),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: [
                  Text(
                    _status.isPlaying
                        ? 'Playing in the native media session'
                        : 'Preparing channel playback',
                    style: theme.textTheme.titleMedium,
                  ),
                  if (_status.error != null) ...[
                    const SizedBox(height: 8),
                    Text(
                      _status.error!,
                      style: TextStyle(color: theme.colorScheme.error),
                    ),
                  ],
                  const SizedBox(height: 12),
                  FilledButton.icon(
                    onPressed: _stopping ? null : _stop,
                    icon: const Icon(Icons.stop_circle_outlined),
                    label: Text(_stopping ? 'Stopping…' : 'Stop channel'),
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

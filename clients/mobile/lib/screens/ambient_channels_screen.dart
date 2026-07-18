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

import 'package:duskcue_mobile/api/client_error.dart';
import 'package:duskcue_mobile/models/profile_models.dart';
import 'package:duskcue_mobile/services/service_providers.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';

class AmbientChannelsScreen extends ConsumerStatefulWidget {
  const AmbientChannelsScreen({super.key});

  @override
  ConsumerState<AmbientChannelsScreen> createState() =>
      _AmbientChannelsScreenState();
}

class _AmbientChannelsScreenState extends ConsumerState<AmbientChannelsScreen> {
  List<AmbientChannelSummary> _channels = const [];
  bool _loading = true;
  String? _error;
  String? _startingChannelId;

  @override
  void initState() {
    super.initState();
    _load();
  }

  Future<void> _load() async {
    setState(() {
      _loading = true;
      _error = null;
    });
    try {
      final response = await ref
          .read(profileServiceProvider)
          .listAmbientChannels();
      if (!mounted) return;
      setState(() {
        _channels = response.items;
        _loading = false;
      });
    } catch (error) {
      if (!mounted) return;
      setState(() {
        _loading = false;
        _error = _errorMessage(error);
      });
    }
  }

  Future<void> _start(AmbientChannelSummary channel) async {
    if (!channel.isPlayable || _startingChannelId != null) return;
    setState(() {
      _startingChannelId = channel.id;
      _error = null;
    });
    try {
      await ref.read(ambientPlaybackServiceProvider).start(channel);
      if (mounted) context.go('/ambient/player');
    } catch (error) {
      if (!mounted) return;
      setState(() => _error = _errorMessage(error));
    } finally {
      if (mounted) setState(() => _startingChannelId = null);
    }
  }

  String _errorMessage(Object error) {
    if (error is ClientError && error.problem.detail.isNotEmpty) {
      return error.problem.detail;
    }
    return error.toString();
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Scaffold(
      appBar: AppBar(
        title: const Text('Ambient channels'),
        actions: [
          IconButton(
            tooltip: 'Refresh',
            onPressed: _loading ? null : _load,
            icon: const Icon(Icons.refresh),
          ),
        ],
      ),
      body: SafeArea(
        child: RefreshIndicator(
          onRefresh: _load,
          child: ListView(
            padding: const EdgeInsets.all(16),
            children: [
              Text(
                'Set-and-forget channels keep their playback separate from your history and recommendations.',
                style: theme.textTheme.bodyLarge,
              ),
              const SizedBox(height: 16),
              if (_loading)
                const Center(
                  child: Padding(
                    padding: EdgeInsets.all(24),
                    child: CircularProgressIndicator(),
                  ),
                )
              else if (_channels.isEmpty)
                const Padding(
                  padding: EdgeInsets.all(24),
                  child: Text(
                    'No ambient channels are available for this profile.',
                    textAlign: TextAlign.center,
                  ),
                )
              else
                for (final channel in _channels)
                  Card(
                    child: ListTile(
                      enabled: channel.isPlayable && _startingChannelId == null,
                      leading: CircleAvatar(
                        child: Icon(
                          channel.isKids
                              ? Icons.child_care_outlined
                              : Icons.tv_outlined,
                        ),
                      ),
                      title: Text(channel.name),
                      subtitle: Text(
                        '${channel.itemCount} item${channel.itemCount == 1 ? '' : 's'} · ${channel.isKids ? 'Kids' : 'Standard'}',
                      ),
                      trailing: _startingChannelId == channel.id
                          ? const SizedBox(
                              width: 24,
                              height: 24,
                              child: CircularProgressIndicator(strokeWidth: 2),
                            )
                          : const Icon(Icons.play_circle_outline),
                      onTap: () => _start(channel),
                    ),
                  ),
              if (_error != null) ...[
                const SizedBox(height: 12),
                Text(
                  _error!,
                  style: TextStyle(color: theme.colorScheme.error),
                  textAlign: TextAlign.center,
                ),
              ],
            ],
          ),
        ),
      ),
    );
  }
}

// Duskcue — Self-hosted media streaming server
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
import 'package:duskcue_mobile/models/server_profile.dart';
import 'package:duskcue_mobile/services/service_providers.dart';
import 'package:duskcue_mobile/stores/session_store.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';

class ServerSelectionScreen extends ConsumerStatefulWidget {
  const ServerSelectionScreen({super.key});

  @override
  ConsumerState<ServerSelectionScreen> createState() =>
      _ServerSelectionScreenState();
}

class _ServerSelectionScreenState extends ConsumerState<ServerSelectionScreen> {
  final TextEditingController _controller = TextEditingController(
    text: 'http://10.0.2.2:48027',
  );
  NetworkMode _networkMode = NetworkMode.local;
  List<ServerProfile> _savedServers = const [];
  bool _loadingSavedServers = true;
  bool _testing = false;
  String? _message;

  @override
  void initState() {
    super.initState();
    _loadSavedServers();
  }

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  Future<void> _loadSavedServers() async {
    final repository = ref.read(serverRepositoryProvider);
    try {
      final saved = await repository.readSavedServers();
      final last = await repository.readLastServer();
      if (!mounted) return;
      setState(() {
        _savedServers = saved;
        _loadingSavedServers = false;
        if (last != null) {
          _controller.text = last.origin.toString();
          _networkMode = last.networkMode;
        }
      });
    } catch (_) {
      if (!mounted) return;
      setState(() {
        _loadingSavedServers = false;
        _message = 'Saved servers could not be loaded.';
      });
    }
  }

  Future<void> _continue() async {
    setState(() {
      _testing = true;
      _message = null;
    });

    try {
      final profile = ServerProfile.fromInput(
        _controller.text,
        networkMode: _networkMode,
      );
      final apiClient = ref.read(apiClientProvider);
      apiClient.configure(profile.origin);
      await apiClient.ready();

      await ref.read(serverRepositoryProvider).saveConnectedServer(profile);
      final connectedProfile = profile.copyWith(
        lastConnectedAt: DateTime.now().toUtc(),
      );
      ref.read(sessionProvider.notifier).selectServer(connectedProfile);

      try {
        final session = await ref
            .read(authServiceProvider)
            .restore(connectedProfile);
        if (session != null) {
          ref.read(sessionProvider.notifier).setAuthenticated(session.user);
          if (mounted) context.go('/profiles');
          return;
        }
      } catch (_) {
        await ref.read(authServiceProvider).clearLocalSession();
        ref.read(sessionProvider.notifier).clearAuthentication();
      }

      if (mounted) context.go('/auth');
    } on FormatException catch (error) {
      _showMessage(error.message);
    } on ClientError catch (error) {
      _showMessage(_connectionMessage(error));
    } catch (_) {
      _showMessage('Could not reach this Duskcue server.');
    } finally {
      if (mounted) {
        setState(() => _testing = false);
      }
    }
  }

  void _showMessage(String value) {
    if (!mounted) return;
    setState(() => _message = value);
  }

  String _connectionMessage(ClientError error) {
    return switch (error.kind) {
      ClientErrorKind.network =>
        'The server could not be reached. Check the URL, network, and certificate trust.',
      ClientErrorKind.serverUnavailable =>
        'The server responded but is not ready yet.',
      _ =>
        'The server responded with ${error.problem.status}: ${error.problem.title}.',
    };
  }

  void _selectSaved(ServerProfile server) {
    setState(() {
      _controller.text = server.origin.toString();
      _networkMode = server.networkMode;
      _message = null;
    });
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(title: const Text('Duskcue')),
      body: SafeArea(
        child: Padding(
          padding: const EdgeInsets.all(24),
          child: ListView(
            children: [
              TextField(
                controller: _controller,
                keyboardType: TextInputType.url,
                decoration: const InputDecoration(
                  labelText: 'Server URL',
                  helperText: 'Use http(s)://<server>:48027. Do not use 48028.',
                  border: OutlineInputBorder(),
                ),
                onSubmitted: (_) => _continue(),
              ),
              const SizedBox(height: 16),
              DropdownButtonFormField<NetworkMode>(
                initialValue: _networkMode,
                decoration: const InputDecoration(
                  labelText: 'Network mode',
                  border: OutlineInputBorder(),
                ),
                items: NetworkMode.values
                    .map(
                      (mode) => DropdownMenuItem(
                        value: mode,
                        child: Text(mode.label),
                      ),
                    )
                    .toList(growable: false),
                onChanged: _testing
                    ? null
                    : (value) {
                        if (value == null) return;
                        setState(() {
                          _networkMode = value;
                          _message = null;
                        });
                      },
              ),
              const SizedBox(height: 8),
              Text(
                _networkMode.description,
                style: Theme.of(context).textTheme.bodySmall,
              ),
              if (_message != null) ...[
                const SizedBox(height: 16),
                Text(
                  _message!,
                  style: TextStyle(color: Theme.of(context).colorScheme.error),
                ),
              ],
              const SizedBox(height: 16),
              FilledButton(
                onPressed: _testing ? null : _continue,
                child: Text(_testing ? 'Testing...' : 'Test and continue'),
              ),
              const SizedBox(height: 24),
              Text(
                'Saved servers',
                style: Theme.of(context).textTheme.titleMedium,
              ),
              const SizedBox(height: 8),
              if (_loadingSavedServers)
                const LinearProgressIndicator()
              else if (_savedServers.isEmpty)
                const Text(
                  'Servers that pass the connection test will appear here.',
                )
              else
                ..._savedServers.map(
                  (server) => ListTile(
                    contentPadding: EdgeInsets.zero,
                    title: Text(server.displayName ?? server.origin.host),
                    subtitle: Text(
                      '${server.origin} · ${server.networkMode.label}',
                    ),
                    trailing: const Icon(Icons.chevron_right),
                    onTap: _testing ? null : () => _selectSaved(server),
                  ),
                ),
            ],
          ),
        ),
      ),
    );
  }
}

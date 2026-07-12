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
import 'package:duskcue_mobile/models/auth_models.dart';
import 'package:duskcue_mobile/models/server_profile.dart';
import 'package:duskcue_mobile/services/service_providers.dart';
import 'package:duskcue_mobile/stores/session_store.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';

class AuthScreen extends ConsumerStatefulWidget {
  const AuthScreen({super.key});

  @override
  ConsumerState<AuthScreen> createState() => _AuthScreenState();
}

class _AuthScreenState extends ConsumerState<AuthScreen> {
  final _usernameController = TextEditingController();
  final _passwordController = TextEditingController();
  final _inviteController = TextEditingController();
  final _reauthController = TextEditingController();

  DeviceCode? _deviceCode;
  bool _loading = false;
  String? _message;

  @override
  void dispose() {
    _usernameController.dispose();
    _passwordController.dispose();
    _inviteController.dispose();
    _reauthController.dispose();
    super.dispose();
  }

  Future<void> _loginWithPassword() async {
    await _runAuth(() async {
      final server = _requireServer();
      return ref.read(authServiceProvider).loginWithPassword(
            username: _usernameController.text.trim(),
            password: _passwordController.text,
            server: server,
          );
    });
  }

  Future<void> _loginWithInvite() async {
    await _runAuth(() async {
      final server = _requireServer();
      return ref.read(authServiceProvider).loginWithInvite(
            code: _inviteController.text.trim(),
            server: server,
          );
    });
  }

  Future<void> _loginWithReauth() async {
    await _runAuth(() async {
      final server = _requireServer();
      return ref.read(authServiceProvider).loginWithReauthCode(
            code: _reauthController.text.trim(),
            server: server,
          );
    });
  }

  Future<void> _loginWithPasskey() async {
    await _runAuth(() => ref.read(authServiceProvider).loginWithPasskey());
  }

  Future<void> _createDeviceCode() async {
    await _run(() async {
      final code = await ref.read(authServiceProvider).createDeviceCode();
      setState(() => _deviceCode = code);
    });
  }

  Future<void> _pollDeviceCode() async {
    final code = _deviceCode;
    if (code == null) return;
    await _runAuth(() => ref.read(authServiceProvider).pollDeviceToken(code.deviceCode));
  }

  Future<void> _runAuth(Future<AuthSession> Function() action) async {
    await _run(() async {
      final session = await action();
      ref.read(sessionProvider.notifier).setAuthenticated(session.user);
      if (mounted) context.go('/dashboard');
    });
  }

  Future<void> _run(Future<void> Function() action) async {
    setState(() {
      _loading = true;
      _message = null;
    });
    try {
      await action();
    } on ClientError catch (error) {
      _showMessage(error.toString());
    } on PlatformException catch (error) {
      _showMessage(error.message ?? 'This sign-in method is not available on this device.');
    } on UnsupportedError catch (error) {
      _showMessage(error.message ?? 'This sign-in method is not available on this device.');
    } catch (_) {
      _showMessage('Authentication failed.');
    } finally {
      if (mounted) setState(() => _loading = false);
    }
  }

  void _showMessage(String value) {
    if (!mounted) return;
    setState(() => _message = value);
  }

  ServerProfile _requireServer() {
    final server = ref.read(sessionProvider).server;
    if (server == null) {
      throw StateError('Select a server before signing in.');
    }
    return server;
  }

  @override
  Widget build(BuildContext context) {
    final server = ref.watch(sessionProvider).server;
    return Scaffold(
      appBar: AppBar(
        title: const Text('Sign in'),
        leading: IconButton(
          onPressed: () => context.go('/server'),
          icon: const Icon(Icons.arrow_back),
        ),
      ),
      body: SafeArea(
        child: ListView(
          padding: const EdgeInsets.all(24),
          children: [
            Text(server?.origin.toString() ?? 'No server selected', style: Theme.of(context).textTheme.titleMedium),
            const SizedBox(height: 24),
            FilledButton.icon(
              onPressed: _loading ? null : _loginWithPasskey,
              icon: const Icon(Icons.fingerprint),
              label: const Text('Continue with passkey'),
            ),
            const SizedBox(height: 24),
            TextField(
              controller: _usernameController,
              decoration: const InputDecoration(labelText: 'Username', border: OutlineInputBorder()),
            ),
            const SizedBox(height: 12),
            TextField(
              controller: _passwordController,
              obscureText: true,
              decoration: const InputDecoration(labelText: 'Password', border: OutlineInputBorder()),
              onSubmitted: (_) => _loginWithPassword(),
            ),
            const SizedBox(height: 12),
            FilledButton(
              onPressed: _loading ? null : _loginWithPassword,
              child: const Text('Sign in with password'),
            ),
            const Divider(height: 40),
            TextField(
              controller: _inviteController,
              decoration: const InputDecoration(labelText: 'Invite code', border: OutlineInputBorder()),
              onSubmitted: (_) => _loginWithInvite(),
            ),
            const SizedBox(height: 12),
            OutlinedButton(
              onPressed: _loading ? null : _loginWithInvite,
              child: const Text('Use invite code'),
            ),
            const SizedBox(height: 24),
            TextField(
              controller: _reauthController,
              decoration: const InputDecoration(labelText: 'Re-auth code', border: OutlineInputBorder()),
              onSubmitted: (_) => _loginWithReauth(),
            ),
            const SizedBox(height: 12),
            OutlinedButton(
              onPressed: _loading ? null : _loginWithReauth,
              child: const Text('Use re-auth code'),
            ),
            const Divider(height: 40),
            OutlinedButton.icon(
              onPressed: _loading ? null : _createDeviceCode,
              icon: const Icon(Icons.link),
              label: const Text('Link this device'),
            ),
            if (_deviceCode != null) ...[
              const SizedBox(height: 12),
              SelectableText('Code: ${_deviceCode!.userCode}\nApprove at ${_deviceCode!.verificationUri}'),
              const SizedBox(height: 12),
              OutlinedButton(
                onPressed: _loading ? null : _pollDeviceCode,
                child: const Text('Check authorization'),
              ),
            ],
            if (_message != null) ...[
              const SizedBox(height: 16),
              Text(_message!, style: TextStyle(color: Theme.of(context).colorScheme.error)),
            ],
            if (_loading) ...[
              const SizedBox(height: 16),
              const LinearProgressIndicator(),
            ],
          ],
        ),
      ),
    );
  }
}

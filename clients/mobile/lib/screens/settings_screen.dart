import 'package:duskcue_mobile/api/client_error.dart';
import 'package:duskcue_mobile/models/auth_models.dart';
import 'package:duskcue_mobile/services/service_providers.dart';
import 'package:duskcue_mobile/stores/session_store.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';

class SettingsScreen extends ConsumerStatefulWidget {
  const SettingsScreen({super.key});

  @override
  ConsumerState<SettingsScreen> createState() => _SettingsScreenState();
}

class _SettingsScreenState extends ConsumerState<SettingsScreen> {
  List<SessionDetail> _sessions = const [];
  bool _loading = true;
  String? _message;

  @override
  void initState() {
    super.initState();
    _loadSessions();
  }

  Future<void> _loadSessions() async {
    try {
      final sessions = await ref.read(authServiceProvider).listSessions();
      if (!mounted) return;
      setState(() {
        _sessions = sessions;
        _loading = false;
        _message = null;
      });
    } on ClientError catch (error) {
      _handleError(error);
    } catch (_) {
      if (!mounted) return;
      setState(() {
        _loading = false;
        _message = 'Unable to load sessions.';
      });
    }
  }

  Future<void> _deleteSession(String sessionId) async {
    try {
      await ref.read(authServiceProvider).deleteSession(sessionId);
      await _loadSessions();
    } on ClientError catch (error) {
      _handleError(error);
    }
  }

  Future<void> _logout() async {
    await ref.read(authServiceProvider).logout();
    ref.read(sessionProvider.notifier).clearAuthentication();
    if (mounted) context.go('/auth');
  }

  Future<void> _logoutAll() async {
    await ref.read(authServiceProvider).logoutAll();
    ref.read(sessionProvider.notifier).clearAuthentication();
    if (mounted) context.go('/auth');
  }

  void _handleError(ClientError error) {
    if (error.kind == ClientErrorKind.authExpired) {
      ref.read(authServiceProvider).clearLocalSession();
      ref.read(sessionProvider.notifier).clearAuthentication();
      if (mounted) context.go('/auth');
      return;
    }
    if (!mounted) return;
    setState(() {
      _loading = false;
      _message = error.toString();
    });
  }

  @override
  Widget build(BuildContext context) {
    final session = ref.watch(sessionProvider);
    return Scaffold(
      appBar: AppBar(
        title: const Text('Settings'),
        leading: IconButton(
          onPressed: () => context.go('/dashboard'),
          icon: const Icon(Icons.arrow_back),
        ),
      ),
      body: SafeArea(
        child: ListView(
          padding: const EdgeInsets.all(24),
          children: [
            Text(session.user?.displayName ?? 'Not signed in', style: Theme.of(context).textTheme.titleLarge),
            const SizedBox(height: 4),
            Text(session.server?.origin.toString() ?? ''),
            const SizedBox(height: 24),
            FilledButton(
              onPressed: session.isAuthenticated ? _logout : null,
              child: const Text('Sign out'),
            ),
            const SizedBox(height: 8),
            OutlinedButton(
              onPressed: session.isAuthenticated ? _logoutAll : null,
              child: const Text('Sign out all devices'),
            ),
            const SizedBox(height: 24),
            Text('Authorized devices', style: Theme.of(context).textTheme.titleMedium),
            const SizedBox(height: 8),
            if (_loading)
              const LinearProgressIndicator()
            else if (_message != null)
              Text(_message!, style: TextStyle(color: Theme.of(context).colorScheme.error))
            else if (_sessions.isEmpty)
              const Text('No active sessions found.')
            else
              ..._sessions.map(
                (item) => ListTile(
                  contentPadding: EdgeInsets.zero,
                  title: Text(item.deviceName ?? item.clientName ?? item.clientPlatform ?? 'Duskcue client'),
                  subtitle: Text('${item.clientName ?? 'Unknown app'} ${item.clientVersion ?? ''}\nLast active ${item.lastActiveAt.toLocal()}'),
                  isThreeLine: true,
                  trailing: IconButton(
                    tooltip: 'Sign out device',
                    onPressed: () => _deleteSession(item.id),
                    icon: const Icon(Icons.logout),
                  ),
                ),
              ),
          ],
        ),
      ),
    );
  }
}

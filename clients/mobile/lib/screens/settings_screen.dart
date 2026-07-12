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

import 'dart:async';

import 'package:duskcue_mobile/api/client_error.dart';
import 'package:duskcue_mobile/models/auth_models.dart';
import 'package:duskcue_mobile/services/quality_service.dart';
import 'package:duskcue_mobile/services/service_providers.dart';
import 'package:duskcue_mobile/stores/session_store.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';

class SettingsScreen extends ConsumerStatefulWidget {
  const SettingsScreen({super.key});

  @override
  ConsumerState<SettingsScreen> createState() => _SettingsScreenState();
}

class _SettingsScreenState extends ConsumerState<SettingsScreen> {
  final TextEditingController _passkeyNameController = TextEditingController(text: 'Mobile passkey');

  List<SessionDetail> _sessions = const [];
  List<PasskeySummary> _passkeys = const [];
  List<NotificationPreference> _preferences = const [];
  List<PushDeviceSummary> _pushDevices = const [];
  String? _currentDeviceId;
  QualityMode _defaultQualityMode = QualityMode.auto;
  bool _loading = true;
  bool _savingPasskey = false;
  String? _message;

  @override
  void initState() {
    super.initState();
    unawaited(_load());
  }

  @override
  void dispose() {
    _passkeyNameController.dispose();
    super.dispose();
  }

  Future<void> _load() async {
    setState(() {
      _loading = true;
      _message = null;
    });
    try {
      final auth = ref.read(authServiceProvider);
      final quality = ref.read(qualityServiceProvider);
      final identity = await ref.read(deviceIdentityProvider).current();
      final sessions = await auth.listSessions();
      final passkeys = await auth.listPasskeys();
      final preferences = await auth.listNotificationPreferences();
      final pushDevices = await auth.listPushDevices();
      final defaultQuality = await quality.defaultSelection();
      if (!mounted) return;
      setState(() {
        _currentDeviceId = identity.deviceId;
        _sessions = sessions;
        _passkeys = passkeys;
        _preferences = preferences;
        _pushDevices = pushDevices;
        _defaultQualityMode = defaultQuality.mode;
        _loading = false;
      });
    } on ClientError catch (error) {
      _handleError(error);
    } catch (_) {
      if (!mounted) return;
      setState(() {
        _loading = false;
        _message = 'Unable to load settings.';
      });
    }
  }

  Future<void> _registerPasskey() async {
    final name = _passkeyNameController.text.trim();
    if (name.isEmpty || _savingPasskey) return;
    setState(() => _savingPasskey = true);
    try {
      await ref.read(authServiceProvider).registerPasskey(name);
      final passkeys = await ref.read(authServiceProvider).listPasskeys();
      if (!mounted) return;
      setState(() {
        _passkeys = passkeys;
        _savingPasskey = false;
        _message = 'Passkey registered.';
      });
    } on ClientError catch (error) {
      _handleError(error);
      if (mounted) setState(() => _savingPasskey = false);
    } on UnsupportedError catch (error) {
      if (!mounted) return;
      setState(() {
        _savingPasskey = false;
        _message = error.message;
      });
    } catch (_) {
      if (!mounted) return;
      setState(() {
        _savingPasskey = false;
        _message = 'Unable to register passkey on this device.';
      });
    }
  }

  Future<void> _deletePasskey(String passkeyId) async {
    await ref.read(authServiceProvider).deletePasskey(passkeyId);
    if (!mounted) return;
    setState(() => _passkeys = _passkeys.where((item) => item.id != passkeyId).toList(growable: false));
  }

  Future<void> _deleteSession(String sessionId) async {
    try {
      await ref.read(authServiceProvider).deleteSession(sessionId);
      if (!mounted) return;
      setState(() => _sessions = _sessions.where((item) => item.id != sessionId).toList(growable: false));
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

  Future<void> _chooseServer() async {
    await ref.read(authServiceProvider).clearLocalSession();
    ref.read(sessionProvider.notifier).clearAuthentication();
    if (mounted) context.go('/server');
  }

  Future<void> _savePreference(NotificationPreference preference, String channel, bool value) async {
    try {
      final updated = switch (channel) {
        'in_app' => await ref.read(authServiceProvider).updateNotificationPreference(preference, inAppEnabled: value),
        'webhook' => await ref.read(authServiceProvider).updateNotificationPreference(preference, webhookEnabled: value),
        'push' => await ref.read(authServiceProvider).updateNotificationPreference(preference, pushEnabled: value),
        _ => preference,
      };
      if (!mounted) return;
      setState(() {
        _preferences = _preferences
            .map((item) => item.notificationTypeId == updated.notificationTypeId ? updated : item)
            .toList(growable: false);
      });
    } on ClientError catch (error) {
      _handleError(error);
    }
  }

  Future<void> _deletePushDevice(String deviceId) async {
    await ref.read(authServiceProvider).deletePushDevice(deviceId);
    if (!mounted) return;
    setState(() => _pushDevices = _pushDevices.where((item) => item.id != deviceId).toList(growable: false));
  }

  Future<void> _setDefaultQuality(QualityMode mode) async {
    setState(() => _defaultQualityMode = mode);
    await ref.read(qualityServiceProvider).saveDefaultSelection(mode);
  }

  Future<void> _copyAdminSettingsUrl() async {
    final server = ref.read(sessionProvider).server;
    if (server == null) return;
    final url = server.origin.replace(path: '/settings').toString();
    await Clipboard.setData(ClipboardData(text: url));
    if (!mounted) return;
    ScaffoldMessenger.of(context).showSnackBar(const SnackBar(content: Text('Settings URL copied.')));
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
    final theme = Theme.of(context);
    return Scaffold(
      appBar: AppBar(
        title: const Text('Settings'),
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
              _SectionHeader(
                title: session.user?.displayName ?? 'Not signed in',
                subtitle: [
                  if (session.user?.username.isNotEmpty == true) '@${session.user!.username}',
                  if (session.user?.role.isNotEmpty == true) session.user!.role,
                ].join(' · '),
              ),
              if (_message != null) ...[
                const SizedBox(height: 8),
                Text(_message!, style: TextStyle(color: theme.colorScheme.error)),
              ],
              const SizedBox(height: 12),
              if (_loading) const LinearProgressIndicator(),
              _ServerSettingsCard(
                serverName: session.server?.displayName,
                serverOrigin: session.server?.origin.toString(),
                networkMode: session.server?.networkMode.label,
                onChooseServer: _chooseServer,
                onCopyAdminSettingsUrl: _copyAdminSettingsUrl,
              ),
              const SizedBox(height: 12),
              _QualitySettingsCard(
                value: _defaultQualityMode,
                onChanged: (mode) {
                  if (mode != null) unawaited(_setDefaultQuality(mode));
                },
              ),
              const SizedBox(height: 12),
              ExpansionTile(
                leading: const Icon(Icons.devices),
                title: const Text('Sessions'),
                subtitle: Text('${_sessions.length} authorized device${_sessions.length == 1 ? '' : 's'}'),
                children: [
                  ..._sessions.map((item) {
                    final isCurrent = item.deviceId != null && item.deviceId == _currentDeviceId;
                    return ListTile(
                      title: Text(item.deviceName ?? item.clientName ?? item.clientPlatform ?? 'Duskcue client'),
                      subtitle: Text(
                        [
                          if (isCurrent) 'Current device',
                          '${item.clientName ?? 'Unknown app'} ${item.clientVersion ?? ''}'.trim(),
                          if (item.ipAddress != null) item.ipAddress!,
                          'Last active ${_formatDate(item.lastActiveAt)}',
                        ].where((value) => value.isNotEmpty).join('\n'),
                      ),
                      isThreeLine: true,
                      trailing: IconButton(
                        tooltip: isCurrent ? 'Sign out here' : 'Sign out device',
                        onPressed: session.isAuthenticated ? () => unawaited(isCurrent ? _logout() : _deleteSession(item.id)) : null,
                        icon: const Icon(Icons.logout),
                      ),
                    );
                  }),
                  OverflowBar(
                    children: [
                      OutlinedButton.icon(
                        onPressed: session.isAuthenticated ? _logout : null,
                        icon: const Icon(Icons.logout),
                        label: const Text('Sign out'),
                      ),
                      FilledButton.icon(
                        onPressed: session.isAuthenticated ? _logoutAll : null,
                        icon: const Icon(Icons.logout_outlined),
                        label: const Text('All devices'),
                      ),
                    ],
                  ),
                ],
              ),
              ExpansionTile(
                leading: const Icon(Icons.key),
                title: const Text('Passkeys'),
                subtitle: Text('${_passkeys.length} registered'),
                children: [
                  Padding(
                    padding: const EdgeInsets.fromLTRB(16, 8, 16, 8),
                    child: Row(
                      children: [
                        Expanded(
                          child: TextField(
                            controller: _passkeyNameController,
                            decoration: const InputDecoration(labelText: 'Passkey name', border: OutlineInputBorder()),
                          ),
                        ),
                        const SizedBox(width: 8),
                        FilledButton.icon(
                          onPressed: _savingPasskey ? null : _registerPasskey,
                          icon: const Icon(Icons.add),
                          label: Text(_savingPasskey ? 'Adding' : 'Add'),
                        ),
                      ],
                    ),
                  ),
                  if (_passkeys.isEmpty)
                    const ListTile(title: Text('No passkeys registered for this account.'))
                  else
                    ..._passkeys.map(
                      (item) => ListTile(
                        title: Text(item.name),
                        subtitle: Text(
                          [
                            if (item.transports.isNotEmpty) item.transports.join(', '),
                            'Created ${_formatDate(item.createdAt)}',
                            if (item.lastUsedAt != null) 'Last used ${_formatDate(item.lastUsedAt!)}',
                          ].join('\n'),
                        ),
                        isThreeLine: true,
                        trailing: IconButton(
                          tooltip: 'Delete passkey',
                          onPressed: () => unawaited(_deletePasskey(item.id)),
                          icon: const Icon(Icons.delete_outline),
                        ),
                      ),
                    ),
                ],
              ),
              ExpansionTile(
                leading: const Icon(Icons.notifications),
                title: const Text('Notification preferences'),
                subtitle: Text('${_preferences.length} notification type${_preferences.length == 1 ? '' : 's'}'),
                children: [
                  if (_preferences.isEmpty)
                    const ListTile(title: Text('No notification preferences available.'))
                  else
                    ..._preferences.map(
                      (item) => Column(
                        children: [
                          ListTile(
                            title: Text(_titleCase(item.name.replaceAll('_', ' '))),
                            subtitle: Text('${item.category} · ${item.priority}${item.isUsingDefaults ? ' · default' : ''}'),
                          ),
                          SwitchListTile(
                            title: const Text('In-app'),
                            value: item.inAppEnabled,
                            onChanged: (value) => unawaited(_savePreference(item, 'in_app', value)),
                          ),
                          SwitchListTile(
                            title: const Text('Push'),
                            value: item.pushEnabled,
                            onChanged: (value) => unawaited(_savePreference(item, 'push', value)),
                          ),
                          SwitchListTile(
                            title: const Text('Webhook'),
                            value: item.webhookEnabled,
                            onChanged: (value) => unawaited(_savePreference(item, 'webhook', value)),
                          ),
                          const Divider(height: 1),
                        ],
                      ),
                    ),
                ],
              ),
              ExpansionTile(
                leading: const Icon(Icons.phonelink_ring),
                title: const Text('Push devices'),
                subtitle: Text('${_pushDevices.length} registered'),
                children: [
                  if (_pushDevices.isEmpty)
                    const ListTile(title: Text('No push devices registered.'))
                  else
                    ..._pushDevices.map(
                      (item) => ListTile(
                        title: Text(item.deviceName ?? item.platform ?? item.provider),
                        subtitle: Text(
                          [
                            _providerLabel(item.provider),
                            if (item.appVersion != null) 'v${item.appVersion}',
                            if (item.tokenPreview.isNotEmpty) item.tokenPreview,
                            if (item.lastSeenAt != null) 'Last seen ${_formatDate(item.lastSeenAt!)}',
                            if (item.invalidatedAt != null) 'Invalidated ${_formatDate(item.invalidatedAt!)}',
                            item.isActive ? 'Active' : 'Inactive',
                          ].join('\n'),
                        ),
                        isThreeLine: true,
                        trailing: IconButton(
                          tooltip: 'Revoke push device',
                          onPressed: () => unawaited(_deletePushDevice(item.id)),
                          icon: const Icon(Icons.delete_outline),
                        ),
                      ),
                    ),
                ],
              ),
              const SizedBox(height: 12),
              const _AdminWorkflowNote(),
            ],
          ),
        ),
      ),
    );
  }
}

class _SectionHeader extends StatelessWidget {
  const _SectionHeader({required this.title, required this.subtitle});

  final String title;
  final String subtitle;

  @override
  Widget build(BuildContext context) {
    return ListTile(
      contentPadding: EdgeInsets.zero,
      leading: const CircleAvatar(child: Icon(Icons.person)),
      title: Text(title, style: Theme.of(context).textTheme.titleLarge),
      subtitle: subtitle.isEmpty ? null : Text(subtitle),
    );
  }
}

class _ServerSettingsCard extends StatelessWidget {
  const _ServerSettingsCard({
    required this.serverName,
    required this.serverOrigin,
    required this.networkMode,
    required this.onChooseServer,
    required this.onCopyAdminSettingsUrl,
  });

  final String? serverName;
  final String? serverOrigin;
  final String? networkMode;
  final VoidCallback onChooseServer;
  final VoidCallback onCopyAdminSettingsUrl;

  @override
  Widget build(BuildContext context) {
    return Card(
      child: Padding(
        padding: const EdgeInsets.all(16),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text('Server connection', style: Theme.of(context).textTheme.titleMedium),
            const SizedBox(height: 8),
            Text(serverName ?? serverOrigin ?? 'No server selected'),
            if (serverOrigin != null) Text(serverOrigin!, style: Theme.of(context).textTheme.bodySmall),
            if (networkMode != null) Text('Network mode: $networkMode', style: Theme.of(context).textTheme.bodySmall),
            const SizedBox(height: 12),
            Wrap(
              spacing: 8,
              runSpacing: 8,
              children: [
                OutlinedButton.icon(
                  onPressed: onChooseServer,
                  icon: const Icon(Icons.dns_outlined),
                  label: const Text('Choose server'),
                ),
                OutlinedButton.icon(
                  onPressed: serverOrigin == null ? null : onCopyAdminSettingsUrl,
                  icon: const Icon(Icons.copy),
                  label: const Text('Copy web settings URL'),
                ),
              ],
            ),
          ],
        ),
      ),
    );
  }
}

class _QualitySettingsCard extends StatelessWidget {
  const _QualitySettingsCard({required this.value, required this.onChanged});

  final QualityMode value;
  final ValueChanged<QualityMode?> onChanged;

  @override
  Widget build(BuildContext context) {
    return Card(
      child: Padding(
        padding: const EdgeInsets.all(16),
        child: DropdownButtonFormField<QualityMode>(
          initialValue: value,
          decoration: const InputDecoration(
            labelText: 'Default playback quality',
            border: OutlineInputBorder(),
          ),
          items: QualityMode.values
              .map((mode) => DropdownMenuItem<QualityMode>(value: mode, child: Text(mode.label)))
              .toList(growable: false),
          onChanged: onChanged,
        ),
      ),
    );
  }
}

class _AdminWorkflowNote extends StatelessWidget {
  const _AdminWorkflowNote();

  @override
  Widget build(BuildContext context) {
    return const Card(
      child: ListTile(
        leading: Icon(Icons.admin_panel_settings_outlined),
        title: Text('Admin settings'),
        subtitle: Text('Server, library, backup, migration, storage, and full quality policy administration remain web-first.'),
      ),
    );
  }
}

String _formatDate(DateTime value) {
  return value.toLocal().toString().split('.').first;
}

String _providerLabel(String value) {
  return switch (value) {
    'fcm' => 'FCM',
    'apns' => 'APNs',
    'unifiedpush' => 'UnifiedPush',
    _ => value,
  };
}

String _titleCase(String value) {
  return value
      .split(' ')
      .where((part) => part.isNotEmpty)
      .map((part) => '${part[0].toUpperCase()}${part.substring(1)}')
      .join(' ');
}

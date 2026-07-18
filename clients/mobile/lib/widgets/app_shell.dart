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

import 'package:duskcue_mobile/l10n/app_strings.dart';
import 'package:duskcue_mobile/models/realtime_models.dart';
import 'package:duskcue_mobile/services/push_registration_service.dart';
import 'package:duskcue_mobile/services/service_providers.dart';
import 'package:duskcue_mobile/stores/download_manager_store.dart';
import 'package:duskcue_mobile/stores/realtime_store.dart';
import 'package:duskcue_mobile/stores/session_store.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';

class AppShell extends ConsumerStatefulWidget {
  const AppShell({required this.navigationShell, super.key});

  final StatefulNavigationShell navigationShell;

  @override
  ConsumerState<AppShell> createState() => _AppShellState();
}

class _AppShellState extends ConsumerState<AppShell>
    with WidgetsBindingObserver {
  static const _fallbackPollInterval = Duration(seconds: 60);

  StreamSubscription<RealtimeEvent>? _eventSubscription;
  StreamSubscription<RealtimeConnectionStatus>? _statusSubscription;
  StreamSubscription<PushNotificationTap>? _pushTapSubscription;
  Timer? _fallbackTimer;
  AppLifecycleState _lifecycleState = AppLifecycleState.resumed;

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addObserver(this);
    final realtime = ref.read(realtimeServiceProvider);
    _eventSubscription = realtime.events.listen(_handleRealtimeEvent);
    _statusSubscription = realtime.status.listen((status) {
      ref.read(realtimeProvider.notifier).setStatus(status);
    });
    _pushTapSubscription = ref
        .read(pushRegistrationServiceProvider)
        .notificationTaps
        .listen(_handlePushTap);
    _fallbackTimer = Timer.periodic(
      _fallbackPollInterval,
      (_) => unawaited(_pollFallback()),
    );
    WidgetsBinding.instance.addPostFrameCallback(
      (_) => _syncRealtime(refresh: true),
    );
  }

  @override
  void dispose() {
    WidgetsBinding.instance.removeObserver(this);
    _fallbackTimer?.cancel();
    _eventSubscription?.cancel();
    _statusSubscription?.cancel();
    _pushTapSubscription?.cancel();
    unawaited(ref.read(realtimeServiceProvider).disconnect());
    unawaited(ref.read(pushRegistrationServiceProvider).stop());
    super.dispose();
  }

  @override
  void didChangeAppLifecycleState(AppLifecycleState state) {
    _lifecycleState = state;
    _syncRealtime(refresh: state == AppLifecycleState.resumed);
  }

  void _syncRealtime({bool refresh = false}) {
    if (!mounted) return;
    final session = ref.read(sessionProvider);
    final foreground = _lifecycleState == AppLifecycleState.resumed;
    final realtime = ref.read(realtimeServiceProvider);
    if (session.isAuthenticated && session.isProfileScopeReady && foreground) {
      unawaited(realtime.connect());
      unawaited(ref.read(pushRegistrationServiceProvider).startOrRefresh());
      unawaited(ref.read(qualityServiceProvider).reportCapabilities());
      unawaited(
        ref.read(downloadManagerProvider.notifier).loadForCurrentSession(),
      );
      if (refresh) {
        unawaited(_pollFallback(force: true));
      }
    } else {
      unawaited(realtime.disconnect());
    }
  }

  void _handlePushTap(PushNotificationTap tap) {
    final session = ref.read(sessionProvider);
    if (!mounted) return;
    if (!session.isAuthenticated) {
      context.go('/auth');
      return;
    }
    context.go(tap.route);
  }

  Future<void> _pollFallback({bool force = false}) async {
    if (!mounted) return;
    final session = ref.read(sessionProvider);
    final realtimeState = ref.read(realtimeProvider);
    if (!session.isAuthenticated || !session.isProfileScopeReady) return;
    if (!force && realtimeState.status == RealtimeConnectionStatus.connected) {
      return;
    }
    try {
      final count = await ref
          .read(contentServiceProvider)
          .unreadNotificationCount();
      ref.read(realtimeProvider.notifier).setUnreadCount(count);
    } catch (_) {}
  }

  void _handleRealtimeEvent(RealtimeEvent event) {
    ref.read(realtimeProvider.notifier).recordEvent(event);
    switch (event.type) {
      case 'notification':
        ref.read(realtimeProvider.notifier).incrementUnread();
        _showNotification(event);
        unawaited(_pollFallback(force: true));
        break;
      case 'session_kicked':
        unawaited(ref.read(authServiceProvider).clearLocalSession());
        ref.read(sessionProvider.notifier).clearAuthentication();
        if (mounted) context.go('/auth');
        break;
      case 'transcode_progress':
      case 'playback_updated':
      case 'storyboard_progress':
      case 'scan_progress':
      case 'admin_task':
        break;
      case 'download_job_status':
        unawaited(
          ref.read(downloadManagerProvider.notifier).handleRealtimeEvent(event),
        );
        break;
    }
  }

  void _showNotification(RealtimeEvent event) {
    final data = event.jsonData;
    final title = data['title']?.toString();
    final body = data['body']?.toString() ?? data['message']?.toString();
    final message = [
      title,
      body,
    ].whereType<String>().where((value) => value.isNotEmpty).join('\n');
    if (message.isEmpty || !mounted) return;
    ScaffoldMessenger.of(
      context,
    ).showSnackBar(SnackBar(content: Text(message)));
  }

  @override
  Widget build(BuildContext context) {
    final session = ref.watch(sessionProvider);
    final realtime = ref.watch(realtimeProvider);
    final strings = AppStrings.of(context);
    WidgetsBinding.instance.addPostFrameCallback((_) => _syncRealtime());

    return Scaffold(
      body: widget.navigationShell,
      bottomNavigationBar: NavigationBar(
        selectedIndex: widget.navigationShell.currentIndex,
        onDestinationSelected: (index) {
          widget.navigationShell.goBranch(
            index,
            initialLocation: index == widget.navigationShell.currentIndex,
          );
        },
        destinations: [
          NavigationDestination(
            icon: const Icon(Icons.home_outlined),
            selectedIcon: const Icon(Icons.home),
            label: strings.dashboard,
          ),
          NavigationDestination(
            icon: const Icon(Icons.video_library_outlined),
            selectedIcon: const Icon(Icons.video_library),
            label: strings.libraries,
          ),
          NavigationDestination(
            icon: const Icon(Icons.search),
            selectedIcon: const Icon(Icons.manage_search),
            label: strings.search,
          ),
          NavigationDestination(
            icon: const Icon(Icons.collections_bookmark_outlined),
            selectedIcon: const Icon(Icons.collections_bookmark),
            label: strings.collections,
          ),
          NavigationDestination(
            icon: const Icon(Icons.download_outlined),
            selectedIcon: const Icon(Icons.download),
            label: strings.downloads,
          ),
          NavigationDestination(
            icon: _NotificationIcon(
              count: session.isAuthenticated ? realtime.unreadCount : 0,
              selected: false,
            ),
            selectedIcon: _NotificationIcon(
              count: session.isAuthenticated ? realtime.unreadCount : 0,
              selected: true,
            ),
            label: strings.notifications,
          ),
          NavigationDestination(
            icon: const Icon(Icons.settings_outlined),
            selectedIcon: const Icon(Icons.settings),
            label: strings.settings,
          ),
        ],
      ),
    );
  }
}

class _NotificationIcon extends StatelessWidget {
  const _NotificationIcon({required this.count, required this.selected});

  final int count;
  final bool selected;

  @override
  Widget build(BuildContext context) {
    final icon = Icon(
      selected ? Icons.notifications : Icons.notifications_outlined,
    );
    if (count <= 0) return icon;
    return Badge(
      label: Text(count > 99 ? '99+' : count.toString()),
      child: icon,
    );
  }
}

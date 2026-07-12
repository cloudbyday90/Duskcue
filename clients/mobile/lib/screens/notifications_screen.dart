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

import 'package:duskcue_mobile/l10n/app_strings.dart';
import 'package:duskcue_mobile/models/content_models.dart';
import 'package:duskcue_mobile/services/service_providers.dart';
import 'package:duskcue_mobile/stores/realtime_store.dart';
import 'package:duskcue_mobile/widgets/mobile_state_views.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

class NotificationsScreen extends ConsumerStatefulWidget {
  const NotificationsScreen({super.key});

  @override
  ConsumerState<NotificationsScreen> createState() => _NotificationsScreenState();
}

class _NotificationsScreenState extends ConsumerState<NotificationsScreen> {
  List<NotificationSummary> _items = const [];
  String? _nextCursor;
  bool _loading = true;
  bool _loadingMore = false;
  Object? _error;

  @override
  void initState() {
    super.initState();
    _load();
  }

  Future<void> _load() async {
    setState(() {
      _loading = true;
      _error = null;
      _nextCursor = null;
    });
    try {
      final service = ref.read(contentServiceProvider);
      final page = await service.listNotifications();
      final unread = await service.unreadNotificationCount();
      if (!mounted) return;
      setState(() {
        _items = page.items;
        _nextCursor = page.nextCursor;
        _loading = false;
      });
      ref.read(realtimeProvider.notifier).setUnreadCount(unread);
    } catch (error) {
      if (!mounted) return;
      setState(() {
        _error = error;
        _loading = false;
      });
    }
  }

  Future<void> _loadMore() async {
    final cursor = _nextCursor;
    if (cursor == null || _loadingMore) return;
    setState(() => _loadingMore = true);
    try {
      final page = await ref.read(contentServiceProvider).listNotifications(cursor: cursor);
      if (!mounted) return;
      setState(() {
        _items = [..._items, ...page.items];
        _nextCursor = page.nextCursor;
        _loadingMore = false;
      });
    } catch (error) {
      if (!mounted) return;
      setState(() {
        _error = error;
        _loadingMore = false;
      });
    }
  }

  Future<void> _markRead(NotificationSummary item) async {
    if (item.isRead) return;
    await ref.read(contentServiceProvider).markNotificationRead(item.id);
    await _load();
  }

  Future<void> _markAllRead() async {
    await ref.read(contentServiceProvider).markAllNotificationsRead();
    await _load();
  }

  @override
  Widget build(BuildContext context) {
    final strings = AppStrings.of(context);

    return Scaffold(
      appBar: AppBar(
        title: Text(strings.notifications),
        actions: [
          TextButton(onPressed: _items.any((item) => !item.isRead) ? _markAllRead : null, child: Text(strings.markAllRead)),
        ],
      ),
      body: SafeArea(
        child: _loading
            ? const Center(child: CircularProgressIndicator())
            : _error != null
                ? ErrorState(message: userFacingError(context, _error!), onRetry: _load)
                : RefreshIndicator(
                    onRefresh: _load,
                    child: _items.isEmpty
                        ? ListView(children: [EmptyState(icon: Icons.notifications_outlined, message: strings.emptyNotifications)])
                        : ListView.separated(
                            padding: const EdgeInsets.all(16),
                            itemCount: _items.length + 1,
                            separatorBuilder: (context, index) => const Divider(height: 1),
                            itemBuilder: (context, index) {
                              if (index == _items.length) {
                                if (_nextCursor == null) return const SizedBox.shrink();
                                return Padding(
                                  padding: const EdgeInsets.symmetric(vertical: 16),
                                  child: OutlinedButton(
                                    onPressed: _loadingMore ? null : _loadMore,
                                    child: Text(_loadingMore ? '...' : strings.loadMore),
                                  ),
                                );
                              }
                              final item = _items[index];
                              return ListTile(
                                leading: Icon(item.isRead ? Icons.notifications_none : Icons.notifications_active),
                                title: Text(item.title),
                                subtitle: Text(
                                  [item.body, item.createdAt?.toLocal().toString()].whereType<String>().join('\n'),
                                  maxLines: 3,
                                  overflow: TextOverflow.ellipsis,
                                ),
                                isThreeLine: item.body != null,
                                trailing: Text(item.isRead ? strings.read : strings.unread),
                                onTap: () => _markRead(item),
                              );
                            },
                          ),
                  ),
      ),
    );
  }
}

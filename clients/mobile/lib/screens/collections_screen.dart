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
import 'package:duskcue_mobile/widgets/mobile_state_views.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';

class CollectionsScreen extends ConsumerStatefulWidget {
  const CollectionsScreen({super.key});

  @override
  ConsumerState<CollectionsScreen> createState() => _CollectionsScreenState();
}

class _CollectionsScreenState extends ConsumerState<CollectionsScreen> {
  List<CollectionSummary> _items = const [];
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
      final page = await ref.read(contentServiceProvider).listCollections();
      if (!mounted) return;
      setState(() {
        _items = page.items;
        _nextCursor = page.nextCursor;
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

  Future<void> _loadMore() async {
    final cursor = _nextCursor;
    if (cursor == null || _loadingMore) return;
    setState(() => _loadingMore = true);
    try {
      final page = await ref.read(contentServiceProvider).listCollections(cursor: cursor);
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

  @override
  Widget build(BuildContext context) {
    final strings = AppStrings.of(context);

    return Scaffold(
      appBar: AppBar(title: Text(strings.collections)),
      body: SafeArea(
        child: _loading
            ? const Center(child: CircularProgressIndicator())
            : _error != null
                ? ErrorState(message: userFacingError(context, _error!), onRetry: _load)
                : RefreshIndicator(
                    onRefresh: _load,
                    child: _items.isEmpty
                        ? ListView(children: [EmptyState(icon: Icons.collections_bookmark_outlined, message: strings.emptyCollections)])
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
                              final collection = _items[index];
                              return ListTile(
                                leading: const Icon(Icons.collections_bookmark_outlined),
                                title: Text(collection.name),
                                subtitle: Text([collection.description, if (collection.itemCount != null) '${collection.itemCount} items'].whereType<String>().join(' · ')),
                                trailing: const Icon(Icons.chevron_right),
                                onTap: () => context.go('/collections/${collection.id}'),
                              );
                            },
                          ),
                  ),
      ),
    );
  }
}

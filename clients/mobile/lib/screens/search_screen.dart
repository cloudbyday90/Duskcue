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
import 'package:duskcue_mobile/models/content_models.dart';
import 'package:duskcue_mobile/services/service_providers.dart';
import 'package:duskcue_mobile/widgets/content_cards.dart';
import 'package:duskcue_mobile/widgets/mobile_state_views.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';

class SearchScreen extends ConsumerStatefulWidget {
  const SearchScreen({super.key});

  @override
  ConsumerState<SearchScreen> createState() => _SearchScreenState();
}

class _SearchScreenState extends ConsumerState<SearchScreen> {
  final _controller = TextEditingController();
  Timer? _debounce;
  List<MediaItemSummary> _items = const [];
  String? _nextCursor;
  bool _loading = false;
  bool _loadingMore = false;
  Object? _error;

  @override
  void dispose() {
    _debounce?.cancel();
    _controller.dispose();
    super.dispose();
  }

  void _queueSearch() {
    _debounce?.cancel();
    _debounce = Timer(const Duration(milliseconds: 350), () => _search(reset: true));
  }

  Future<void> _search({required bool reset}) async {
    final query = _controller.text.trim();
    if (query.isEmpty) {
      setState(() {
        _items = const [];
        _nextCursor = null;
        _error = null;
        _loading = false;
      });
      return;
    }

    setState(() {
      if (reset) _loading = true;
      if (!reset) _loadingMore = true;
      _error = null;
    });

    try {
      final page = await ref.read(contentServiceProvider).search(query, cursor: reset ? null : _nextCursor);
      if (!mounted) return;
      setState(() {
        _items = reset ? page.items : [..._items, ...page.items];
        _nextCursor = page.nextCursor;
        _loading = false;
        _loadingMore = false;
      });
    } catch (error) {
      if (!mounted) return;
      setState(() {
        _error = error;
        _loading = false;
        _loadingMore = false;
      });
    }
  }

  @override
  Widget build(BuildContext context) {
    final strings = AppStrings.of(context);
    final query = _controller.text.trim();

    return Scaffold(
      appBar: AppBar(title: Text(strings.search)),
      body: SafeArea(
        child: Column(
          children: [
            Padding(
              padding: const EdgeInsets.all(16),
              child: TextField(
                controller: _controller,
                decoration: InputDecoration(
                  hintText: strings.searchHint,
                  prefixIcon: const Icon(Icons.search),
                  border: const OutlineInputBorder(),
                ),
                textInputAction: TextInputAction.search,
                onChanged: (_) => _queueSearch(),
                onSubmitted: (_) => _search(reset: true),
              ),
            ),
            Expanded(
              child: _loading
                  ? const Center(child: CircularProgressIndicator())
                  : _error != null
                      ? ErrorState(message: userFacingError(context, _error!), onRetry: () => _search(reset: true))
                      : query.isEmpty
                          ? EmptyState(icon: Icons.search, message: strings.searchEmpty)
                          : _items.isEmpty
                              ? EmptyState(icon: Icons.search_off, message: strings.searchNoResults)
                              : RefreshIndicator(
                                  onRefresh: () => _search(reset: true),
                                  child: ListView.separated(
                                    padding: const EdgeInsets.all(16),
                                    itemCount: _items.length + 1,
                                    separatorBuilder: (context, index) => const Divider(height: 1),
                                    itemBuilder: (context, index) {
                                      if (index == _items.length) {
                                        if (_nextCursor == null) return const SizedBox.shrink();
                                        return Padding(
                                          padding: const EdgeInsets.symmetric(vertical: 16),
                                          child: OutlinedButton(
                                            onPressed: _loadingMore ? null : () => _search(reset: false),
                                            child: Text(_loadingMore ? '...' : strings.loadMore),
                                          ),
                                        );
                                      }
                                      final item = _items[index];
                                      return MediaItemListTile(item: item, onTap: () => context.go('/media/${item.id}'));
                                    },
                                  ),
                                ),
            ),
          ],
        ),
      ),
    );
  }
}

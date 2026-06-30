import 'package:duskcue_mobile/l10n/app_strings.dart';
import 'package:duskcue_mobile/models/content_models.dart';
import 'package:duskcue_mobile/services/service_providers.dart';
import 'package:duskcue_mobile/widgets/content_cards.dart';
import 'package:duskcue_mobile/widgets/mobile_state_views.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';

class LibraryDetailScreen extends ConsumerStatefulWidget {
  const LibraryDetailScreen({required this.libraryId, super.key});

  final String libraryId;

  @override
  ConsumerState<LibraryDetailScreen> createState() => _LibraryDetailScreenState();
}

class _LibraryDetailScreenState extends ConsumerState<LibraryDetailScreen> {
  List<MediaItemSummary> _items = const [];
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
      final page = await ref.read(contentServiceProvider).listLibraryItems(widget.libraryId);
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
      final page = await ref.read(contentServiceProvider).listLibraryItems(widget.libraryId, cursor: cursor);
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
      appBar: AppBar(title: Text(strings.libraries)),
      body: SafeArea(
        child: _loading
            ? const Center(child: CircularProgressIndicator())
            : _error != null
                ? ErrorState(message: userFacingError(context, _error!), onRetry: _load)
                : RefreshIndicator(
                    onRefresh: _load,
                    child: ListView.separated(
                      padding: const EdgeInsets.all(16),
                      itemCount: _items.length + 1,
                      separatorBuilder: (context, index) => const Divider(height: 1),
                      itemBuilder: (context, index) {
                        if (_items.isEmpty) {
                          return EmptyState(icon: Icons.movie_outlined, message: strings.emptyItems);
                        }
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
                        return MediaItemListTile(item: item, onTap: () => context.go('/media/${item.id}'));
                      },
                    ),
                  ),
      ),
    );
  }
}

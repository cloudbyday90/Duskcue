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

import 'package:cached_network_image/cached_network_image.dart';
import 'package:duskcue_mobile/l10n/app_strings.dart';
import 'package:duskcue_mobile/models/content_models.dart';
import 'package:duskcue_mobile/models/download_models.dart';
import 'package:duskcue_mobile/services/service_providers.dart';
import 'package:duskcue_mobile/stores/download_manager_store.dart';
import 'package:duskcue_mobile/widgets/mobile_state_views.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';

class MediaDetailScreen extends ConsumerStatefulWidget {
  const MediaDetailScreen({required this.itemId, super.key});

  final String itemId;

  @override
  ConsumerState<MediaDetailScreen> createState() => _MediaDetailScreenState();
}

class _MediaDetailScreenState extends ConsumerState<MediaDetailScreen> {
  MediaItemSummary? _item;
  bool _loading = true;
  Object? _error;

  @override
  void initState() {
    super.initState();
    _load();
    WidgetsBinding.instance.addPostFrameCallback((_) {
      ref.read(downloadManagerProvider.notifier).loadForCurrentSession();
    });
  }

  Future<void> _load() async {
    setState(() {
      _loading = true;
      _error = null;
    });
    try {
      final item = await ref.read(contentServiceProvider).getMediaItem(widget.itemId);
      if (!mounted) return;
      setState(() {
        _item = item;
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

  @override
  Widget build(BuildContext context) {
    final strings = AppStrings.of(context);
    final item = _item;
    final service = ref.watch(contentServiceProvider);
    final downloadState = ref.watch(downloadManagerProvider);
    final offlineItem = _offlineItemFor(downloadState.items, widget.itemId);

    return Scaffold(
      appBar: AppBar(title: Text(item?.title ?? strings.mediaDetails)),
      body: SafeArea(
        child: _loading
            ? const Center(child: CircularProgressIndicator())
            : _error != null
                ? ErrorState(message: userFacingError(context, _error!), onRetry: _load)
                : RefreshIndicator(
                    onRefresh: _load,
                    child: ListView(
                      padding: const EdgeInsets.all(16),
                      children: [
                        AspectRatio(
                          aspectRatio: 16 / 9,
                          child: CachedNetworkImage(
                            imageUrl: service.artworkUri(item!.id, type: 'backdrop').toString(),
                            httpHeaders: service.mediaHeaders,
                            fit: BoxFit.cover,
                            errorWidget: (context, url, error) => ColoredBox(
                              color: Theme.of(context).colorScheme.surfaceContainerHighest,
                              child: const Center(child: Icon(Icons.movie_outlined, size: 56)),
                            ),
                          ),
                        ),
                        const SizedBox(height: 20),
                        Text(item.title, style: Theme.of(context).textTheme.headlineSmall),
                        const SizedBox(height: 8),
                        Text([item.mediaType, if (item.year != null) item.year.toString()].whereType<String>().join(' · ')),
                        if (item.overview != null && item.overview!.isNotEmpty) ...[
                          const SizedBox(height: 16),
                          Text(item.overview!),
                        ],
                        const SizedBox(height: 24),
                        FilledButton.icon(
                          onPressed: () => context.go('/play/${item.id}'),
                          icon: const Icon(Icons.play_arrow),
                          label: Text(strings.play),
                        ),
                        if (offlineItem != null) ...[
                          const SizedBox(height: 12),
                          FilledButton.tonalIcon(
                            onPressed: () => context.go('/play/${item.id}?offline=true'),
                            icon: const Icon(Icons.offline_pin),
                            label: Text(strings.playOffline),
                          ),
                        ],
                        const SizedBox(height: 12),
                        OutlinedButton.icon(
                          onPressed: () async {
                            await ref.read(downloadManagerProvider.notifier).queueDownload(item);
                            if (!context.mounted) return;
                            final downloadError = ref.read(downloadManagerProvider).error;
                            ScaffoldMessenger.of(context).showSnackBar(
                              SnackBar(content: Text(downloadError ?? strings.downloadQueued)),
                            );
                          },
                          icon: const Icon(Icons.download),
                          label: Text(strings.download),
                        ),
                      ],
                    ),
                  ),
      ),
    );
  }

  DownloadItem? _offlineItemFor(List<DownloadItem> items, String itemId) {
    for (final item in items) {
      if (item.mediaItemId == itemId && item.canPlayOffline) return item;
    }
    return null;
  }
}

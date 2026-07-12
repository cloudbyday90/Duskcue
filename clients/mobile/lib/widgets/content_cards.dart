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
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';

class MediaItemCard extends ConsumerWidget {
  const MediaItemCard({
    required this.item,
    required this.onTap,
    super.key,
  });

  final MediaItemSummary item;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final service = ref.watch(contentServiceProvider);
    final offlinePlayable = _hasOfflinePlayable(ref.watch(downloadManagerProvider).items, item.id);
    final theme = Theme.of(context);
    final strings = AppStrings.of(context);

    return Card(
      clipBehavior: Clip.antiAlias,
      child: InkWell(
        onTap: onTap,
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            AspectRatio(
              aspectRatio: 2 / 3,
              child: Stack(
                fit: StackFit.expand,
                children: [
                  CachedNetworkImage(
                    imageUrl: service.artworkUri(item.id).toString(),
                    httpHeaders: service.mediaHeaders,
                    fit: BoxFit.cover,
                    placeholder: (context, url) => const Center(child: CircularProgressIndicator()),
                    errorWidget: (context, url, error) => ColoredBox(
                      color: theme.colorScheme.surfaceContainerHighest,
                      child: const Center(child: Icon(Icons.movie_outlined)),
                    ),
                  ),
                  if (offlinePlayable)
                    Positioned(
                      right: 8,
                      bottom: 8,
                      child: IconButton.filledTonal(
                        tooltip: strings.playOffline,
                        onPressed: () => context.go('/play/${item.id}?offline=true'),
                        icon: const Icon(Icons.offline_pin),
                      ),
                    ),
                ],
              ),
            ),
            Padding(
              padding: const EdgeInsets.all(12),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text(item.title, maxLines: 2, overflow: TextOverflow.ellipsis, style: theme.textTheme.titleSmall),
                  const SizedBox(height: 4),
                  Text(
                    [item.mediaType, if (item.year != null) item.year.toString()].whereType<String>().join(' · '),
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                    style: theme.textTheme.bodySmall,
                  ),
                ],
              ),
            ),
          ],
        ),
      ),
    );
  }
}

class MediaItemListTile extends ConsumerWidget {
  const MediaItemListTile({
    required this.item,
    required this.onTap,
    super.key,
  });

  final MediaItemSummary item;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final service = ref.watch(contentServiceProvider);
    final offlinePlayable = _hasOfflinePlayable(ref.watch(downloadManagerProvider).items, item.id);
    final strings = AppStrings.of(context);

    return ListTile(
      leading: ClipRRect(
        borderRadius: BorderRadius.circular(6),
        child: SizedBox(
          width: 48,
          height: 72,
          child: CachedNetworkImage(
            imageUrl: service.artworkUri(item.id).toString(),
            httpHeaders: service.mediaHeaders,
            fit: BoxFit.cover,
            errorWidget: (context, url, error) => const ColoredBox(
              color: Colors.black12,
              child: Icon(Icons.movie_outlined),
            ),
          ),
        ),
      ),
      title: Text(item.title, maxLines: 1, overflow: TextOverflow.ellipsis),
      subtitle: Text(
        [item.mediaType, item.libraryName, if (item.year != null) item.year.toString()].whereType<String>().join(' · '),
        maxLines: 2,
        overflow: TextOverflow.ellipsis,
      ),
      trailing: offlinePlayable
          ? IconButton(
              tooltip: strings.playOffline,
              onPressed: () => context.go('/play/${item.id}?offline=true'),
              icon: const Icon(Icons.offline_pin),
            )
          : const Icon(Icons.chevron_right),
      onTap: onTap,
    );
  }
}

bool _hasOfflinePlayable(List<DownloadItem> items, String itemId) {
  for (final item in items) {
    if (item.mediaItemId == itemId && item.canPlayOffline == true) return true;
  }
  return false;
}

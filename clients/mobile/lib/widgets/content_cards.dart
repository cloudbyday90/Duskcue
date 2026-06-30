import 'package:cached_network_image/cached_network_image.dart';
import 'package:duskcue_mobile/models/content_models.dart';
import 'package:duskcue_mobile/services/service_providers.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

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
    final theme = Theme.of(context);

    return Card(
      clipBehavior: Clip.antiAlias,
      child: InkWell(
        onTap: onTap,
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            AspectRatio(
              aspectRatio: 2 / 3,
              child: CachedNetworkImage(
                imageUrl: service.artworkUri(item.id).toString(),
                httpHeaders: service.mediaHeaders,
                fit: BoxFit.cover,
                placeholder: (context, url) => const Center(child: CircularProgressIndicator()),
                errorWidget: (context, url, error) => ColoredBox(
                  color: theme.colorScheme.surfaceContainerHighest,
                  child: const Center(child: Icon(Icons.movie_outlined)),
                ),
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
      trailing: const Icon(Icons.chevron_right),
      onTap: onTap,
    );
  }
}

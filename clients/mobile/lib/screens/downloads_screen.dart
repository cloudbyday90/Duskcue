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
import 'package:duskcue_mobile/models/download_models.dart';
import 'package:duskcue_mobile/stores/download_manager_store.dart';
import 'package:duskcue_mobile/widgets/mobile_state_views.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';

class DownloadsScreen extends ConsumerStatefulWidget {
  const DownloadsScreen({super.key});

  @override
  ConsumerState<DownloadsScreen> createState() => _DownloadsScreenState();
}

class _DownloadsScreenState extends ConsumerState<DownloadsScreen> {
  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addPostFrameCallback((_) {
      ref.read(downloadManagerProvider.notifier).loadForCurrentSession();
    });
  }

  @override
  Widget build(BuildContext context) {
    final strings = AppStrings.of(context);
    final state = ref.watch(downloadManagerProvider);
    final manager = ref.read(downloadManagerProvider.notifier);

    return Scaffold(
      appBar: AppBar(
        title: Text(strings.downloads),
        actions: [
          if (state.hasItems)
            IconButton(
              onPressed: state.loading ? null : manager.deleteAll,
              icon: const Icon(Icons.delete_sweep_outlined),
              tooltip: strings.deleteAll,
            ),
          IconButton(
            onPressed: state.loading ? null : manager.refreshJobs,
            icon: const Icon(Icons.refresh),
            tooltip: strings.retry,
          ),
        ],
      ),
      body: SafeArea(
        child: RefreshIndicator(
          onRefresh: manager.refreshJobs,
          child: ListView(
            padding: const EdgeInsets.all(16),
            children: [
              _DownloadSettingsPanel(
                settings: state.settings,
                onChanged: manager.updateSettings,
              ),
              const SizedBox(height: 16),
              if (state.error != null) ...[
                ErrorState(message: state.error!, onRetry: manager.loadForCurrentSession),
                const SizedBox(height: 16),
              ],
              if (state.loading && !state.hasItems)
                const Padding(
                  padding: EdgeInsets.only(top: 96),
                  child: Center(child: CircularProgressIndicator()),
                )
              else if (!state.hasItems)
                Padding(
                  padding: const EdgeInsets.only(top: 96),
                  child: EmptyState(icon: Icons.download_done_outlined, message: strings.emptyDownloads),
                )
              else
                ...state.items.map((item) => _DownloadTile(item: item)),
            ],
          ),
        ),
      ),
    );
  }
}

class _DownloadSettingsPanel extends StatelessWidget {
  const _DownloadSettingsPanel({
    required this.settings,
    required this.onChanged,
  });

  final DownloadManagerSettings settings;
  final ValueChanged<DownloadManagerSettings> onChanged;

  @override
  Widget build(BuildContext context) {
    final strings = AppStrings.of(context);
    return Card(
      margin: EdgeInsets.zero,
      child: Padding(
        padding: const EdgeInsets.all(16),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Row(
              children: [
                const Icon(Icons.tune),
                const SizedBox(width: 12),
                Text(strings.downloadSettings, style: Theme.of(context).textTheme.titleMedium),
              ],
            ),
            const SizedBox(height: 12),
            DropdownButtonFormField<DownloadQualityMode>(
              initialValue: settings.defaultQualityMode,
              decoration: InputDecoration(labelText: strings.quality),
              items: DownloadQualityMode.values
                  .map(
                    (mode) => DropdownMenuItem(
                      value: mode,
                      child: Text(mode.label),
                    ),
                  )
                  .toList(growable: false),
              onChanged: (mode) {
                if (mode != null) onChanged(settings.copyWith(defaultQualityMode: mode));
              },
            ),
            const SizedBox(height: 8),
            SwitchListTile(
              contentPadding: EdgeInsets.zero,
              value: settings.wifiOnly,
              onChanged: (value) => onChanged(settings.copyWith(wifiOnly: value)),
              title: Text(strings.wifiOnly),
            ),
            SwitchListTile(
              contentPadding: EdgeInsets.zero,
              value: settings.allowCellular,
              onChanged: settings.wifiOnly ? null : (value) => onChanged(settings.copyWith(allowCellular: value)),
              title: Text(strings.allowCellular),
            ),
            SwitchListTile(
              contentPadding: EdgeInsets.zero,
              value: settings.chargingOnly,
              onChanged: (value) => onChanged(settings.copyWith(chargingOnly: value)),
              title: Text(strings.chargingOnly),
            ),
            SwitchListTile(
              contentPadding: EdgeInsets.zero,
              value: settings.pauseOnLowStorage,
              onChanged: (value) => onChanged(settings.copyWith(pauseOnLowStorage: value)),
              title: Text(strings.pauseOnLowStorage),
            ),
            DropdownButtonFormField<int?>(
              initialValue: _storageCapValue(settings.storageCapBytes),
              decoration: const InputDecoration(labelText: 'Storage cap'),
              items: const [
                DropdownMenuItem<int?>(value: null, child: Text('No cap')),
                DropdownMenuItem<int?>(value: 5 * 1024 * 1024 * 1024, child: Text('5 GB')),
                DropdownMenuItem<int?>(value: 10 * 1024 * 1024 * 1024, child: Text('10 GB')),
                DropdownMenuItem<int?>(value: 25 * 1024 * 1024 * 1024, child: Text('25 GB')),
                DropdownMenuItem<int?>(value: 50 * 1024 * 1024 * 1024, child: Text('50 GB')),
              ],
              onChanged: (value) => onChanged(
                value == null ? settings.copyWith(clearStorageCap: true) : settings.copyWith(storageCapBytes: value),
              ),
            ),
            SwitchListTile(
              contentPadding: EdgeInsets.zero,
              value: settings.autoDeleteWatched,
              onChanged: (value) => onChanged(settings.copyWith(autoDeleteWatched: value)),
              title: const Text('Auto-delete watched downloads'),
            ),
          ],
        ),
      ),
    );
  }
}

int? _storageCapValue(int? value) {
  const caps = <int>{
    5 * 1024 * 1024 * 1024,
    10 * 1024 * 1024 * 1024,
    25 * 1024 * 1024 * 1024,
    50 * 1024 * 1024 * 1024,
  };
  return caps.contains(value) ? value : null;
}

class _DownloadTile extends ConsumerWidget {
  const _DownloadTile({required this.item});

  final DownloadItem item;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final strings = AppStrings.of(context);
    final manager = ref.read(downloadManagerProvider.notifier);
    final progress = (item.progressPercent.clamp(0, 100) / 100).toDouble();
    final subtitle = [
      item.status.label,
      if (item.waitingReason != null && item.waitingReason!.isNotEmpty) item.waitingReason,
      if (item.failureReason != null && item.failureReason!.isNotEmpty) item.failureReason,
    ].whereType<String>().join(' · ');

    return Card(
      margin: const EdgeInsets.only(bottom: 12),
      child: Padding(
        padding: const EdgeInsets.all(12),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Row(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Expanded(
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Text(item.title, style: Theme.of(context).textTheme.titleMedium),
                      const SizedBox(height: 4),
                      Text(subtitle),
                    ],
                  ),
                ),
                PopupMenuButton<String>(
                  onSelected: (action) {
                    switch (action) {
                      case 'play_offline':
                        context.go('/play/${item.mediaItemId}?offline=true');
                        break;
                      case 'open':
                        context.go('/media/${item.mediaItemId}');
                        break;
                      case 'save_offline':
                        manager.materializePackage(item);
                        break;
                      case 'pause':
                        manager.pause(item);
                        break;
                      case 'resume':
                        manager.resume(item);
                        break;
                      case 'retry':
                        manager.retry(item);
                        break;
                      case 'cancel':
                        manager.cancel(item);
                        break;
                      case 'delete':
                        manager.delete(item);
                        break;
                    }
                  },
                  itemBuilder: (context) => [
                    if (item.canPlayOffline) PopupMenuItem(value: 'play_offline', child: Text(strings.playOffline)),
                    PopupMenuItem(value: 'open', child: Text(strings.mediaDetails)),
                    if (item.status == DownloadItemStatus.ready)
                      PopupMenuItem(value: 'save_offline', child: Text(strings.saveOffline)),
                    if (item.status == DownloadItemStatus.downloading || item.status == DownloadItemStatus.ready)
                      PopupMenuItem(value: 'pause', child: Text(strings.pause)),
                    if (item.status == DownloadItemStatus.paused)
                      PopupMenuItem(value: 'resume', child: Text(strings.resume)),
                    if (item.canRetry) PopupMenuItem(value: 'retry', child: Text(strings.retry)),
                    if (!item.status.isTerminal) PopupMenuItem(value: 'cancel', child: Text(strings.cancel)),
                    PopupMenuItem(value: 'delete', child: Text(strings.delete)),
                  ],
                ),
              ],
            ),
            const SizedBox(height: 12),
            if (item.canPlayOffline) ...[
              FilledButton.icon(
                onPressed: () => context.go('/play/${item.mediaItemId}?offline=true'),
                icon: const Icon(Icons.play_arrow),
                label: Text(strings.playOffline),
              ),
              const SizedBox(height: 12),
            ],
            LinearProgressIndicator(value: item.status == DownloadItemStatus.failed ? null : progress),
            const SizedBox(height: 8),
            Row(
              children: [
                Text('${item.progressPercent.round()}%'),
                const Spacer(),
                Text(_bytesLabel(item.bytesPrepared, item.bytesExpected)),
              ],
            ),
          ],
        ),
      ),
    );
  }
}

String _bytesLabel(int prepared, int? expected) {
  String format(int value) {
    if (value >= 1024 * 1024 * 1024) return '${(value / (1024 * 1024 * 1024)).toStringAsFixed(1)} GB';
    if (value >= 1024 * 1024) return '${(value / (1024 * 1024)).toStringAsFixed(1)} MB';
    if (value >= 1024) return '${(value / 1024).toStringAsFixed(1)} KB';
    return '$value B';
  }

  if (expected == null || expected <= 0) return format(prepared);
  return '${format(prepared)} / ${format(expected)}';
}

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
import 'dart:convert';

import 'package:connectivity_plus/connectivity_plus.dart';
import 'package:crypto/crypto.dart';
import 'package:duskcue_mobile/models/content_models.dart';
import 'package:duskcue_mobile/models/download_models.dart';
import 'package:duskcue_mobile/models/realtime_models.dart';
import 'package:duskcue_mobile/services/service_providers.dart';
import 'package:duskcue_mobile/stores/session_store.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

class DownloadManagerState {
  const DownloadManagerState({
    this.scope,
    this.items = const [],
    this.settings = const DownloadManagerSettings(),
    this.loading = false,
    this.error,
  });

  final DownloadInventoryScope? scope;
  final List<DownloadItem> items;
  final DownloadManagerSettings settings;
  final bool loading;
  final String? error;

  bool get hasItems => items.isNotEmpty;

  int get activeCount {
    return items
        .where(
          (item) =>
              item.status == DownloadItemStatus.queued ||
              item.status == DownloadItemStatus.preparing ||
              item.status == DownloadItemStatus.downloading,
        )
        .length;
  }

  DownloadManagerState copyWith({
    DownloadInventoryScope? scope,
    bool clearScope = false,
    List<DownloadItem>? items,
    DownloadManagerSettings? settings,
    bool? loading,
    String? error,
    bool clearError = false,
  }) {
    return DownloadManagerState(
      scope: clearScope ? null : scope ?? this.scope,
      items: items ?? this.items,
      settings: settings ?? this.settings,
      loading: loading ?? this.loading,
      error: clearError ? null : error ?? this.error,
    );
  }
}

class DownloadManagerNotifier extends Notifier<DownloadManagerState> {
  static const _renewBeforeExpiry = Duration(days: 3);

  @override
  DownloadManagerState build() {
    return const DownloadManagerState();
  }

  Future<void> loadForCurrentSession() async {
    final scope = await _currentScope();
    if (scope == null) {
      state = const DownloadManagerState();
      return;
    }

    state = state.copyWith(scope: scope, loading: true, clearError: true);
    try {
      await ref.read(protectedDownloadStorageProvider).prepareScope(scope);
      final items = await _readItems(scope);
      final settings = await _readSettings(scope);
      state = state.copyWith(
        scope: scope,
        items: _sortItems(await _applyTransferConstraints(items, settings)),
        settings: settings,
        loading: false,
        clearError: true,
      );
      await _syncPendingChanges(scope);
    } catch (error) {
      state = state.copyWith(loading: false, error: error.toString());
    }
  }

  void clearForProfileChange() {
    state = const DownloadManagerState();
  }

  Future<void> queueDownload(MediaItemSummary item) async {
    final scope = await _requireScope();
    final settings = state.settings;
    state = state.copyWith(loading: true, clearError: true);
    try {
      final service = ref.read(downloadServiceProvider);
      final plan = await service.planDownload(
        mediaItemId: item.id,
        qualityMode: settings.defaultQualityMode,
      );
      if (!_fitsStorageCap(settings, plan.estimatedBytes)) {
        throw StateError('Download storage cap would be exceeded.');
      }
      final job = await service.createDownloadJob(
        item: item,
        qualityMode: settings.defaultQualityMode,
        plan: plan,
      );
      final next = _upsertItem(state.items, DownloadItem.queued(item, job));
      await _persistItems(scope, next);
      state = state.copyWith(
        scope: scope,
        items: _sortItems(await _applyTransferConstraints(next, settings)),
        loading: false,
        clearError: true,
      );
    } catch (error) {
      state = state.copyWith(loading: false, error: error.toString());
    }
  }

  Future<void> refreshJobs() async {
    final scope = state.scope ?? await _currentScope();
    if (scope == null) return;

    final service = ref.read(downloadServiceProvider);
    final updated = <DownloadItem>[];
    for (final item in state.items) {
      final jobId = item.jobId;
      if (jobId == null || jobId.isEmpty) {
        updated.add(item);
        continue;
      }
      try {
        final job = await service.getJob(jobId);
        updated.add(_mergeJob(item, job));
      } catch (_) {
        updated.add(item);
      }
    }
    final constrained = await _applyTransferConstraints(
      updated,
      state.settings,
    );
    final materialized = await _materializeReadyPackages(scope, constrained);
    await _persistItems(scope, materialized);
    state = state.copyWith(
      scope: scope,
      items: _sortItems(materialized),
      clearError: true,
    );
    await _syncPendingChanges(scope);
  }

  Future<void> cancel(DownloadItem item) async {
    final scope = await _requireScope();
    final jobId = item.jobId;
    if (jobId != null && jobId.isNotEmpty && !item.status.isTerminal) {
      try {
        await ref.read(downloadServiceProvider).cancelJob(jobId);
      } catch (_) {}
    }
    final next = _replaceItem(
      state.items,
      item.copyWith(
        status: DownloadItemStatus.cancelled,
        waitingReason: 'Cancelled',
        updatedAt: DateTime.now(),
      ),
    );
    await _persistItems(scope, next);
    state = state.copyWith(items: _sortItems(next), clearError: true);
  }

  Future<void> pause(DownloadItem item) async {
    final scope = await _requireScope();
    final next = _replaceItem(
      state.items,
      item.copyWith(
        status: DownloadItemStatus.paused,
        waitingReason: 'Paused',
        updatedAt: DateTime.now(),
      ),
    );
    await _persistItems(scope, next);
    state = state.copyWith(items: _sortItems(next), clearError: true);
  }

  Future<void> resume(DownloadItem item) async {
    final scope = await _requireScope();
    final resumed = item.copyWith(
      status: item.packageId == null
          ? DownloadItemStatus.ready
          : DownloadItemStatus.downloading,
      updatedAt: DateTime.now(),
      clearWaitingReason: true,
    );
    final next = await _applyTransferConstraints(
      _replaceItem(state.items, resumed),
      state.settings,
    );
    await _persistItems(scope, next);
    state = state.copyWith(items: _sortItems(next), clearError: true);
  }

  Future<void> delete(DownloadItem item) async {
    final scope = await _requireScope();
    final packageId = item.packageId;
    if (packageId != null && packageId.isNotEmpty) {
      try {
        await ref.read(downloadServiceProvider).deletePackage(packageId);
      } catch (_) {}
    }
    await ref.read(protectedDownloadStorageProvider).deletePackage(scope, item);
    final next = state.items
        .where((candidate) => candidate.id != item.id)
        .toList(growable: false);
    await _persistItems(scope, next);
    state = state.copyWith(items: _sortItems(next), clearError: true);
  }

  Future<void> deleteAll() async {
    final scope = await _requireScope();
    for (final item in state.items) {
      final packageId = item.packageId;
      if (packageId != null && packageId.isNotEmpty) {
        try {
          await ref.read(downloadServiceProvider).deletePackage(packageId);
        } catch (_) {}
      }
    }
    await ref.read(protectedDownloadStorageProvider).deleteScope(scope);
    await _saveItems(scope, const []);
    state = state.copyWith(items: const [], clearError: true);
  }

  Future<void> retry(DownloadItem item) async {
    await queueDownload(
      MediaItemSummary(
        id: item.mediaItemId,
        title: item.title,
        mediaType: item.mediaType,
      ),
    );
  }

  Future<void> updateSettings(DownloadManagerSettings settings) async {
    final scope = await _requireScope();
    await _saveSettings(scope, settings);
    final constrained = await _applyTransferConstraints(state.items, settings);
    await _persistItems(scope, constrained);
    state = state.copyWith(
      settings: settings,
      items: _sortItems(constrained),
      clearError: true,
    );
  }

  Future<void> handleRealtimeEvent(RealtimeEvent event) async {
    if (event.type != 'download_job_status') return;
    final scope = state.scope ?? await _currentScope();
    if (scope == null) return;
    final update = DownloadJobStatusEvent.fromJson(event.jsonData);
    if (update.deviceIdentifier != scope.deviceIdentifier) return;

    final current = state.items.firstWhere(
      (item) =>
          item.jobId == update.jobId || item.mediaItemId == update.mediaItemId,
      orElse: () => DownloadItem(
        mediaItemId: update.mediaItemId,
        title: 'Download',
        jobId: update.jobId,
        status: update.status,
        progressPercent: update.progressPercent,
        bytesExpected: update.bytesExpected,
        bytesPrepared: update.bytesPrepared,
        failureReason: update.failureReason,
        waitingReason: update.reason,
        updatedAt: DateTime.now(),
      ),
    );
    final next = await _applyTransferConstraints(
      _upsertItem(state.items, _mergeStatusEvent(current, update)),
      state.settings,
    );
    final materialized = await _materializeReadyPackages(scope, next);
    await _persistItems(scope, materialized);
    state = state.copyWith(
      scope: scope,
      items: _sortItems(materialized),
      clearError: true,
    );
  }

  Future<void> materializePackage(DownloadItem item) async {
    final scope = await _requireScope();
    final next = await _materializeReadyPackages(
      scope,
      _replaceItem(state.items, item),
    );
    await _persistItems(scope, next);
    state = state.copyWith(items: _sortItems(next), clearError: true);
  }

  Future<void> recordOfflinePlaybackProgress({
    required DownloadItem item,
    required int positionMs,
    required int durationMs,
    required String eventType,
    bool isPaused = false,
    bool isBuffering = false,
  }) async {
    final scope = await _requireScope();
    final packageId = item.packageId;
    if (packageId == null || packageId.isEmpty) return;
    final completed =
        eventType == 'completed' ||
        (durationMs > 0 && durationMs - positionMs <= 2000);
    final watched =
        completed || (durationMs > 0 && positionMs / durationMs >= 0.9);
    final protectedStorage = ref.read(protectedDownloadStorageProvider);
    await protectedStorage.appendOfflinePlaybackEvent(
      scope,
      OfflinePlaybackEvent(
        eventId: _offlineEventId(packageId, eventType, positionMs),
        packageId: packageId,
        eventType: eventType,
        positionMs: positionMs,
        occurredAt: DateTime.now(),
        details: {
          'media_item_id': item.mediaItemId,
          'duration_ms': durationMs,
          'completed': completed,
          'watched': watched,
          'is_paused': isPaused,
          'is_buffering': isBuffering,
        },
      ),
    );
    final pendingCount = await protectedStorage.pendingPlaybackEventCount(
      scope,
      packageId,
    );
    final nextItem = item.copyWith(
      localResumePositionMs: completed ? 0 : positionMs,
      localCompleted: completed,
      localWatched: watched,
      localPlaybackUpdatedAt: DateTime.now(),
      pendingPlaybackEventCount: pendingCount,
      updatedAt: DateTime.now(),
    );
    final next = _replaceItem(state.items, nextItem);
    await _persistItems(scope, next);
    state = state.copyWith(items: _sortItems(next), clearError: true);
    if (completed && state.settings.autoDeleteWatched) {
      await delete(nextItem);
      unawaited(_syncPendingChanges(scope));
      return;
    }
    unawaited(_syncPendingChanges(scope));
  }

  Future<DownloadInventoryScope> _requireScope() async {
    final scope = state.scope ?? await _currentScope();
    if (scope == null) {
      throw StateError('No authenticated download scope.');
    }
    if (state.scope?.key != scope.key) {
      await loadForCurrentSession();
    }
    return scope;
  }

  Future<DownloadInventoryScope?> _currentScope() async {
    final session = ref.read(sessionProvider);
    final server = session.server;
    final user = session.user;
    if (!session.isAuthenticated ||
        !session.isProfileScopeReady ||
        server == null ||
        user == null ||
        user.activeProfileId.isEmpty) {
      return null;
    }
    final identity = await ref.read(deviceIdentityProvider).current();
    return DownloadInventoryScope(
      serverOrigin: server.origin.toString(),
      userId: user.id,
      profileId: user.activeProfileId,
      deviceIdentifier: identity.deviceId,
    );
  }

  Future<List<DownloadItem>> _readItems(DownloadInventoryScope scope) async {
    final raw = await ref.read(secureStorageProvider).readDownloadInventory();
    final root = _decodeRoot(raw);
    final scoped = root[scope.key];
    if (scoped is! List) return const [];
    return scoped
        .whereType<Map>()
        .map((item) => DownloadItem.fromJson(Map<String, Object?>.from(item)))
        .where((item) => item.mediaItemId.isNotEmpty)
        .toList(growable: false);
  }

  Future<void> _saveItems(
    DownloadInventoryScope scope,
    List<DownloadItem> items,
  ) async {
    final storage = ref.read(secureStorageProvider);
    final root = _decodeRoot(await storage.readDownloadInventory());
    root[scope.key] = items
        .map((item) => item.toJson())
        .toList(growable: false);
    await storage.writeDownloadInventory(jsonEncode(root));
  }

  Future<void> _persistItems(
    DownloadInventoryScope scope,
    List<DownloadItem> items,
  ) async {
    await _saveItems(scope, items);
    final protectedStorage = ref.read(protectedDownloadStorageProvider);
    await protectedStorage.prepareScope(scope);
    for (final item in items) {
      await protectedStorage.writePackageMetadata(scope, item);
    }
  }

  Future<void> _syncPendingChanges(DownloadInventoryScope scope) async {
    try {
      final protectedStorage = ref.read(protectedDownloadStorageProvider);
      final events = await protectedStorage.readOfflinePlaybackEvents(scope);
      final packageStates = _packageStatesForSync(state.items, events);
      if (packageStates.isEmpty && events.isEmpty) return;
      final response = await ref
          .read(downloadServiceProvider)
          .syncDownloadState(
            packageStates: packageStates,
            playbackEvents: events,
          );
      await protectedStorage.removeOfflinePlaybackEvents(
        scope,
        response.acceptedPlaybackEventIds.toSet(),
      );
      final next = <DownloadItem>[];
      for (final item in state.items) {
        var updated = item;
        final packageId = item.packageId;
        if (packageId != null &&
            response.deletedPackageIds.contains(packageId)) {
          await protectedStorage.deletePackage(scope, item);
          continue;
        }
        if (packageId != null &&
            response.expiredPackageIds.contains(packageId)) {
          await protectedStorage.deletePackage(scope, item);
          updated = updated.copyWith(
            status: DownloadItemStatus.expired,
            waitingReason: 'Expired',
            bytesPrepared: 0,
            localFilesVerified: 0,
            pendingPlaybackEventCount: 0,
            clearLocalPlaybackPath: true,
            clearLocalManifestHashSha256: true,
            clearLocalPlaybackUpdatedAt: true,
            updatedAt: DateTime.now(),
          );
        } else if (packageId != null &&
            response.revokedPackageIds.contains(packageId)) {
          await protectedStorage.deletePackage(scope, item);
          updated = updated.copyWith(
            status: DownloadItemStatus.unavailable,
            waitingReason: 'Access changed',
            bytesPrepared: 0,
            localFilesVerified: 0,
            pendingPlaybackEventCount: 0,
            clearLocalPlaybackPath: true,
            clearLocalManifestHashSha256: true,
            clearLocalPlaybackUpdatedAt: true,
            updatedAt: DateTime.now(),
          );
        }
        if (packageId != null) {
          updated = updated.copyWith(
            pendingPlaybackEventCount: await protectedStorage
                .pendingPlaybackEventCount(scope, packageId),
          );
        }
        next.add(updated);
      }
      final renewed = await _renewExpiringPackages(next);
      await _persistItems(scope, renewed);
      state = state.copyWith(items: _sortItems(renewed), clearError: true);
    } catch (_) {}
  }

  Future<List<DownloadItem>> _renewExpiringPackages(
    List<DownloadItem> items,
  ) async {
    final now = DateTime.now();
    final service = ref.read(downloadServiceProvider);
    final renewed = <DownloadItem>[];
    for (final item in items) {
      final packageId = item.packageId;
      final expiresAt = item.expiresAt;
      if (packageId == null ||
          packageId.isEmpty ||
          expiresAt == null ||
          !expiresAt.isAfter(now) ||
          expiresAt.difference(now) > _renewBeforeExpiry ||
          (item.status != DownloadItemStatus.ready &&
              item.status != DownloadItemStatus.playableOffline)) {
        renewed.add(item);
        continue;
      }
      try {
        final renewal = await service.renewPackage(packageId);
        renewed.add(
          item.copyWith(
            expiresAt: renewal.expiresAt,
            waitingReason: item.status == DownloadItemStatus.playableOffline
                ? 'Playable offline'
                : item.waitingReason,
            updatedAt: DateTime.now(),
          ),
        );
      } catch (_) {
        renewed.add(item);
      }
    }
    return renewed;
  }

  List<Map<String, Object?>> _packageStatesForSync(
    List<DownloadItem> items,
    List<OfflinePlaybackEvent> events,
  ) {
    final eventsByPackage = <String, List<Map<String, Object?>>>{};
    for (final event in events) {
      eventsByPackage
          .putIfAbsent(event.packageId, () => <Map<String, Object?>>[])
          .add(event.toJson());
    }
    return items
        .where((item) => item.packageId != null && item.packageId!.isNotEmpty)
        .map(
          (item) => {
            'package_id': item.packageId,
            'local_status': _syncStatusFor(item),
            'bytes_downloaded': item.bytesPrepared,
            'files_verified': item.localFilesVerified,
            'local_manifest_hash_sha256': item.localManifestHashSha256,
            'local_resume_position_ms': item.localResumePositionMs,
            'pending_events':
                eventsByPackage[item.packageId] ??
                const <Map<String, Object?>>[],
          },
        )
        .toList(growable: false);
  }

  String _syncStatusFor(DownloadItem item) {
    return switch (item.status) {
      DownloadItemStatus.downloading => 'downloading',
      DownloadItemStatus.paused => 'paused',
      DownloadItemStatus.playableOffline =>
        item.pendingPlaybackEventCount > 0 ? 'sync_pending' : 'playable',
      DownloadItemStatus.failed => 'failed',
      DownloadItemStatus.expired => 'expired',
      DownloadItemStatus.unavailable => 'revoked',
      DownloadItemStatus.cancelled => 'deleted',
      _ => 'not_downloaded',
    };
  }

  String _offlineEventId(String packageId, String eventType, int positionMs) {
    return '$packageId:$eventType:$positionMs:${DateTime.now().microsecondsSinceEpoch}';
  }

  Future<List<DownloadItem>> _materializeReadyPackages(
    DownloadInventoryScope scope,
    List<DownloadItem> items,
  ) async {
    var next = items;
    for (final item in items) {
      if (item.status != DownloadItemStatus.ready ||
          item.packageId == null ||
          item.packageId!.isEmpty) {
        continue;
      }
      try {
        next = _replaceItem(
          next,
          item.copyWith(
            status: DownloadItemStatus.downloading,
            waitingReason: 'Saving package files',
            updatedAt: DateTime.now(),
          ),
        );
        state = state.copyWith(items: _sortItems(next), clearError: true);
        final playable = await _materializePackage(scope, item);
        next = _replaceItem(next, playable);
      } catch (error) {
        next = _replaceItem(
          next,
          item.copyWith(
            status: DownloadItemStatus.ready,
            failureReason: error.toString(),
            waitingReason: 'Ready on server; local files not saved',
            updatedAt: DateTime.now(),
          ),
        );
      }
    }
    return next;
  }

  Future<DownloadItem> _materializePackage(
    DownloadInventoryScope scope,
    DownloadItem item,
  ) async {
    final packageId = item.packageId;
    if (packageId == null || packageId.isEmpty) {
      throw StateError('Download package is missing.');
    }
    final downloadService = ref.read(downloadServiceProvider);
    final protectedStorage = ref.read(protectedDownloadStorageProvider);
    final manifest = await downloadService.getPackageManifest(packageId);
    final transfer = await downloadService.createTransferUrls(
      packageId: packageId,
      filePaths: manifest.files
          .map((file) => file.relativePath)
          .toList(growable: false),
    );
    final transferByPath = {
      for (final file in transfer.files) file.relativePath: file,
    };
    var bytesWritten = 0;
    var filesVerified = 0;
    for (final file in manifest.files) {
      final transferFile = transferByPath[file.relativePath];
      if (transferFile == null) {
        throw StateError('Missing transfer URL for ${file.relativePath}.');
      }
      final bytes = await downloadService.downloadPackageFile(transferFile.url);
      final checksum = sha256.convert(bytes).toString();
      if (checksum.toLowerCase() != file.checksumSha256.toLowerCase()) {
        throw StateError('Checksum mismatch for ${file.relativePath}.');
      }
      await protectedStorage.writePackageFile(
        scope,
        item,
        relativePath: file.relativePath,
        bytes: bytes,
      );
      bytesWritten += bytes.length;
      filesVerified += 1;
    }
    await protectedStorage.writePackageManifest(scope, item, manifest);
    final playbackFile = manifest.primaryPlaybackFile;
    if (playbackFile == null) {
      throw StateError('Package has no playable offline file.');
    }
    final playbackPath = await protectedStorage.packageFilePath(
      scope,
      item,
      playbackFile.relativePath,
    );
    final pendingEvents = await protectedStorage.pendingPlaybackEventCount(
      scope,
      packageId,
    );
    return item.copyWith(
      status: DownloadItemStatus.playableOffline,
      progressPercent: 100,
      bytesExpected: manifest.totalBytes,
      bytesPrepared: bytesWritten,
      localFilesVerified: filesVerified,
      localPlaybackPath: playbackPath,
      localManifestHashSha256: manifest.packageHashSha256,
      pendingPlaybackEventCount: pendingEvents,
      expiresAt: manifest.expiresAt ?? item.expiresAt,
      waitingReason: 'Playable offline',
      localPlaybackUpdatedAt: DateTime.now(),
      updatedAt: DateTime.now(),
    );
  }

  Future<DownloadManagerSettings> _readSettings(
    DownloadInventoryScope scope,
  ) async {
    final raw = await ref.read(secureStorageProvider).readDownloadSettings();
    final root = _decodeRoot(raw);
    final value = root[scope.key];
    if (value is Map) {
      return DownloadManagerSettings.fromJson(Map<String, Object?>.from(value));
    }
    return const DownloadManagerSettings();
  }

  Future<void> _saveSettings(
    DownloadInventoryScope scope,
    DownloadManagerSettings settings,
  ) async {
    final storage = ref.read(secureStorageProvider);
    final root = _decodeRoot(await storage.readDownloadSettings());
    root[scope.key] = settings.toJson();
    await storage.writeDownloadSettings(jsonEncode(root));
  }

  Future<List<DownloadItem>> _applyTransferConstraints(
    List<DownloadItem> items,
    DownloadManagerSettings settings,
  ) async {
    final connectivity = await ref.read(connectivityServiceProvider).current();
    final onCellular = connectivity.contains(ConnectivityResult.mobile);
    final waitingForWifi =
        settings.wifiOnly && !settings.allowCellular && onCellular;
    final overStorageCap = _storageCapExceeded(items, settings);
    return items
        .map((item) {
          if (waitingForWifi &&
              (item.status == DownloadItemStatus.ready ||
                  item.status == DownloadItemStatus.downloading)) {
            return item.copyWith(
              status: DownloadItemStatus.paused,
              waitingReason: 'Waiting for Wi-Fi',
              updatedAt: DateTime.now(),
            );
          }
          if (overStorageCap &&
              (item.status == DownloadItemStatus.queued ||
                  item.status == DownloadItemStatus.ready ||
                  item.status == DownloadItemStatus.downloading)) {
            return item.copyWith(
              status: DownloadItemStatus.paused,
              waitingReason: 'Storage cap reached',
              updatedAt: DateTime.now(),
            );
          }
          return item;
        })
        .toList(growable: false);
  }

  bool _fitsStorageCap(DownloadManagerSettings settings, int estimatedBytes) {
    final cap = settings.storageCapBytes;
    if (cap == null || cap <= 0) return true;
    return _currentStorageBytes(state.items) + estimatedBytes <= cap;
  }

  bool _storageCapExceeded(
    List<DownloadItem> items,
    DownloadManagerSettings settings,
  ) {
    final cap = settings.storageCapBytes;
    if (cap == null || cap <= 0) return false;
    return _currentStorageBytes(items) > cap;
  }

  int _currentStorageBytes(List<DownloadItem> items) {
    return items.fold<int>(0, (total, item) {
      if (item.status == DownloadItemStatus.cancelled ||
          item.status == DownloadItemStatus.expired ||
          item.status == DownloadItemStatus.unavailable) {
        return total;
      }
      return total + (item.bytesExpected ?? item.bytesPrepared);
    });
  }

  Map<String, Object?> _decodeRoot(String? raw) {
    if (raw == null || raw.isEmpty) return <String, Object?>{};
    try {
      final decoded = jsonDecode(raw);
      if (decoded is Map) return Map<String, Object?>.from(decoded);
    } catch (_) {}
    return <String, Object?>{};
  }

  List<DownloadItem> _upsertItem(List<DownloadItem> items, DownloadItem item) {
    var replaced = false;
    final next = items
        .map((candidate) {
          if (candidate.id == item.id ||
              candidate.jobId == item.jobId ||
              candidate.mediaItemId == item.mediaItemId) {
            replaced = true;
            return item;
          }
          return candidate;
        })
        .toList(growable: true);
    if (!replaced) next.add(item);
    return next;
  }

  DownloadItem _mergeJob(DownloadItem item, DownloadJob job) {
    if (item.canPlayOffline && job.status == DownloadItemStatus.ready) {
      return item.copyWith(
        jobId: job.id,
        mediaFileId: job.mediaFileId,
        qualityMode: job.qualityMode,
        bytesExpected: job.bytesExpected,
        expiresAt: job.expiresAt,
        updatedAt: DateTime.now(),
      );
    }
    return item.applyJob(job);
  }

  DownloadItem _mergeStatusEvent(
    DownloadItem item,
    DownloadJobStatusEvent event,
  ) {
    if (item.canPlayOffline && event.status == DownloadItemStatus.ready) {
      return item.copyWith(
        jobId: event.jobId,
        packageId: event.packageId,
        mediaFileId: event.mediaFileId,
        bytesExpected: event.bytesExpected,
        waitingReason: 'Playable offline',
        updatedAt: DateTime.now(),
      );
    }
    return item.applyStatusEvent(event);
  }

  List<DownloadItem> _replaceItem(List<DownloadItem> items, DownloadItem item) {
    return items
        .map((candidate) => candidate.id == item.id ? item : candidate)
        .toList(growable: false);
  }

  List<DownloadItem> _sortItems(List<DownloadItem> items) {
    final next = [...items];
    next.sort((a, b) => b.updatedAt.compareTo(a.updatedAt));
    return next;
  }
}

final downloadManagerProvider =
    NotifierProvider<DownloadManagerNotifier, DownloadManagerState>(
      DownloadManagerNotifier.new,
    );

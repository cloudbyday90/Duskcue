import 'dart:convert';
import 'dart:io';

import 'package:duskcue_mobile/models/download_models.dart';
import 'package:flutter/services.dart';

class ProtectedDownloadLocation {
  const ProtectedDownloadLocation({
    required this.path,
    required this.platform,
    required this.backupExcluded,
    required this.protection,
  });

  final String path;
  final String platform;
  final bool backupExcluded;
  final String protection;

  static ProtectedDownloadLocation fromJson(Map<Object?, Object?> json) {
    return ProtectedDownloadLocation(
      path: json['path']?.toString() ?? '',
      platform: json['platform']?.toString() ?? '',
      backupExcluded: json['backup_excluded'] == true,
      protection: json['protection']?.toString() ?? 'app_private',
    );
  }
}

class ProtectedDownloadStorageService {
  ProtectedDownloadStorageService({MethodChannel? channel})
      : _channel = channel ?? const MethodChannel('duskcue/mobile_storage');

  final MethodChannel _channel;

  Future<ProtectedDownloadLocation> prepareScope(DownloadInventoryScope scope) async {
    final result = await _channel.invokeMapMethod<Object?, Object?>(
      'prepareDownloadScope',
      {'scope_key': scope.key},
    );
    final location = ProtectedDownloadLocation.fromJson(result ?? const {});
    await _writeJson(
      File('${location.path}${Platform.pathSeparator}scope.json'),
      scope.toJson(),
    );
    final syncQueue = File('${location.path}${Platform.pathSeparator}sync_queue.json');
    if (!await syncQueue.exists()) {
      await _writeJson(syncQueue, <Object?>[]);
    }
    return location;
  }

  Future<ProtectedDownloadLocation> preparePackage(
    DownloadInventoryScope scope,
    DownloadItem item,
  ) async {
    final packageKey = _packageKey(item);
    final result = await _channel.invokeMapMethod<Object?, Object?>(
      'prepareDownloadPackage',
      {
        'scope_key': scope.key,
        'package_key': packageKey,
      },
    );
    return ProtectedDownloadLocation.fromJson(result ?? const {});
  }

  Future<void> writePackageMetadata(
    DownloadInventoryScope scope,
    DownloadItem item, {
    ProtectedDownloadLocation? location,
    Map<String, Object?> extra = const {},
  }) async {
    final packageLocation = location ?? await preparePackage(scope, item);
    await _writeJson(
      File('${packageLocation.path}${Platform.pathSeparator}metadata.json'),
      _sanitize({
        'scope': scope.toJson(),
        'download': item.toJson(),
        ...extra,
      }),
    );
  }

  Future<void> writePackageManifest(
    DownloadInventoryScope scope,
    DownloadItem item,
    DownloadPackageManifest manifest,
  ) async {
    final location = await preparePackage(scope, item);
    await _writeJson(
      File('${location.path}${Platform.pathSeparator}package_manifest.json'),
      _sanitize(manifest.toJson()),
    );
  }

  Future<DownloadPackageManifest?> readPackageManifest(
    DownloadInventoryScope scope,
    DownloadItem item,
  ) async {
    final location = await preparePackage(scope, item);
    final file = File('${location.path}${Platform.pathSeparator}package_manifest.json');
    if (!await file.exists()) return null;
    try {
      final decoded = jsonDecode(await file.readAsString());
      if (decoded is Map) {
        return DownloadPackageManifest.fromJson(Map<String, Object?>.from(decoded));
      }
    } catch (_) {}
    return null;
  }

  Future<String> writePackageFile(
    DownloadInventoryScope scope,
    DownloadItem item, {
    required String relativePath,
    required List<int> bytes,
  }) async {
    final location = await preparePackage(scope, item);
    final file = File(_packageFilePath(location.path, relativePath));
    await file.parent.create(recursive: true);
    await file.writeAsBytes(bytes, flush: true);
    return file.path;
  }

  Future<String> packageFilePath(
    DownloadInventoryScope scope,
    DownloadItem item,
    String relativePath,
  ) async {
    final location = await preparePackage(scope, item);
    return _packageFilePath(location.path, relativePath);
  }

  Future<void> appendOfflinePlaybackEvent(
    DownloadInventoryScope scope,
    OfflinePlaybackEvent event,
  ) async {
    final location = await prepareScope(scope);
    final file = File('${location.path}${Platform.pathSeparator}sync_queue.json');
    final events = await _readJsonList(file);
    events.add(event.toJson());
    await _writeJson(file, events);
  }

  Future<int> pendingPlaybackEventCount(
    DownloadInventoryScope scope,
    String packageId,
  ) async {
    final location = await prepareScope(scope);
    final file = File('${location.path}${Platform.pathSeparator}sync_queue.json');
    final events = await _readJsonList(file);
    return events
        .whereType<Map>()
        .where((event) => event['package_id']?.toString() == packageId)
        .length;
  }

  Future<void> deletePackage(DownloadInventoryScope scope, DownloadItem item) async {
    await _channel.invokeMethod<void>(
      'deleteDownloadPackage',
      {
        'scope_key': scope.key,
        'package_key': _packageKey(item),
      },
    );
  }

  Future<void> deleteScope(DownloadInventoryScope scope) async {
    await _channel.invokeMethod<void>(
      'deleteDownloadScope',
      {'scope_key': scope.key},
    );
  }

  Future<void> deleteAllProtectedDownloads() async {
    await _channel.invokeMethod<void>('deleteAllDownloads');
  }

  Future<void> _writeJson(File file, Object? value) async {
    await file.parent.create(recursive: true);
    await file.writeAsString(jsonEncode(value), flush: true);
  }

  Future<List<Object?>> _readJsonList(File file) async {
    if (!await file.exists()) return <Object?>[];
    try {
      final decoded = jsonDecode(await file.readAsString());
      if (decoded is List) return decoded.cast<Object?>().toList(growable: true);
    } catch (_) {}
    return <Object?>[];
  }

  String _packageFilePath(String packageRoot, String relativePath) {
    final parts = relativePath.split('/');
    if (relativePath.isEmpty || parts.any(_unsafePathPart) || relativePath.contains('\\')) {
      throw ArgumentError.value(relativePath, 'relativePath', 'Package path must stay inside the package root.');
    }
    return ([packageRoot, ...parts]).join(Platform.pathSeparator);
  }

  bool _unsafePathPart(String part) {
    return part.isEmpty || part == '.' || part == '..' || part.contains(':');
  }

  String _packageKey(DownloadItem item) {
    return item.packageId ?? item.jobId ?? item.mediaItemId;
  }

  Object? _sanitize(Object? value) {
    if (value is Map) {
      final next = <String, Object?>{};
      for (final entry in value.entries) {
        final key = entry.key.toString();
        if (_sensitiveKey(key)) continue;
        next[key] = _sanitize(entry.value);
      }
      return next;
    }
    if (value is List) {
      return value.map(_sanitize).toList(growable: false);
    }
    return value;
  }

  bool _sensitiveKey(String key) {
    final normalized = key.toLowerCase().replaceAll('-', '_');
    return normalized == 'authorization' ||
        normalized == 'bearer' ||
        normalized == 'token' ||
        normalized.endsWith('_token') ||
        normalized == 'access_token' ||
        normalized == 'refresh_token' ||
        normalized == 'session_token' ||
        normalized == 'package_key' ||
        normalized == 'signed_url' ||
        normalized == 'stream_url' ||
        normalized == 'transfer_url';
  }
}

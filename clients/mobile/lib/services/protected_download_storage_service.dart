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

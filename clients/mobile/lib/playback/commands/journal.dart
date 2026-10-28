// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import 'dart:convert';
import 'package:flutter_secure_storage/flutter_secure_storage.dart';
import 'models.dart';
import 'retained.dart';
import 'wire.dart';

abstract interface class JournalStorage {
  Future<String?> read();
  Future<void> write(String value);
  Future<void> clear();
}

class SecureJournalStorage implements JournalStorage {
  static const _storage = FlutterSecureStorage(
    aOptions: AndroidOptions(
      storageNamespace: 'duskcue.vod.journal.v1',
      preferencesKeyPrefix: 'duskcue_vod_journal_v1',
      resetOnError: false,
      migrateOnAlgorithmChange: false,
      migrateWithBackup: false,
    ),
  );
  static const _key = 'record';
  @override
  Future<String?> read() => _storage.read(key: _key);
  @override
  Future<void> write(String value) => _storage.write(key: _key, value: value);
  @override
  Future<void> clear() => _storage.delete(key: _key);
}

class PlaybackJournal {
  PlaybackJournal(this.storage);
  final JournalStorage storage;
  final operations = RetainedOperations(1);
  Future<void>? _clear;
  Future<VodRecord?> read() => operations.run(() async {
    try {
      final raw = await storage.read();
      return raw == null
          ? null
          : VodRecord.parse(decodeWire(utf8.encode(raw), limit: 4096));
    } on VodFailure {
      rethrow;
    } catch (_) {
      throw const VodFailure('storage_unavailable');
    }
  });
  Future<void> write(VodRecord record) {
    require(_clear == null, 'storage_busy');
    final raw = jsonEncode(record.toJson());
    require(utf8.encode(raw).length <= 4096);
    VodRecord.parse(record.toJson());
    return operations.run(() async {
      try {
        await storage.write(raw);
        require(await storage.read() == raw, 'storage_unavailable');
      } catch (_) {
        throw const VodFailure('storage_unavailable');
      }
    });
  }

  Future<void> clear() {
    if (_clear != null) return _clear!;
    final pending = () async {
      await operations.drain();
      await operations.run(() async {
        try {
          await storage.clear();
          require(await storage.read() == null, 'storage_unavailable');
        } catch (_) {
          throw const VodFailure('storage_unavailable');
        }
      });
    }();
    _clear = pending;
    return pending.whenComplete(() {
      if (identical(_clear, pending)) _clear = null;
    });
  }
}

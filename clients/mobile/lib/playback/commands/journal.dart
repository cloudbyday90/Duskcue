// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import 'dart:convert';
import 'package:flutter/foundation.dart';
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

class PlaybackJournal extends ChangeNotifier {
  PlaybackJournal(this.storage);
  final JournalStorage storage;
  final operations = RetainedOperations(1);
  Future<void>? _clear, _clearActual;
  bool _clearFailed = false, closed = false;
  int _epoch = 0;
  int claim() {
    require(!closed, 'storage_unavailable');
    return ++_epoch;
  }

  bool current(int claim) => !closed && claim == _epoch;
  bool get blocked =>
      closed || _clearActual != null || _clearFailed || operations.count > 0;
  bool get needsClear => _clearActual != null || _clearFailed;
  void _check(int claim) => require(current(claim), 'scope_changed');
  void _notify() {
    if (!closed) notifyListeners();
  }

  Future<VodRecord?> read({int? claim}) async {
    final token = claim ?? _epoch;
    _check(token);
    require(!needsClear, 'storage_busy');
    final work = operations.retain(() async {
      try {
        final raw = await storage.read();
        _check(token);
        return raw == null
            ? null
            : VodRecord.parse(decodeWire(utf8.encode(raw), limit: 4096));
      } on VodFailure {
        rethrow;
      } catch (_) {
        throw const VodFailure('storage_unavailable');
      }
    });
    work.completion.then((_) => _notify());
    return work.result;
  }

  Future<void> write(VodRecord record, {int? claim}) async {
    final token = claim ?? _epoch;
    _check(token);
    require(!needsClear, 'storage_busy');
    final raw = jsonEncode(record.toJson());
    require(utf8.encode(raw).length <= 4096);
    VodRecord.parse(record.toJson());
    final work = operations.retain(() async {
      try {
        _check(token);
        await storage.write(raw);
        _check(token);
        require(await storage.read() == raw, 'storage_unavailable');
        _check(token);
      } on VodFailure {
        rethrow;
      } catch (_) {
        throw const VodFailure('storage_unavailable');
      }
    });
    work.completion.then((_) => _notify());
    return work.result;
  }

  Future<void> clear({int? claim}) {
    if (!current(claim ?? _epoch)) {
      return Future.error(const VodFailure('scope_changed'));
    }
    if (_clear != null) return _clear!;
    final pending = () async {
      await operations.drain();
      try {
        await storage.clear();
        require(await storage.read() == null, 'storage_unavailable');
      } catch (_) {
        throw const VodFailure('storage_unavailable');
      }
    }();
    late final Future<void> settled;
    settled = pending.then(
      (_) {
        if (identical(_clearActual, settled)) {
          _clearActual = null;
          _clear = null;
          _clearFailed = false;
          _notify();
        }
      },
      onError: (Object problem, StackTrace stack) {
        if (identical(_clearActual, settled)) {
          _clearActual = null;
          _clear = null;
          _clearFailed = true;
          _notify();
        }
        Error.throwWithStackTrace(problem, stack);
      },
    );
    _clearActual = settled;
    final view = settled.timeout(
      const Duration(seconds: 5),
      onTimeout: () => throw const VodFailure('storage_unavailable'),
    );
    _clear = view;
    _notify();
    return view;
  }

  Future<void> drain() async {
    await operations.drain();
    await _clearActual;
    require(!_clearFailed, 'storage_unavailable');
  }

  Future<void> close() async {
    closed = true;
    operations.closed = true;
    await drain();
  }
}

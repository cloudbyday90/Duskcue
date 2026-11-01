// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import 'package:flutter/foundation.dart';
import 'commands/journal.dart';
import 'commands/retained.dart';
import 'commands/wire.dart';
import 'native_ownership.dart';

class PlaybackLifetime extends ChangeNotifier {
  PlaybackLifetime(
    this.journal, {
    RetainedOperations? regular,
    RetainedOperations? cleanup,
  }) : regular = regular ?? RetainedOperations(3),
       cleanup = cleanup ?? RetainedOperations(2) {
    journal.addListener(_changed);
    native.addListener(_changed);
  }
  final PlaybackJournal journal;
  final RetainedOperations regular, cleanup;
  final native = NativeOwnership();
  bool closed = false;
  Future<void>? _closing;
  void _changed() {
    if (!closed) notifyListeners();
  }

  void requirePreparation() {
    require(!closed, 'scope_changed');
    require(!native.held, 'native_cleanup_pending');
    require(!journal.blocked, 'storage_busy');
  }

  Future<void> close() => _closing ??= () async {
    closed = true;
    regular.closed = true;
    cleanup.closed = true;
    journal.removeListener(_changed);
    native.removeListener(_changed);
    await Future.wait([
      journal.close(),
      native.close(),
      regular.drain(),
      cleanup.drain(),
    ]);
  }();
}

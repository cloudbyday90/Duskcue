// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import 'dart:async';
import 'package:flutter/foundation.dart';
import 'commands/wire.dart';

class NativeClaim {
  NativeClaim._(this.owner, this.retire);
  final NativeOwnership owner;
  final Future<void> Function() retire;
  final _released = Completer<void>();
  Future<void> get completion => _released.future;
  void release() {
    owner._release(this);
  }
}

class NativeOwnership extends ChangeNotifier {
  NativeClaim? _held;
  bool closed = false;
  bool get held => _held != null;
  void _release(NativeClaim claim) {
    if (!identical(_held, claim)) return;
    _held = null;
    claim._released.complete();
    notifyListeners();
  }

  NativeClaim acquire(Future<void> Function() retire) {
    require(!closed && !held, 'native_cleanup_pending');
    final claim = NativeClaim._(this, retire);
    _held = claim;
    notifyListeners();
    return claim;
  }

  Future<void> drain() async {
    await _held?.completion;
  }

  Future<void> close() async {
    closed = true;
    final current = _held;
    await current?.retire();
    await current?.completion;
  }
}

// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

part of 'controller.dart';

extension PlaybackScopeOwnership on PlaybackCommands {
  void _retireJournal({required bool discard}) {
    if (!journal.current(_journalClaim)) return;
    if (discard) {
      unawaited(journal.clear(claim: _journalClaim).catchError((Object _) {}));
    }
    _journalClaim = journal.claim();
  }

  void _invalidateScope() {
    if (_disposed) return;
    final current = record;
    final bound = transport.bound;
    final canClean =
        bound != null &&
        bound.identity.matches(transport.identity(), cleanup: true);
    final unsent = _pendingStart != null && !_dispatched;
    _epoch++;
    cancelWorkerObservation();
    reader.cancel();
    transport.cancel();
    _retry?.cancel();
    retryAt = 0;
    lease.stop();
    checkpoints.reset();
    admitted = null;
    receipt = null;
    deadline = null;
    resumePosition = 0;
    error = null;
    busy = false;
    monitoring = false;
    _retireJournal(discard: unsent || !canClean);
    _pendingStart = null;
    if (current != null && _dispatched && canClean) {
      cleanupOnly = true;
      state = 'cleanup';
    } else {
      record = null;
      cleanupOnly = false;
      state = 'scope_changed';
    }
    _emit();
  }

  Future<void> _checkLocalCleanup() async {
    require(canAct && record == null, 'command_busy');
    final token = _epoch;
    busy = true;
    try {
      if (journal.needsClear) await journal.clear(claim: _journalClaim);
      await lifetime.native.drain().timeout(
        const Duration(seconds: 5),
        onTimeout: () => throw const VodFailure('native_cleanup_pending'),
      );
      require(ownsEpoch(token), 'scope_changed');
      lifetime.requirePreparation();
      error = null;
      state = 'idle';
    } catch (failure) {
      if (ownsEpoch(token)) fail(failure);
    } finally {
      if (ownsEpoch(token)) {
        busy = false;
        _emit();
      }
    }
  }
}

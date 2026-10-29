// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

part of 'controller.dart';

extension PlaybackSettlement on PlaybackCommands {
  Future<void> close() async {
    final current = record;
    if (current == null) {
      _epoch++;
      transport.cancel();
      lease.stop();
      busy = false;
      monitoring = false;
      if (error == null) state = 'idle';
      return;
    }
    final token = ++_epoch;
    transport.cancel();
    lease.stop();
    monitoring = false;
    busy = true;
    try {
      if (deliverable && !checkpoints.uncertain) {
        try {
          await checkpoints
              .flush(force: true)
              .timeout(const Duration(seconds: 5));
        } catch (_) {
          progressUncertain = true;
        }
      }
      cleanupOnly = true;
      receipt = null;
      state = 'withdrawing';
      _emit();
      try {
        await _save(current.copy(phase: 'cleanup'));
      } catch (_) {
        require(_owns(token, current.start), 'scope_changed');
        progressUncertain = true;
      }
      final context = await transport.context(forCleanup: true);
      require(
        context.scope.matches(current.scope, cleanup: true),
        'scope_changed',
      );
      final reply = await _call(
        'POST',
        '/withdraw',
        body: {'protocol': vodProtocol},
        cleanup: true,
      );
      require(_owns(token, current.start), 'scope_changed');
      final result = VodSettlement.parse(
        reply.body,
        current.start,
        current.scope.origin,
      );
      state = result.status;
      if (result.settled) await _settled(token);
    } catch (failure) {
      if (_owns(token, current.start)) fail(failure);
    } finally {
      if (token == _epoch) {
        busy = false;
        _emit();
      }
    }
    if (_owns(token, current.start) && error == null) await check();
  }

  Future<void> _settled(int token) async {
    await journal.clear();
    require(token == _epoch, 'scope_changed');
    record = null;
    receipt = null;
    admitted = null;
    deadline = null;
    cleanupOnly = false;
    lease.stop();
    checkpoints.reset();
    state = 'settled';
    error = null;
    _emit();
  }
}

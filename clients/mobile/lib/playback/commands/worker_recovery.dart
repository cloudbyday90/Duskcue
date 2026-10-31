// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

part of 'controller.dart';

extension PlaybackWorkerRecovery on PlaybackCommands {
  void _requireWorkerPlay() {
    final notify = _workerUnavailable;
    _workerUnavailable = null;
    notify?.call();
  }

  void cancelWorkerObservation() {
    workerObserving = false;
    workerObserver.cancel();
  }

  Future<void> observeWorkerFailure(
    String output, {
    required bool Function() active,
    required Future<void> Function() detach,
    required void Function() unavailable,
  }) async {
    require(
      canAct &&
          !workerObserver.pending &&
          !cleanupOnly &&
          record?.operation == null &&
          record?.output == output &&
          admitted?.output == output,
      'command_busy',
    );
    final captured = record!, token = _epoch, owner = Object();
    bool boundOwner() =>
        _owns(token, captured.start) &&
        identical(_workerOwner, owner) &&
        record?.output == output &&
        !cleanupOnly;
    bool owns() => boundOwner() && transport.current(captured.scope);
    bool current() =>
        owns() &&
        workerObserving &&
        state == 'checking_worker' &&
        !busy &&
        !monitoring &&
        record?.operation == null &&
        deadline != null &&
        transport.clock.remaining(deadline!) > 0 &&
        active();
    void refused(Object failure) {
      if (boundOwner() && state == 'checking_worker') {
        _requireWorkerPlay();
        fail(
          transport.current(captured.scope)
              ? failure
              : const VodFailure('scope_changed'),
        );
      }
    }

    _workerOwner = owner;
    _workerUnavailable = unavailable;
    workerObserving = true;
    error = null;
    state = 'checking_worker';
    _emit();
    try {
      final eligible = await workerObserver.observe(
        current: current,
        prepare: () async {
          try {
            await detach().timeout(const Duration(seconds: 5));
          } catch (_) {
            throw const VodFailure('native_cleanup_pending');
          }
        },
        read: (timeout) => transport
            .read(
              commandPath(captured.start, '/configuration'),
              captured.scope,
              timeout: timeout,
            )
            .map((reply) {
              final config = VodConfiguration.parse(reply, captured);
              require(
                config.output == output &&
                    const {'ready', 'unavailable'}.contains(config.state) &&
                    config.value['pending_operation'] == null,
                'worker_status_required',
              );
              return config.workerRecovery;
            }),
      );
      if (!boundOwner()) return;
      if (!owns()) {
        refused(const VodFailure('scope_changed'));
        return;
      }
      if (!eligible || !current()) {
        refused(const VodFailure('worker_status_required'));
        return;
      }
      workerObserving = false;
      await change({'kind': 'recover'});
    } catch (failure) {
      refused(failure);
    } finally {
      if (identical(_workerOwner, owner)) {
        _workerOwner = null;
        _workerUnavailable = null;
        workerObserving = false;
        _emit();
      }
    }
  }
}

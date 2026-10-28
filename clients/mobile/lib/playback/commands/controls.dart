// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

part of 'controller.dart';

extension PlaybackControls on PlaybackCommands {
  Future<void> change(Wire action) async {
    action = decodeWire(utf8.encode(jsonEncode(action)));
    require(
      canAct && !cleanupOnly && record != null && record!.operation == null,
      'command_busy',
    );
    busy = true;
    error = null;
    final token = _epoch;
    try {
      if (transport.clock.stale) {
        final context = await transport.context();
        require(context.scope.matches(record!.scope), 'scope_changed');
      }
      final config = await VodConfiguration.read(transport, record!);
      require(config.output == record!.output);
      if (action['kind'] == 'recover') {
        require(config.workerRecovery, 'recovery_ineligible');
        await checkpoints.reconcile();
        final sample = checkpoints.confirmed;
        require(
          sample != null &&
              sample['output_revision'] == config.output &&
              integer(sample['sequence']) > 0,
          'checkpoint_required',
        );
        action = {
          'kind': 'recover',
          'checkpoint_sequence': sample!['sequence'],
          'evidence': {'kind': 'worker'},
        };
      }
      require(token == _epoch, 'scope_changed');
      final next = record!.copy(
        operation: transport.clock.id(),
        phase: 'uncertain',
      );
      await _save(next);
      state = 'changing';
      receipt = null;
      checkpoints.discard();
      _emit();
      require(_owns(token, next.start), 'scope_changed');
      final reply = await _call(
        'POST',
        '/configuration',
        etag: config.etag,
        body: {
          'protocol': vodProtocol,
          'operation_id': next.operation,
          'expected_output': config.output,
          'action': action,
        },
      );
      require(_owns(token, next.start), 'scope_changed');
      operation(reply, next);
    } catch (failure) {
      if (token == _epoch) {
        if (failure is VodFailure &&
            (failure.validation ||
                failure.status == 412 ||
                failure.status == 428)) {
          refusedOperation = record?.operation;
        }
        fail(failure);
      }
    } finally {
      if (token == _epoch) {
        busy = false;
        _emit();
      }
    }
    if (token == _epoch && error == null) await check();
  }

  Future<bool> recoverWorker() async {
    if (!canAct || cleanupOnly || record == null || record!.operation != null) {
      return false;
    }
    final config = await VodConfiguration.read(transport, record!);
    if (!config.workerRecovery || config.output != record!.output) return false;
    await change({'kind': 'recover'});
    return true;
  }
}

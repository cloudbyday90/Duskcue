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
    final captured = record!;
    try {
      if (transport.clock.stale) {
        final context = await transport.context();
        require(
          _owns(token, captured.start) && context.scope.matches(captured.scope),
          'scope_changed',
        );
      }
      final config = await VodConfiguration.read(transport, captured);
      require(
        _owns(token, captured.start) && transport.current(captured.scope),
        'scope_changed',
      );
      require(config.output == captured.output);
      if (action['kind'] == 'recover') {
        require(config.workerRecovery, 'recovery_ineligible');
        await checkpoints.reconcile();
        require(_owns(token, captured.start), 'scope_changed');
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
      if (retireLocal != null) {
        state = 'changing';
        _emit();
        await retireLocal!().timeout(
          const Duration(seconds: 5),
          onTimeout: () => throw const VodFailure('native_cleanup_pending'),
        );
        require(_owns(token, captured.start), 'scope_changed');
      }
      require(!lifetime.native.held, 'native_cleanup_pending');
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
}

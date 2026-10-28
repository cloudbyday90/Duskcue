// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import 'dart:async';
import 'dart:convert';
import 'package:flutter/foundation.dart';
import 'checkpoints.dart';
import 'configuration.dart';
import 'journal.dart';
import 'lease.dart';
import 'models.dart';
import 'scope.dart';
import 'status_reader.dart';
import 'transport.dart';
import 'wire.dart';

part 'controls.dart';
part 'settlement.dart';

class PlaybackCommands extends ChangeNotifier {
  PlaybackCommands(
    this.transport,
    this.journal, {
    Future<void> Function(Duration)? wait,
  }) {
    reader = PlaybackStatusReader(
      transport,
      _owns,
      () => transport.clock.now,
      wait ?? Future<void>.delayed,
    );
    lease = PlaybackLease(
      remaining: () =>
          deadline == null ? -1 : transport.clock.remaining(deadline!),
      renew: _renew,
      now: () => transport.clock.now,
      uncertain: () => fail(const VodFailure('lease_uncertain')),
    );
    checkpoints = PlaybackCheckpoints(
      send: (body) async =>
          (await _call('POST', '/checkpoint', body: body)).body,
      read: () async => (await _call('GET', '/checkpoint')).body,
      current: (output) =>
          !cleanupOnly &&
          record?.output == output &&
          record != null &&
          transport.current(record!.scope),
      now: () => transport.clock.now,
      onUncertain: () {
        progressUncertain = true;
        _emit();
      },
    );
  }
  final PlaybackTransport transport;
  final PlaybackJournal journal;
  late final PlaybackStatusReader reader;
  late final PlaybackLease lease;
  late final PlaybackCheckpoints checkpoints;
  VodRecord? record;
  VodReceipt? receipt;
  VodReady? admitted;
  DateTime? deadline;
  String state = 'idle';
  VodFailure? error;
  bool busy = false,
      monitoring = false,
      cleanupOnly = false,
      progressUncertain = false;
  bool _disposed = false;
  int _epoch = 0, retryAt = 0;
  Timer? _retry;
  String? refusedOperation;
  String? _item, _file, _history;
  int resumePosition = 0;
  bool get deliverable {
    try {
      return !cleanupOnly &&
          record != null &&
          receipt?.status == 'ready' &&
          state == 'ready' &&
          transport.current(record!.scope) &&
          deadline != null &&
          transport.clock.remaining(deadline!) > 0;
    } catch (_) {
      return false;
    }
  }

  bool get canAct => !busy && !monitoring && transport.clock.now >= retryAt;
  bool _owns(int token, String start) =>
      !_disposed && _epoch == token && record?.start == start;
  void _emit() {
    if (!_disposed) notifyListeners();
  }

  void fail(Object problem) {
    error = problem is VodFailure
        ? problem
        : const VodFailure('command_uncertain');
    retryAt = transport.clock.now + error!.retryAfter.inMilliseconds;
    _retry?.cancel();
    if (error!.retryAfter > Duration.zero) {
      _retry = Timer(error!.retryAfter, _emit);
    }
    if (error!.validation && record == null) {
      state = 'refused';
    } else {
      state = cleanupOnly ? 'cleanup' : 'uncertain';
    }
    if (error!.code == 'lease_uncertain' || error!.code == 'scope_changed') {
      lease.stop();
    }
    _emit();
  }

  Future<VodReply> _call(
    String method,
    String suffix, {
    Wire? body,
    String? etag,
    bool cleanup = false,
  }) {
    final current = record;
    require(current != null, 'no_viewer');
    return transport.call(
      method,
      commandPath(current!.start, suffix),
      current.scope,
      body: body,
      etag: etag,
      forCleanup: cleanup,
    );
  }

  Future<void> _save(VodRecord next) async {
    final token = _epoch;
    await journal.write(next);
    require(!_disposed && token == _epoch, 'scope_changed');
    record = next;
  }

  Future<bool> open(Wire selection, {bool resume = false}) async {
    require(canAct && record == null, 'command_busy');
    admitted = null;
    selection = decodeWire(utf8.encode(jsonEncode(selection)));
    busy = true;
    error = null;
    state = 'checking';
    _emit();
    final token = _epoch;
    try {
      final restored = await journal.read();
      late final PlaybackContext context;
      try {
        context = await transport.context();
      } on VodFailure catch (failure) {
        if (restored == null &&
            (failure.status == 404 || failure.status == 405)) {
          return false;
        }
        rethrow;
      }
      require(
        context.identity.ready &&
            context.scope.profile == context.identity.profile,
        'scope_changed',
      );
      require(token == _epoch, 'scope_changed');
      if (restored != null) {
        if (!context.scope.matches(restored.scope, cleanup: true)) {
          await journal.clear();
          require(token == _epoch, 'scope_changed');
        } else {
          record = restored;
          cleanupOnly = true;
          state = 'cleanup';
          return true;
        }
      }
      resumePosition = 0;
      if (resume) {
        final item = entity(selection['media_item_id']);
        final watch = await transport.call(
          'GET',
          '/api/v1/items/$item/watch-data',
          context.scope,
        );
        require(
          watch.body['media_item_id'] == item &&
              watch.body['is_watched'] is bool,
        );
        final position = integer(watch.body['resume_position_ms']);
        resumePosition = watch.body['is_watched'] == true ? 0 : position;
      }
      VodReply prepared;
      try {
        prepared = await transport.call(
          'POST',
          '/api/v1/playback/prepare',
          context.scope,
          body: {'capability_protocol': 1, ...selection},
        );
      } on VodFailure catch (failure) {
        if (failure.status == 404 || failure.status == 405) return false;
        rethrow;
      }
      require(prepared.body['protocol'] == 1);
      if (!commandCapability(prepared.body['command_capability'])) return false;
      require(
        token == _epoch && transport.current(context.scope),
        'scope_changed',
      );
      final next = VodRecord(
        scope: context.scope,
        start: transport.clock.id(),
        phase: 'uncertain',
      );
      _item = entity(selection['media_item_id']);
      _file = selection['media_file_id'] == null
          ? null
          : entity(selection['media_file_id']);
      _history = null;
      await _save(next);
      state = 'starting';
      _emit();
      require(_owns(token, next.start), 'scope_changed');
      final reply = await transport.call(
        'POST',
        '/api/v1/playback-commands',
        next.scope,
        body: {
          'protocol': vodProtocol,
          'request_id': next.start,
          'selection': selection,
        },
      );
      require(_owns(token, next.start), 'scope_changed');
      require(reply.status == 202);
      resource(next.scope.origin, reply.location, commandPath(next.start));
      await _adopt(reply, token);
    } catch (failure) {
      if (token == _epoch) fail(failure);
    } finally {
      if (token == _epoch) {
        busy = false;
        _emit();
      }
    }
    if (record != null && state != 'uncertain' && !cleanupOnly) await check();
    return true;
  }

  Future<void> _adopt(VodReply reply, int token) async {
    final current = record!;
    final next = VodReceipt.parse(
      reply.body,
      current.start,
      current.scope.origin,
    );
    require(_owns(token, current.start), 'scope_changed');
    transport.clock.observe(next.observed, reply.began);
    deadline = next.deadline;
    if (deadline != null) {
      require(
        transport.clock.remaining(deadline!) > 0 &&
            transport.clock.remaining(deadline!) <= 60000,
        'lease_uncertain',
      );
    }
    if (next.ready != null) {
      require(
        next.ready!.item == _item &&
            (_file == null || next.ready!.file == _file) &&
            (_history == null || next.ready!.history == _history),
      );
      _file = next.ready!.file;
      _history = next.ready!.history;
      admitted = next.ready;
      if (current.output != next.ready!.output) checkpoints.reset();
      await _save(
        current.copy(
          output: next.ready!.output,
          phase: 'active',
          clearOperation: true,
        ),
      );
    }
    require(_owns(token, current.start), 'scope_changed');
    receipt = next;
    state = next.status;
    if (deadline != null &&
        const {
          'ready',
          'preparing',
          'changing',
          'unavailable',
        }.contains(state)) {
      if (!lease.running) lease.start();
    } else {
      lease.stop();
    }
    _emit();
  }

  Future<void> _renew() async {
    final current = record;
    require(current != null && !cleanupOnly, 'scope_changed');
    final token = _epoch;
    final reply = await _call('POST', '/renew', body: {});
    require(_owns(token, current!.start));
    final value = reply.body;
    require(
      value['protocol'] == vodProtocol &&
          value['request_id'] == current.start &&
          value['status'] == 'renewed',
    );
    resource(
      current.scope.origin,
      value['status_url'],
      commandPath(current.start),
    );
    transport.clock.observe(instant(value['observed_at']), reply.began);
    deadline = instant(value['lease_deadline']);
    require(
      transport.clock.remaining(deadline!) > 0 &&
          transport.clock.remaining(deadline!) <= 60000,
    );
  }

  Future<void> check({bool once = false}) async {
    require(canAct && record != null, 'command_busy');
    monitoring = true;
    error = null;
    _emit();
    final current = record!,
        token = _epoch,
        until = transport.clock.now + 60000;
    try {
      final context = await transport.context(forCleanup: cleanupOnly);
      require(
        context.scope.matches(current.scope, cleanup: cleanupOnly),
        'scope_changed',
      );
      do {
        final reply = await reader.read(
          current,
          token,
          until,
          suffix: cleanupOnly
              ? '/settlement'
              : current.operation == null
              ? ''
              : '/operations/${current.operation}',
          cleanup: cleanupOnly,
        );
        if (reply == null) break;
        if (cleanupOnly) {
          final result = VodSettlement.parse(
            reply.body,
            current.start,
            current.scope.origin,
          );
          state = result.status;
          if (result.settled) {
            await _settled(token);
            break;
          }
        } else if (current.operation != null) {
          final result = operation(reply, current);
          if (const {
                'completed',
                'failed',
                'cancelled',
              }.contains(result['status']) ||
              result['status'] == 'unknown' &&
                  refusedOperation == current.operation) {
            await _save(record!.copy(clearOperation: true));
            refusedOperation = null;
            break;
          }
        } else {
          await _adopt(reply, token);
          if (!const {
            'preparing',
            'changing',
            'stopping',
            'withdrawing',
          }.contains(state)) {
            break;
          }
        }
        _emit();
      } while (!once &&
          _owns(token, current.start) &&
          transport.clock.now < until);
    } catch (failure) {
      if (_owns(token, current.start)) fail(failure);
    } finally {
      if (token == _epoch) {
        monitoring = false;
        _emit();
      }
    }
    if (current.operation != null &&
        record != null &&
        record!.operation == null &&
        error == null) {
      await check(once: true);
    }
  }

  void observe(String output, int sourcePosition, String playbackState) {
    if (!deliverable) return;
    final changed = checkpoints.observe(output, sourcePosition, playbackState);
    unawaited(checkpoints.flush(force: changed).catchError((Object _) => null));
  }

  Future<void> close() => PlaybackSettlement(this).close();

  void invalidate() {
    final current = record;
    _epoch++;
    transport.cancel();
    _retry?.cancel();
    busy = false;
    monitoring = false;
    if (current == null) {
      state = 'scope_changed';
      _emit();
      return;
    }
    lease.stop();
    checkpoints.reset();
    admitted = null;
    receipt = null;
    deadline = null;
    busy = false;
    monitoring = false;
    cleanupOnly = true;
    state = 'cleanup';
    final bound = transport.bound;
    if (bound == null ||
        !bound.identity.matches(transport.identity(), cleanup: true)) {
      record = null;
      state = 'scope_changed';
      unawaited(journal.clear().catchError((Object _) {}));
    }
    _emit();
  }

  @override
  void dispose() {
    _disposed = true;
    _retry?.cancel();
    _epoch++;
    lease.stop();
    transport.cancel();
    unawaited(transport.dispose());
    super.dispose();
  }
}

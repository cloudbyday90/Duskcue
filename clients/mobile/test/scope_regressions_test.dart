// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import 'dart:async';
import 'dart:convert';
import 'package:duskcue_mobile/playback/commands/journal.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:duskcue_mobile/playback/commands/clock.dart';
import 'package:duskcue_mobile/playback/commands/controller.dart';
import 'package:duskcue_mobile/playback/commands/models.dart';
import 'package:duskcue_mobile/playback/commands/retained.dart';
import 'package:duskcue_mobile/playback/commands/scope.dart';
import 'package:duskcue_mobile/playback/commands/selection.dart';
import 'package:duskcue_mobile/playback/commands/status_reader.dart';
import 'package:duskcue_mobile/playback/commands/transport.dart';
import 'package:duskcue_mobile/playback/commands/wire.dart';
import 'playback_commands_test.dart' show CommandFixture, MemoryJournal;
import 'playback_native_test.dart' show until;

class HeldReadTransport extends PlaybackTransport {
  HeldReadTransport() : super(identity: () => null, clock: PlaybackClock());
  final source = Completer<VodReply>();
  int issued = 0;
  @override
  RetainedOperation<VodReply> read(
    String path,
    PlaybackScope scope, {
    Duration timeout = const Duration(seconds: 5),
    bool forCleanup = false,
  }) {
    issued++;
    return regular.retain(
      () => source.future,
      timeout: const Duration(milliseconds: 10),
    );
  }

  @override
  Future<VodReply> call(
    String method,
    String path,
    PlaybackScope scope, {
    Wire? body,
    String? etag,
    bool forCleanup = false,
    Duration timeout = const Duration(seconds: 5),
  }) => read(path, scope).result;
}

class RefusedClearStorage extends MemoryJournal {
  bool refuse = true;
  @override
  Future<void> clear() async {
    if (refuse) throw StateError('storage delegate failed');
    await super.clear();
  }
}

void main() {
  test(
    'failed clear retains the barrier and stale claims cannot erase a fresh receipt',
    () async {
      final f = CommandFixture();
      await f.open();
      try {
        await f.commands.open(androidSelection(f.item));
        final receipt = f.commands.record!;
        final storage = RefusedClearStorage();
        final guarded = PlaybackJournal(storage), stale = guarded.claim();
        await guarded.write(receipt, claim: stale);
        await expectLater(
          guarded.clear(claim: stale),
          throwsA(isA<VodFailure>()),
        );
        final fresh = guarded.claim();
        expect(guarded.blocked, isTrue);
        await expectLater(
          guarded.read(claim: fresh),
          throwsA(isA<VodFailure>()),
        );
        storage.refuse = false;
        await guarded.clear(claim: fresh);
        await guarded.write(receipt, claim: fresh);
        final saved = storage.value;
        await expectLater(
          guarded.clear(claim: stale),
          throwsA(isA<VodFailure>()),
        );
        await expectLater(
          guarded.write(receipt, claim: stale),
          throwsA(isA<VodFailure>()),
        );
        expect(storage.value, saved);
        await guarded.close();
      } finally {
        await f.close();
      }
    },
  );
  for (final phase in [
    'context',
    'watch',
    'prepare',
    'start',
    'status',
    'configuration',
    'checkpoint',
    'renew',
    'withdraw',
  ]) {
    test('late $phase HTTP completion cannot mutate a newer intent', () async {
      var elapsed = 0;
      final wall = DateTime.now().millisecondsSinceEpoch;
      final f = CommandFixture();
      await f.open(
        clock: PlaybackClock(now: () => elapsed, wall: () => wall + elapsed),
      );
      final gate = Completer<void>(), entered = Completer<void>();
      try {
        final preStart = const {
          'context',
          'watch',
          'prepare',
          'start',
        }.contains(phase);
        if (!preStart) await f.commands.open(androidSelection(f.item));
        f.beforeReply = (request) async {
          final path = request.uri.path, method = request.method;
          final match = switch (phase) {
            'context' => path.endsWith('/session-context'),
            'watch' => path.endsWith('/watch-data'),
            'prepare' => path.endsWith('/prepare'),
            'start' => path == '/api/v1/playback-commands',
            'status' => method == 'GET' && path == commandPath(f.start),
            'configuration' =>
              method == 'POST' && path.endsWith('/configuration'),
            'checkpoint' => method == 'POST' && path.endsWith('/checkpoint'),
            'renew' => path.endsWith('/renew'),
            _ => path.endsWith('/withdraw'),
          };
          if (match && !entered.isCompleted) {
            entered.complete();
            await gate.future;
          }
        };
        late Future<Object?> pending;
        if (preStart) {
          pending = f.commands.open(androidSelection(f.item), resume: true);
        } else if (phase == 'status') {
          pending = f.commands.check(once: true);
        } else if (phase == 'configuration') {
          pending = f.commands.change({'kind': 'seek', 'position_ms': 2500});
        } else if (phase == 'checkpoint') {
          f.commands.checkpoints.observe(f.output, 1234, 'paused');
          pending = f.commands.checkpoints
              .flush(force: true)
              .catchError((Object _) => null);
        } else if (phase == 'renew') {
          elapsed = 15000;
          f.commands.lease.tick();
          pending = f.commands.lease.drain();
        } else {
          pending = f.commands.close();
        }
        await entered.future.timeout(const Duration(seconds: 3));
        final identity = f.identity;
        f.identity = null;
        f.commands.invalidate();
        await f.commands.journal.drain();
        f.identity = identity;
        f.beforeReply = null;
        await f.commands.open(androidSelection(f.item));
        expect(f.commands.error, isNull);
        final current = f.commands.record!.start,
            saved = f.storage.value,
            deadline = f.commands.deadline;
        expect(object(jsonDecode(saved!))['start'], current);
        gate.complete();
        await pending;
        await f.commands.transport.regular.drain();
        await f.commands.transport.cleanup.drain();
        expect(f.commands.record!.start, current);
        expect(f.commands.deadline, deadline);
        expect(f.commands.error, isNull);
        expect(f.commands.deliverable, isTrue);
        expect(f.storage.value, saved);
      } finally {
        if (!gate.isCompleted) gate.complete();
        await f.close();
      }
    });
  }
  test(
    'strong invalidation clears an unpublished Start after actual storage completion',
    () async {
      final f = CommandFixture();
      await f.open();
      final gate = Completer<void>();
      f.storage.gate = gate;
      try {
        final opening = f.commands.open(androidSelection(f.item));
        await until(() => f.storage.calls.contains('write'));
        expect(f.commands.record, isNull);
        f.identity = null;
        f.commands.invalidate();
        gate.complete();
        await opening;
        await f.commands.journal.operations.drain();
        await Future<void>.delayed(const Duration(milliseconds: 20));
        expect(f.starts, 0);
        expect(f.storage.value, isNull);
        expect(f.commands.record, isNull);
        expect(f.commands.lease.running, isFalse);
      } finally {
        if (!gate.isCompleted) gate.complete();
        await f.close();
      }
    },
  );
  test(
    'status read stays exclusive after observation timeout until source completion',
    () async {
      final f = CommandFixture(), transport = HeldReadTransport();
      await f.open();
      try {
        await f.commands.open(androidSelection(f.item));
        final VodRecord record = f.commands.record!;
        final reader = PlaybackStatusReader(
          transport,
          (_, _) => true,
          () => 0,
          (_) async {},
        );
        await expectLater(
          reader.read(record, 0, 5000),
          throwsA(isA<VodFailure>()),
        );
        expect(transport.regular.count, 1);
        await expectLater(
          Future.sync(() => reader.read(record, 0, 5000)),
          throwsA(
            isA<VodFailure>().having((e) => e.code, 'code', 'status_busy'),
          ),
        );
        expect(transport.issued, 1);
        transport.source.complete(const VodReply(200, {}, 0));
        await reader.drain();
        await transport.regular.drain();
      } finally {
        if (!transport.source.isCompleted) {
          transport.source.complete(const VodReply(200, {}, 0));
        }
        await transport.dispose();
        await f.close();
      }
    },
  );
}

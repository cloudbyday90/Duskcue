// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import 'dart:async';
import 'dart:convert';
import 'dart:io';
import 'package:flutter_test/flutter_test.dart';
import 'package:uuid/uuid.dart';
import 'package:duskcue_mobile/playback/commands/clock.dart';
import 'package:duskcue_mobile/playback/commands/controller.dart';
import 'package:duskcue_mobile/playback/commands/journal.dart';
import 'package:duskcue_mobile/playback/commands/models.dart';
import 'package:duskcue_mobile/playback/commands/retained.dart';
import 'package:duskcue_mobile/playback/commands/scope.dart';
import 'package:duskcue_mobile/playback/commands/selection.dart';
import 'package:duskcue_mobile/playback/commands/transport.dart';
import 'package:duskcue_mobile/playback/commands/wire.dart';

class MemoryJournal implements JournalStorage {
  String? value;
  Completer<void>? gate;
  final calls = <String>[];
  @override
  Future<String?> read() async {
    calls.add('read');
    return value;
  }

  @override
  Future<void> write(String next) async {
    calls.add('write');
    await gate?.future;
    value = next;
  }

  @override
  Future<void> clear() async {
    calls.add('clear');
    value = null;
  }
}

class CommandFixture {
  late final HttpServer server;
  late PlaybackTransport transport;
  late PlaybackCommands commands;
  final storage = MemoryJournal();
  final user = const Uuid().v7(),
      session = const Uuid().v7(),
      profile = const Uuid().v7();
  final item = const Uuid().v7(),
      file = const Uuid().v7(),
      history = const Uuid().v7();
  String output = const Uuid().v7();
  PlaybackIdentity? identity;
  int starts = 0, withdrawals = 0, changes = 0, checkpoint = 0;
  bool lostStart = false, supported = true, fenced = false;
  Object? filePayload;
  String contextRevision = '1';
  String? fileContextRevision;
  late String start;
  final requests = <String>[];
  Wire get capability => {
    'protocol': vodProtocol,
    'features': [
      'start-settlement',
      'conditional-controls',
      'checkpoints',
      'lease-observation',
    ],
  };
  Future<void> open() async {
    server = await HttpServer.bind(InternetAddress.loopbackIPv4, 0);
    final origin = Uri.parse('http://127.0.0.1:${server.port}');
    identity = PlaybackIdentity(
      origin: origin,
      token: 'test-secret',
      credentialEpoch: 1,
      profileEpoch: 1,
      user: user,
      profile: profile,
      ready: true,
    );
    transport = PlaybackTransport(
      identity: () => identity,
      clock: PlaybackClock(),
    );
    commands = PlaybackCommands(transport, PlaybackJournal(storage));
    server.listen((request) async {
      try {
        requests.add('${request.method} ${request.uri.path}');
        expect(request.headers.value('authorization'), 'Bearer test-secret');
        final raw = await utf8.decoder.bind(request).join();
        final body = raw.isEmpty
            ? <String, Object?>{}
            : object(jsonDecode(raw));
        final path = request.uri.path;
        if (path.endsWith('/files')) {
          contextRevision = fileContextRevision ?? contextRevision;
          request.response.headers.contentType = ContentType.json;
          request.response.write(jsonEncode(filePayload ?? []));
          await request.response.close();
          return;
        }
        Wire value;
        final now = DateTime.now().toUtc();
        if (path.endsWith('/session-context')) {
          value = {
            'user_id': user,
            'session_id': session,
            'profile_id': profile,
            'credential_revision': '1',
            'profile_context_revision': contextRevision,
            'context_id': 'sc1.${'a' * 43}',
            'transport': 'bearer',
            'profile_selection_required': false,
            'observed_at': now.toIso8601String(),
            'session_expires_at': now
                .add(const Duration(hours: 1))
                .toIso8601String(),
          };
        } else if (path.endsWith('/prepare')) {
          expect(body['capability_protocol'], 1);
          expect(body['force_transcode'], true);
          value = {
            'protocol': 1,
            if (supported) 'command_capability': capability,
          };
        } else {
          if (!path.endsWith('/withdraw') && !path.endsWith('/settlement')) {
            expect(
              request.headers.value('x-session-context'),
              'sc1.${'a' * 43}',
            );
          }
          if (path == '/api/v1/playback-commands') {
            starts++;
            start = body['request_id'] as String;
            entity(start, v7: true);
            expect(storage.value, contains(start));
            if (lostStart) {
              (await request.response.detachSocket()).destroy();
              return;
            }
            request.response.statusCode = 202;
            request.response.headers.set('Location', commandPath(start));
          }
          if (path.endsWith('/withdraw') || path.endsWith('/settlement')) {
            if (request.method == 'POST') {
              withdrawals++;
              fenced = true;
            }
            value = {
              'protocol': vodProtocol,
              'request_id': start,
              'status': fenced ? 'settled' : 'pending',
              'status_url': commandPath(start, '/settlement'),
              'observed_at': now.toIso8601String(),
              'admission_fenced': fenced,
              'preparation_complete': fenced,
              'parent_released': fenced,
            };
          } else if (path.endsWith('/configuration')) {
            request.response.headers.set('etag', '"vod-config-${'a' * 64}"');
            if (request.method == 'POST') {
              changes++;
              output = const Uuid().v7();
              value = {
                'protocol': vodProtocol,
                'request_id': start,
                'operation_id': body['operation_id'],
                'status': 'completed',
                'cleanup_complete': true,
                'status_url': commandPath(
                  start,
                  '/operations/${body['operation_id']}',
                ),
              };
            } else {
              value = {
                'protocol': vodProtocol,
                'request_id': start,
                'output_revision': output,
                'revision': '1',
                'state': 'ready',
                'pending_operation': null,
                'worker_recovery': false,
                'configuration': {},
              };
            }
          } else if (path.contains('/operations/')) {
            value = {
              'protocol': vodProtocol,
              'request_id': start,
              'operation_id': path.split('/').last,
              'status': 'completed',
              'cleanup_complete': true,
              'status_url': path,
            };
          } else if (path.endsWith('/checkpoint')) {
            if (request.method == 'POST') checkpoint = body['sequence'] as int;
            value = {
              'protocol': vodProtocol,
              'request_id': start,
              'output_revision': output,
              'sequence': checkpoint,
              'position_ms': body['position_ms'] ?? 1000,
              'playback_state': body['playback_state'] ?? 'paused',
            };
          } else if (path.endsWith('/renew')) {
            value = {
              'protocol': vodProtocol,
              'request_id': start,
              'status': 'renewed',
              'status_url': commandPath(start),
              'observed_at': now.toIso8601String(),
              'lease_deadline': now
                  .add(const Duration(seconds: 60))
                  .toIso8601String(),
            };
          } else {
            value = {
              'protocol': vodProtocol,
              'request_id': start,
              'status': 'ready',
              'status_url': commandPath(start),
              'observed_at': now.toIso8601String(),
              'lease_deadline': now
                  .add(const Duration(seconds: 60))
                  .toIso8601String(),
              'ready': {
                'delivery_mode': 'transcode',
                'playback_mode': 'interactive',
                'output_revision': output,
                'session_id': history,
                'media_item_id': item,
                'media_file_id': file,
                'position_ms': 0,
                'stream_offset_ms': 0,
                'audio_stream_index': 1,
                'subtitle_stream_index': null,
              },
              'stream_url': commandPath(
                start,
                '/outputs/$output/hls/manifest.m3u8',
              ),
            };
          }
        }
        request.response.headers.contentType = ContentType.json;
        request.response.write(jsonEncode(value));
        await request.response.close();
      } catch (_) {
        request.response.statusCode = 500;
        await request.response.close();
        rethrow;
      }
    });
  }

  Future<void> close() async {
    commands.dispose();
    await transport.dispose();
    await server.close(force: true);
  }
}

void main() {
  test(
    'timed-out observation retains capacity until actual completion',
    () async {
      final owner = RetainedOperations(1), pending = Completer<int>();
      await expectLater(
        owner.run(
          () => pending.future,
          timeout: const Duration(milliseconds: 10),
        ),
        throwsA(isA<VodFailure>()),
      );
      expect(owner.count, 1);
      await expectLater(owner.run(() async => 2), throwsA(isA<VodFailure>()));
      pending.complete(1);
      await owner.drain();
      expect(await owner.run(() async => 3), 3);
    },
  );
  test('native journal clear cannot overtake a pending write', () async {
    final storage = MemoryJournal()..gate = Completer<void>();
    final journal = PlaybackJournal(storage);
    final scope = PlaybackScope.fromJson({
      'origin': 'http://localhost',
      'user': const Uuid().v7(),
      'session': const Uuid().v7(),
      'credential': '1',
      'profile': const Uuid().v7(),
      'revision': '1',
      'context': 'sc1.${'a' * 43}',
      'transport': 'bearer',
    });
    final write = journal.write(
      VodRecord(scope: scope, start: const Uuid().v7()),
    );
    final clear = journal.clear();
    await Future<void>.delayed(Duration.zero);
    expect(storage.calls, ['write']);
    storage.gate!.complete();
    await write;
    await clear;
    expect(storage.value, isNull);
    expect(storage.calls, ['write', 'read', 'clear', 'read']);
  });
  test('server-time UUIDv7 refuses stale or drifting clock evidence', () {
    var now = 0, wall = 100000;
    final clock = PlaybackClock(now: () => now, wall: () => wall);
    clock.observe(DateTime.utc(2026, 10, 28), 0);
    expect(clock.id().substring(14, 15), '7');
    now = 31000;
    wall += 31000;
    expect(clock.id, throwsA(isA<VodFailure>()));
    clock.observe(DateTime.utc(2026, 10, 28), now);
    wall += 3000;
    expect(clock.id, throwsA(isA<VodFailure>()));
  });
  test(
    'real HTTP start, output replacement and withdrawal preserve one viewer',
    () async {
      final fixture = CommandFixture();
      await fixture.open();
      addTearDown(fixture.close);
      final commands = fixture.commands;
      expect(await commands.open(androidSelection(fixture.item)), isTrue);
      expect(commands.error, isNull);
      expect(commands.deliverable, isTrue);
      final start = commands.record!.start, oldOutput = commands.record!.output;
      commands.observe(oldOutput!, 1000, 'paused');
      await commands.checkpoints.drain();
      await commands.change({'kind': 'seek', 'position_ms': 5000});
      expect(commands.error, isNull);
      expect(commands.record!.start, start);
      expect(commands.record!.output, isNot(oldOutput));
      expect(fixture.starts, 1);
      expect(fixture.changes, 1);
      await commands.close();
      expect(commands.record, isNull);
      expect(fixture.withdrawals, 1);
      expect(fixture.storage.value, isNull);
    },
  );
  test(
    'lost accepted Start is reconciled without repeating mutation',
    () async {
      final fixture = CommandFixture()..lostStart = true;
      await fixture.open();
      addTearDown(fixture.close);
      await fixture.commands.open(androidSelection(fixture.item));
      expect(fixture.commands.state, 'uncertain');
      expect(fixture.starts, 1);
      expect(
        VodRecord.parse(object(jsonDecode(fixture.storage.value!))).start,
        fixture.commands.record!.start,
      );
      await fixture.commands.check(once: true);
      expect(fixture.commands.deliverable, isTrue);
      expect(fixture.starts, 1);
      await fixture.commands.close();
      expect(fixture.commands.state, 'settled');
    },
  );
  test('absent capability selects legacy only before any Start', () async {
    final fixture = CommandFixture()..supported = false;
    await fixture.open();
    addTearDown(fixture.close);
    expect(
      await fixture.commands.open(androidSelection(fixture.item)),
      isFalse,
    );
    expect(fixture.starts, 0);
    expect(fixture.storage.value, isNull);
  });
  test(
    'cold journal restoration permits cleanup only and never reattaches URL',
    () async {
      final fixture = CommandFixture();
      await fixture.open();
      addTearDown(fixture.close);
      await fixture.commands.open(androidSelection(fixture.item));
      fixture.commands.dispose();
      fixture.transport = PlaybackTransport(
        identity: () => fixture.identity,
        clock: PlaybackClock(),
      );
      fixture.commands = PlaybackCommands(
        fixture.transport,
        PlaybackJournal(fixture.storage),
      );
      expect(
        await fixture.commands.open(androidSelection(fixture.item)),
        isTrue,
      );
      expect(fixture.commands.cleanupOnly, isTrue);
      expect(fixture.commands.receipt, isNull);
      expect(fixture.commands.deliverable, isFalse);
      expect(fixture.starts, 1);
      await fixture.commands.close();
      expect(fixture.withdrawals, 1);
    },
  );
}

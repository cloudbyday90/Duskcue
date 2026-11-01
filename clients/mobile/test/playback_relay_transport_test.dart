// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import 'dart:async';
import 'dart:convert';
import 'dart:io';
import 'dart:typed_data';
import 'package:flutter_test/flutter_test.dart';
import 'package:duskcue_mobile/playback/commands/wire.dart';
import 'package:duskcue_mobile/playback/relay/raw_io.dart';
import 'package:duskcue_mobile/playback/relay/socket_adapter.dart';
import 'package:duskcue_mobile/playback/relay/transport.dart';
import 'relay_fixture.dart';

class DeferredRaw extends Stream<RawSocketEvent> implements RawSocket {
  final events = StreamController<RawSocketEvent>();
  final destruction = Completer<RawSocket>();
  final writes = <List<int>>[];
  int maximumWrite = 65536;
  bool closing = false;
  Uint8List? pending;
  bool writeEnabled = false;
  @override
  bool readEventsEnabled = false;
  @override
  bool get writeEventsEnabled => writeEnabled;
  @override
  set writeEventsEnabled(bool value) {
    writeEnabled = value;
  }

  void writable() {
    if (writeEnabled) events.add(RawSocketEvent.write);
  }

  @override
  Uint8List? read([int? length]) {
    final result = pending;
    pending = null;
    return result;
  }

  @override
  int write(List<int> buffer, [int offset = 0, int? count]) {
    final length = (count ?? buffer.length - offset).clamp(0, maximumWrite);
    writes.add(buffer.sublist(offset, offset + length));
    return length;
  }

  @override
  Future<RawSocket> close() {
    closing = true;
    return destruction.future;
  }

  @override
  StreamSubscription<RawSocketEvent> listen(
    void Function(RawSocketEvent)? onData, {
    Function? onError,
    void Function()? onDone,
    bool? cancelOnError,
  }) => events.stream.listen(
    onData,
    onError: onError,
    onDone: onDone,
    cancelOnError: cancelOnError,
  );
  @override
  void shutdown(SocketDirection direction) {}
  @override
  dynamic noSuchMethod(Invocation invocation) =>
      throw UnsupportedError('Unneeded fixture member');
}

void main() {
  test(
    'scope loss between partial writes refuses remaining bytes after a readiness wait',
    () async {
      final raw = DeferredRaw()..maximumWrite = 1;
      var current = true;
      final io = RelayRawIo(
        raw,
        active: () {
          require(current, 'scope_changed');
        },
      );
      final writing = io.writer.write([1, 2, 3]);
      final refused = expectLater(writing, throwsA(isA<VodFailure>()));
      await eventually(() => raw.writes.isNotEmpty);
      current = false;
      raw.writable();
      await refused;
      expect(raw.writes.expand((b) => b), [1]);
      final closing = io.close();
      raw.destruction.complete(raw);
      await closing;
      await raw.events.close();
    },
  );
  test(
    'partial and final writes wait for readiness; raw destruction remains owned',
    () async {
      final raw = DeferredRaw()..maximumWrite = 2;
      final io = RelayRawIo(raw, active: () {});
      var complete = false;
      final write = io.writer.write([1, 2, 3, 4]).then((_) => complete = true);
      await eventually(() => raw.writes.length == 1);
      expect(complete, isFalse);
      raw.writable();
      await eventually(() => raw.writes.length == 2);
      expect(complete, isFalse);
      raw.writable();
      await write;
      expect(raw.writes.expand((b) => b), [1, 2, 3, 4]);
      var closed = false;
      final closing = io.close().then((_) => closed = true);
      await eventually(() => raw.closing);
      expect(closed, isFalse);
      raw.destruction.complete(raw);
      await closing;
      await raw.events.close();
    },
  );

  test(
    'two queued chunks and failed physical close retain sticky ownership',
    () async {
      final raw = DeferredRaw();
      final io = RelayRawIo(raw, active: () {});
      final write = io.writer.enqueue(Uint8List(131072));
      final failure = expectLater(write, throwsA(isA<VodFailure>()));
      expect(() => io.writer.enqueue([1]), throwsA(isA<VodFailure>()));
      expect(io.writer.peakQueued, 2);
      final first = io.close();
      final failed = expectLater(first, throwsStateError);
      raw.destruction.completeError(StateError('fixture close refused'));
      await failed;
      await failure;
      expect(identical(io.close(), first), isTrue);
      await raw.events.close();
    },
  );

  test(
    'late connect settlement cannot release or expose a cancelled transport',
    () async {
      final issued = Completer<RelayDial>();
      final connected = Completer<RawSocket>();
      final raw = DeferredRaw();
      var cancellations = 0, closed = false;
      final transport = RelayTransport(
        Uri.parse('http://localhost:1/'),
        () {},
        connect: (_, _) => issued.future,
      );
      final task = await transport.factory(transport.uri, null, null);
      final refusal = expectLater(task.socket, throwsA(isA<VodFailure>()));
      final closing = transport.close().then((_) => closed = true);
      await Future<void>.delayed(Duration.zero);
      expect(closed, isFalse);
      issued.complete(RelayDial(connected.future, () => cancellations++));
      await Future<void>.delayed(Duration.zero);
      expect(cancellations, 1);
      connected.complete(raw);
      await eventually(() => raw.closing);
      expect(closed, isFalse);
      raw.destruction.complete(raw);
      await refusal;
      await closing;
      unawaited(raw.events.close());
    },
  );

  test(
    'factory rejects proxy, alternate origin and repeated creation before dispatch',
    () async {
      var connects = 0;
      final transport = RelayTransport(
        Uri.parse('http://localhost:1/a'),
        () {},
        connect: (_, _) async {
          connects++;
          throw StateError('fixture');
        },
      );
      await expectLater(
        transport.factory(Uri.parse('http://localhost:1/b'), null, null),
        throwsA(isA<VodFailure>()),
      );
      await expectLater(
        transport.factory(transport.uri, 'proxy.invalid', 8080),
        throwsA(isA<VodFailure>()),
      );
      expect(connects, 0);
      final task = await transport.factory(transport.uri, null, null);
      await expectLater(task.socket, throwsStateError);
      await expectLater(
        transport.factory(transport.uri, null, null),
        throwsA(isA<VodFailure>()),
      );
      await transport.close();
      expect(connects, 1);
    },
  );

  test(
    'SDK HTTP parser uses verified TLS; default trust and wrong name refuse',
    () async {
      final serverContext = SecurityContext()
        ..useCertificateChain('test/fixtures/relay-localhost-cert.pem')
        ..usePrivateKey('test/fixtures/relay-localhost-key.pem');
      final trust = SecurityContext(withTrustedRoots: false)
        ..setTrustedCertificates('test/fixtures/relay-localhost-cert.pem');
      final server = await HttpServer.bindSecure(
        InternetAddress.loopbackIPv4,
        0,
        serverContext,
      );
      var requests = 0;
      server.listen((request) async {
        requests++;
        request.response.write('verified');
        await request.response.close();
      }, onError: (Object _) {});
      Future<String> get(String host, SecurityContext? trust) async {
        final owner = RelayTransport(
          Uri.parse('https://$host:${server.port}/'),
          () {},
          securityContext: trust,
        );
        final client = HttpClient()
          ..findProxy = ((_) => 'DIRECT')
          ..connectionFactory = owner.factory;
        try {
          final request = await client.getUrl(owner.uri);
          return await utf8.decoder.bind(await request.close()).join();
        } finally {
          client.close(force: true);
          await owner.close();
        }
      }

      try {
        expect(await get('localhost', trust), 'verified');
        await expectLater(
          get('localhost', null),
          throwsA(isA<HandshakeException>()),
        );
        await expectLater(
          get('127.0.0.1', trust),
          throwsA(isA<HandshakeException>()),
        );
        expect(requests, 1);
      } finally {
        await server.close(force: true);
      }
    },
  );

  test(
    'cancellation joins stalled TLS handshake and TCP destruction',
    () async {
      final server = await RawServerSocket.bind(
        InternetAddress.loopbackIPv4,
        0,
      );
      final peers = <RawSocket>[];
      server.listen((peer) {
        peers.add(peer);
        peer.writeEventsEnabled = false;
        peer.listen((event) {
          if (event == RawSocketEvent.read) peer.read();
        });
      });
      final owner = RelayTransport(
        Uri.parse('https://127.0.0.1:${server.port}/'),
        () {},
      );
      final task = await owner.factory(owner.uri, null, null);
      final failed = expectLater(task.socket, throwsA(anything));
      try {
        await eventually(() => peers.isNotEmpty);
        await owner.close().timeout(const Duration(seconds: 3));
        await failed;
      } finally {
        for (final peer in peers) {
          await peer.close();
        }
        await server.close();
      }
    },
  );

  test(
    'socket send half-close preserves incoming stream until owner destruction',
    () async {
      final server = await ServerSocket.bind(InternetAddress.loopbackIPv4, 0);
      server.listen((peer) {
        peer.listen(
          (_) {},
          onDone: () async {
            peer.add([4, 5]);
            await peer.close();
          },
        );
      });
      final raw = await RawSocket.connect('127.0.0.1', server.port);
      final io = RelayRawIo(raw, active: () {}), received = <int>[];
      final socket = RelaySocket(io), done = Completer<void>();
      socket.listen(received.addAll, onDone: done.complete);
      socket.add([1, 2, 3]);
      await socket.close();
      await done.future.timeout(const Duration(seconds: 3));
      expect(received, [4, 5]);
      await io.close();
      await server.close();
    },
  );
}

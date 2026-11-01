// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import 'dart:async';
import 'dart:convert';
import 'dart:io';
import 'dart:typed_data';
import '../commands/wire.dart';
import 'raw_io.dart';

class RelaySocket extends Stream<Uint8List> implements Socket {
  RelaySocket(this.io) {
    _done.future.then((_) {}, onError: (Object _) {});
  }
  final RelayRawIo io;
  final _done = Completer<void>();
  bool _sinkClosed = false, _adding = false;
  Encoding _encoding = utf8;
  Future<void>? _sinkClose;
  @override
  Encoding get encoding => _encoding;
  @override
  set encoding(Encoding value) {
    require(!_sinkClosed);
    _encoding = value;
  }

  @override
  StreamSubscription<Uint8List> listen(
    void Function(Uint8List)? onData, {
    Function? onError,
    void Function()? onDone,
    bool? cancelOnError,
  }) => io.input.listen(
    onData,
    onError: onError,
    onDone: onDone,
    cancelOnError: cancelOnError,
  );
  @override
  void add(List<int> bytes) {
    require(!_sinkClosed && !_adding, 'socket_closed');
    final work = io.writer.enqueue(bytes);
    work.then(
      (_) {},
      onError: (Object error, StackTrace stack) {
        if (!_done.isCompleted) _done.completeError(error, stack);
        destroy();
      },
    );
  }

  @override
  Future<void> addStream(Stream<List<int>> stream) async {
    require(!_sinkClosed && !_adding, 'socket_closed');
    _adding = true;
    try {
      await for (final bytes in stream) {
        await io.writer.write(bytes);
      }
    } finally {
      _adding = false;
    }
  }

  @override
  void write(Object? object) => add(_encoding.encode('$object'));
  @override
  void writeln([Object? object = '']) => write('$object\n');
  @override
  void writeAll(Iterable objects, [String separator = '']) =>
      write(objects.join(separator));
  @override
  void writeCharCode(int charCode) => write(String.fromCharCode(charCode));
  @override
  void addError(Object error, [StackTrace? stackTrace]) =>
      throw UnsupportedError('Socket addError');
  @override
  Future<void> flush() => io.writer.completion;
  @override
  Future<void> close() => _sinkClose ??= () async {
    _sinkClosed = true;
    try {
      await flush();
      if (!io.closing) io.socket.shutdown(SocketDirection.send);
      if (!_done.isCompleted) _done.complete();
    } catch (error, stack) {
      if (!_done.isCompleted) _done.completeError(error, stack);
      rethrow;
    }
  }();
  @override
  Future<void> get done => _done.future;
  @override
  void destroy() {
    _sinkClosed = true;
    final work = io.close();
    work.then(
      (_) {
        if (!_done.isCompleted) _done.complete();
      },
      onError: (Object error, StackTrace stack) {
        if (!_done.isCompleted) _done.completeError(error, stack);
      },
    );
  }

  @override
  bool setOption(SocketOption option, bool enabled) =>
      io.socket.setOption(option, enabled);
  @override
  Uint8List getRawOption(RawSocketOption option) =>
      io.socket.getRawOption(option);
  @override
  void setRawOption(RawSocketOption option) => io.socket.setRawOption(option);
  @override
  InternetAddress get address => io.socket.address;
  @override
  InternetAddress get remoteAddress => io.socket.remoteAddress;
  @override
  int get port => io.socket.port;
  @override
  int get remotePort => io.socket.remotePort;
  @override
  dynamic noSuchMethod(Invocation invocation) =>
      throw UnsupportedError('Unsupported socket operation');
}

class RelaySecureSocket extends RelaySocket implements SecureSocket {
  RelaySecureSocket(super.io);
  RawSecureSocket get secure => io.socket as RawSecureSocket;
  @override
  X509Certificate? get peerCertificate => secure.peerCertificate;
  @override
  String? get selectedProtocol => secure.selectedProtocol;
  @override
  void renegotiate({
    bool useSessionCache = true,
    bool requestClientCertificate = false,
    bool requireClientCertificate = false,
  }) => throw UnsupportedError('Relay TLS renegotiation');
}

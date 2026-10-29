// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import 'dart:convert';
import 'dart:io';
import 'package:dio/dio.dart';
import 'package:dio/io.dart';
import 'clock.dart';
import 'retained.dart';
import 'scope.dart';
import 'wire.dart';

class VodReply {
  const VodReply(
    this.status,
    this.body,
    this.began, {
    this.etag,
    this.location,
  });
  final int status, began;
  final Wire body;
  final String? etag, location;
}

class PlaybackTransport {
  PlaybackTransport({
    required this.identity,
    required this.clock,
    Dio Function()? createClient,
  }) : _client = (createClient ?? _defaultClient)();
  final PlaybackIdentity? Function() identity;
  final PlaybackClock clock;
  final Dio _client;
  final regular = RetainedOperations(3);
  final cleanup = RetainedOperations(2);
  final Set<CancelToken> _tokens = {};
  PlaybackContext? _bound;
  PlaybackContext? get bound => _bound;

  static Dio _defaultClient() {
    final dio = Dio();
    dio.httpClientAdapter = IOHttpClientAdapter(
      createHttpClient: () => HttpClient()..autoUncompress = false,
    );
    return dio;
  }

  bool current(PlaybackScope expected, {bool forCleanup = false}) {
    final bound = _bound;
    return bound != null &&
        bound.scope.matches(expected, cleanup: forCleanup) &&
        bound.identity.matches(identity(), cleanup: forCleanup) &&
        (forCleanup || bound.scope.profile == identity()?.profile);
  }

  Future<PlaybackContext> context({bool forCleanup = false}) async {
    final local = identity();
    require(local != null && local.token.isNotEmpty, 'scope_changed');
    final reply = await _send(
      'GET',
      '/api/v1/auth/session-context',
      local!,
      forCleanup: forCleanup,
    );
    require(local.matches(identity(), cleanup: true), 'scope_changed');
    require(reply.status == 200);
    final value = PlaybackContext.parse(
      reply.body,
      local,
      reply.began,
      cleanup: forCleanup,
    );
    _bound = value;
    clock.observe(value.observedAt, reply.began);
    return value;
  }

  Future<VodReply> call(
    String method,
    String path,
    PlaybackScope scope, {
    Wire? body,
    String? etag,
    bool forCleanup = false,
    Duration timeout = const Duration(seconds: 5),
  }) {
    require(current(scope, forCleanup: forCleanup), 'scope_changed');
    return _send(
      method,
      path,
      _bound!.identity,
      scope: scope,
      body: body,
      etag: etag,
      forCleanup: forCleanup,
      timeout: timeout,
    );
  }

  Future<List<Wire>> files(String item, PlaybackScope scope) async {
    require(current(scope), 'scope_changed');
    final reply = await _send(
      'GET',
      '/api/v1/media-items/${entity(item)}/files',
      _bound!.identity,
      scope: scope,
      fileCollection: true,
    );
    final observed = await context();
    require(observed.scope.matches(scope), 'scope_changed');
    return (reply.body['items'] as List).cast<Wire>();
  }

  Future<VodReply> _send(
    String method,
    String path,
    PlaybackIdentity local, {
    PlaybackScope? scope,
    Wire? body,
    String? etag,
    bool forCleanup = false,
    bool fileCollection = false,
    Duration timeout = const Duration(seconds: 5),
  }) {
    require(timeout.inMilliseconds > 0 && timeout.inMilliseconds <= 5000);
    require(
      path.startsWith('/api/v1/') &&
          !path.contains('%') &&
          !path.contains('?') &&
          !path.contains('#') &&
          !path.contains('..') &&
          path.length <= 512,
    );
    final bytes = body == null ? null : utf8.encode(jsonEncode(body));
    require(
      bytes == null ||
          bytes.length <=
              (path.endsWith('/checkpoint') || path.endsWith('/withdraw')
                  ? 1024
                  : 32768),
    );
    final cancel = CancelToken();
    final began = clock.now;
    return (forCleanup ? cleanup : regular).run(
      () async {
        _tokens.add(cancel);
        try {
          require(
            local.matches(identity(), cleanup: forCleanup || scope == null),
            'scope_changed',
          );
          final response = await _client.requestUri<ResponseBody>(
            local.origin.resolve(path),
            data: bytes == null ? null : utf8.decode(bytes),
            cancelToken: cancel,
            options: Options(
              method: method,
              responseType: ResponseType.stream,
              followRedirects: false,
              maxRedirects: 0,
              validateStatus: (_) => true,
              sendTimeout: timeout,
              receiveTimeout: timeout,
              headers: {
                'Authorization': 'Bearer ${local.token}',
                'Accept-Encoding': 'identity',
                'Accept': 'application/json',
                'Cache-Control': 'no-store',
                if (bytes != null) 'Content-Type': 'application/json',
                if (scope != null && !forCleanup)
                  'X-Session-Context': scope.context,
                if (etag != null) 'If-Match': etag,
              },
            ),
          );
          final stream = response.data;
          require(stream != null);
          final coding = response.headers.value('content-encoding');
          require(coding == null || coding == 'identity');
          final payload = <int>[];
          await for (final part in stream!.stream) {
            require(payload.length + part.length <= 32768);
            payload.addAll(part);
          }
          require(
            !cancel.isCancelled &&
                local.matches(identity(), cleanup: forCleanup || scope == null),
            'scope_changed',
          );
          if (scope != null) {
            require(current(scope, forCleanup: forCleanup), 'scope_changed');
          }
          final status = response.statusCode ?? 0;
          if (status < 200 || status >= 300) {
            throw VodFailure(
              'server_refused',
              status: status,
              retryAfter: retryDelay(
                response.headers.value('retry-after'),
                response.headers.value('date'),
              ),
            );
          }
          final type = response.headers.value('content-type') ?? '';
          require(type.split(';').first == 'application/json');
          return VodReply(
            status,
            fileCollection
                ? {'items': decodeFileList(payload)}
                : decodeWire(payload),
            began,
            etag: response.headers.value('etag'),
            location: response.headers.value('location'),
          );
        } on VodFailure {
          rethrow;
        } catch (_) {
          throw const VodFailure('network_uncertain');
        } finally {
          cancel.cancel();
          _tokens.remove(cancel);
        }
      },
      timeout: timeout,
      cancel: () => cancel.cancel(),
    );
  }

  void cancel() {
    for (final token in List.of(_tokens)) {
      token.cancel();
    }
  }

  Future<void> dispose() async {
    cancel();
    regular.closed = true;
    cleanup.closed = true;
    _client.close(force: true);
    await Future.wait([regular.drain(), cleanup.drain()]);
  }
}

Duration retryDelay(String? supplied, String? date) {
  if (supplied == null) return Duration.zero;
  if (supplied.length > 128) return const Duration(seconds: 5);
  final raw = supplied.trim();
  final seconds = RegExp(r'^[0-9]{1,8}$').hasMatch(raw) ? int.parse(raw) : null;
  if (seconds != null) return Duration(seconds: seconds.clamp(0, 86400));
  try {
    final observed = date == null
        ? DateTime.now().toUtc()
        : HttpDate.parse(date);
    return Duration(
      milliseconds: HttpDate.parse(
        raw,
      ).difference(observed).inMilliseconds.clamp(0, 86400000),
    );
  } catch (_) {
    return const Duration(seconds: 5);
  }
}

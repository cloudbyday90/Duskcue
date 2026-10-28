// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import 'dart:async';
import 'dart:io';
import '../commands/wire.dart';
import 'connection.dart';
import 'request.dart';
import 'service.dart';

Future<void> forward(
  PlaybackRelay owner,
  RelayConnection local,
  RelayRequest request,
) async {
  require(owner.clients.length < 4, 'relay_busy');
  final client = HttpClient()
    ..autoUncompress = false
    ..findProxy = ((_) => 'DIRECT')
    ..connectionTimeout = const Duration(seconds: 5)
    ..maxConnectionsPerHost = 1;
  owner.clients.add(client);
  var cancelled = false;
  void cancel() {
    cancelled = true;
    client.close(force: true);
    local.cancel();
  }

  var idle = Timer(const Duration(seconds: 5), cancel);
  final total = Timer(const Duration(seconds: 60), cancel);
  void resetIdle() {
    idle.cancel();
    idle = Timer(const Duration(seconds: 10), cancel);
  }

  void active() =>
      require(!cancelled && !owner.closing && owner.current(), 'scope_changed');
  try {
    active();
    final upstream = await client.getUrl(
      owner.origin.resolve('${owner.prefix}${request.path}'),
    );
    active();
    upstream.followRedirects = false;
    upstream.maxRedirects = 0;
    upstream.headers.set('Authorization', 'Bearer ${owner.token}');
    upstream.headers.set('X-Session-Context', owner.context);
    upstream.headers.set('Accept-Encoding', 'identity');
    upstream.headers.set('Cache-Control', 'no-store');
    if (request.range != null) upstream.headers.set('Range', request.range!);
    final response = await upstream.close();
    active();
    resetIdle();
    final coding = response.headers.value('content-encoding');
    require(coding == null || coding == 'identity');
    final binary = RelayRequest.binary.hasMatch(request.path);
    final maximum = binary ? 16777216 : 1048576;
    require(
      response.statusCode == 200 ||
          binary && request.range != null && response.statusCode == 206,
    );
    require(response.contentLength <= maximum);
    final type = response.headers.contentType?.mimeType;
    if (!binary) {
      require(
        type == 'application/vnd.apple.mpegurl' ||
            type == 'application/x-mpegurl',
      );
    }
    String? range;
    if (response.statusCode == 206) {
      range = response.headers.value('content-range');
      require(
        range != null &&
            validRange(range, request.range!, response.contentLength),
      );
    } else {
      require(response.headers.value('content-range') == null);
    }
    if (binary) {
      await local.respond(
        response.statusCode,
        length: response.contentLength < 0 ? null : response.contentLength,
        type: request.path.endsWith('init.mp4')
            ? 'video/mp4'
            : 'video/iso.segment',
        contentRange: range,
      );
    }
    var count = 0;
    final bytes = <int>[];
    await for (final part in response) {
      active();
      resetIdle();
      count += part.length;
      require(count <= maximum);
      if (binary) {
        await local.write(part);
        active();
      } else {
        bytes.addAll(part);
      }
    }
    active();
    require(response.contentLength < 0 || count == response.contentLength);
    if (!binary) {
      final rewritten = owner.playlists.rewrite(bytes, request.path);
      await local.respond(
        200,
        type: 'application/vnd.apple.mpegurl',
        length: rewritten.length,
      );
      active();
      await local.write(rewritten);
    }
  } finally {
    idle.cancel();
    total.cancel();
    client.close(force: true);
    owner.clients.remove(client);
  }
}

bool validRange(String value, String requested, int length) {
  final match = RegExp(
    r'^bytes ([0-9]{1,10})-([0-9]{1,10})/([0-9]{1,10})$',
  ).firstMatch(value);
  if (match == null) return false;
  final start = int.parse(match[1]!),
      end = int.parse(match[2]!),
      total = int.parse(match[3]!);
  if (start > end ||
      end >= total ||
      total > 16777216 ||
      length != end - start + 1) {
    return false;
  }
  final parts = requested.substring(6).split('-');
  if (parts[0].isEmpty) {
    return end == total - 1 &&
        start == (total - int.parse(parts[1])).clamp(0, total - 1);
  }
  return start == int.parse(parts[0]) &&
      end ==
          (parts[1].isEmpty
              ? total - 1
              : int.parse(parts[1]).clamp(0, total - 1));
}

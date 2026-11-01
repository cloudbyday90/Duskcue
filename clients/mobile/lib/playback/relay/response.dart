// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import 'dart:io';
import '../commands/wire.dart';
import 'request.dart';

class RelayResponse {
  RelayResponse(HttpClientResponse response, RelayRequest request) {
    binary = RelayRequest.binary.hasMatch(request.path);
    maximum = binary ? 16777216 : 1048576;
    final headers = response.headers;
    final coding = headers.value('content-encoding');
    require(coding == null || coding == 'identity');
    final transfer = headers.value('transfer-encoding');
    final length = headers.value('content-length');
    require(transfer == null || transfer.toLowerCase() == 'chunked');
    require(transfer == null || length == null);
    require(
      length == null || RegExp(r'^(?:0|[1-9][0-9]{0,9})$').hasMatch(length),
    );
    require(response.contentLength >= -1 && response.contentLength <= maximum);
    require(
      response.statusCode == 200 ||
          binary && request.range != null && response.statusCode == 206,
    );
    final contentType = headers.contentType;
    if (binary) {
      type = request.path.endsWith('init.mp4')
          ? 'video/mp4'
          : 'video/iso.segment';
      require(contentType?.mimeType == type);
    } else {
      require(
        contentType?.mimeType == 'application/vnd.apple.mpegurl' ||
            contentType?.mimeType == 'application/x-mpegurl',
      );
      require(
        contentType?.charset == null ||
            contentType!.charset!.toLowerCase() == 'utf-8',
      );
      type = 'application/vnd.apple.mpegurl';
    }
    range = headers.value('content-range');
    if (response.statusCode == 206) {
      require(
        range != null &&
            validRange(range!, request.range!, response.contentLength),
      );
    } else {
      require(range == null);
    }
  }
  late final bool binary;
  late final int maximum;
  late final String type;
  late final String? range;
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

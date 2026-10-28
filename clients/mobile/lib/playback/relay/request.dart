// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import 'dart:convert';
import '../commands/wire.dart';

class RelayRequest {
  const RelayRequest(this.path, this.range);
  final String path;
  final String? range;
  static final binary = RegExp(
    r'^[A-Za-z0-9_-]{1,64}/(?:init\.mp4|seg_[0-9]{1,50}\.m4s)$',
  );
  static final playlist = RegExp(r'^[A-Za-z0-9_-]{1,64}/index\.m3u8$');
  static bool resource(String value) =>
      value == 'manifest.m3u8' ||
      binary.hasMatch(value) ||
      playlist.hasMatch(value);
  factory RelayRequest.parse(
    List<int> bytes, {
    required String host,
    required String capability,
  }) {
    require(
      bytes.length <= 8192 &&
          bytes.every(
            (byte) => byte >= 32 && byte < 127 || byte == 13 || byte == 10,
          ),
      'bad_request',
    );
    final source = ascii.decode(bytes);
    require(
      source.endsWith('\r\n\r\n') &&
          source.indexOf('\r\n\r\n') == source.length - 4,
      'bad_request',
    );
    final lines = source.substring(0, source.length - 4).split('\r\n');
    final start = lines.removeAt(0).split(' ');
    require(
      start.length == 3 &&
          start[0] == 'GET' &&
          start[2] == 'HTTP/1.1' &&
          start[1].length <= 2048,
      'bad_request',
    );
    final prefix = '/$capability/';
    require(start[1].startsWith(prefix), 'bad_request');
    final path = start[1].substring(prefix.length);
    require(resource(path), 'bad_request');
    final headers = <String, String>{};
    for (final line in lines) {
      require(!line.contains('\r') && !line.contains('\n'), 'bad_request');
      final at = line.indexOf(':');
      require(at > 0, 'bad_request');
      final key = line.substring(0, at).toLowerCase();
      require(
        RegExp(r'^[a-z0-9!#$&*+.^_`|~-]+$').hasMatch(key) &&
            !headers.containsKey(key),
        'bad_request',
      );
      headers[key] = line.substring(at + 1).trim();
    }
    require(
      headers['host'] == host &&
          !headers.containsKey('authorization') &&
          !headers.containsKey('cookie') &&
          !headers.containsKey('transfer-encoding') &&
          !headers.containsKey('expect') &&
          !headers.containsKey('upgrade') &&
          !headers.containsKey('content-length'),
      'bad_request',
    );
    final range = headers['range'];
    if (range != null) {
      require(
        binary.hasMatch(path) &&
            RegExp(
              r'^bytes=(?:[0-9]{1,10}-[0-9]{0,10}|-[0-9]{1,10})$',
            ).hasMatch(range),
        'bad_request',
      );
      final pair = range.substring(6).split('-');
      if (pair[0].isEmpty) {
        require(int.parse(pair[1]) > 0, 'bad_request');
      } else if (pair[1].isNotEmpty) {
        require(int.parse(pair[1]) >= int.parse(pair[0]), 'bad_request');
      }
    }
    return RelayRequest(path, range);
  }
}

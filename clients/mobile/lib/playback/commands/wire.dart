// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import 'dart:convert';

typedef Wire = Map<String, Object?>;
const vodProtocol = 'duskcue-vod-command/2';

class VodFailure implements Exception {
  const VodFailure(this.code, {this.status, this.retryAfter = Duration.zero});
  final String code;
  final int? status;
  final Duration retryAfter;
  bool get validation => const [400, 413, 415, 422].contains(status);
  @override
  String toString() => code;
}

void require(bool condition, [String code = 'invalid_response']) {
  if (!condition) throw VodFailure(code);
}

Wire object(Object? value) {
  require(value is Map && value.length <= 128);
  final map = value as Map;
  require(map.keys.every((key) => key is String));
  return Map<String, Object?>.from(map);
}

String text(Object? value, {int max = 512}) {
  require(value is String && value.isNotEmpty && value.length <= max);
  return value as String;
}

int integer(Object? value, {int max = 2147483647}) {
  require(value is int && value >= 0 && value <= max);
  return value as int;
}

bool boolean(Object? value) {
  require(value is bool);
  return value as bool;
}

final _entity = RegExp(
  r'^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$',
);
final _v7 = RegExp(
  r'^[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$',
);
String entity(Object? value, {bool v7 = false}) {
  final result = text(value, max: 36);
  require((v7 ? _v7 : _entity).hasMatch(result));
  return result;
}

String revisionValue(Object? value) {
  final result = text(value, max: 19);
  require(RegExp(r'^[1-9][0-9]{0,18}$').hasMatch(result));
  require(BigInt.parse(result) <= BigInt.parse('9223372036854775807'));
  return result;
}

DateTime instant(Object? value) {
  final result = text(value, max: 40);
  final match = RegExp(
    r'^(\d{4})-(\d{2})-(\d{2})T(\d{2}):(\d{2}):(\d{2})(?:\.\d{1,9})?(?:Z|\+00:00)$',
  ).firstMatch(result);
  require(match != null);
  final parts = [for (var i = 1; i <= 6; i++) int.parse(match!.group(i)!)];
  final date = DateTime.utc(
    parts[0],
    parts[1],
    parts[2],
    parts[3],
    parts[4],
    parts[5],
  );
  require(
    date.year == parts[0] &&
        date.month == parts[1] &&
        date.day == parts[2] &&
        date.hour == parts[3] &&
        date.minute == parts[4] &&
        date.second == parts[5],
  );
  final parsed = DateTime.tryParse(result);
  require(parsed != null);
  return parsed!.toUtc();
}

void exactKeys(Wire value, Set<String> keys) {
  require(value.length == keys.length && value.keys.every(keys.contains));
}

Wire decodeWire(List<int> bytes, {int limit = 32768}) =>
    object(_decodeJson(bytes, limit));

List<Wire> decodeFileList(List<int> bytes) {
  final value = _decodeJson(bytes, 32768);
  require(value is List && value.length <= 128);
  return (value as List).map(object).toList(growable: false);
}

Object? _decodeJson(List<int> bytes, int limit) {
  require(bytes.length <= limit);
  final source = utf8.decode(bytes, allowMalformed: false);
  var depth = 0;
  var quoted = false;
  var escaped = false;
  for (final c in source.codeUnits) {
    if (quoted) {
      if (escaped) {
        escaped = false;
        continue;
      }
      if (c == 92) {
        escaped = true;
        continue;
      }
      if (c == 34) quoted = false;
    } else if (c == 34) {
      quoted = true;
    } else if (c == 123 || c == 91) {
      require(++depth <= 16);
    } else if (c == 125 || c == 93) {
      require(--depth >= 0);
    }
  }
  require(!quoted && depth == 0);
  return jsonDecode(source);
}

bool commandCapability(Object? value) {
  if (value == null) return false;
  final map = object(value);
  const features = {
    'start-settlement',
    'conditional-controls',
    'checkpoints',
    'lease-observation',
  };
  require(map['protocol'] == vodProtocol && map['features'] is List);
  final supplied = map['features'] as List;
  require(
    supplied.length == features.length &&
        supplied.toSet().containsAll(features),
  );
  return true;
}

String commandPath(String id, [String suffix = '']) =>
    '/api/v1/playback-commands/${entity(id, v7: true)}$suffix';

Uri resource(Uri origin, Object? value, String path) {
  final supplied = text(value);
  require(
    supplied == path && path.startsWith('/api/v1/') && !path.contains('%'),
  );
  final uri = origin.resolve(supplied);
  require(
    uri.origin == origin.origin &&
        uri.userInfo.isEmpty &&
        !uri.hasQuery &&
        !uri.hasFragment,
  );
  return uri;
}

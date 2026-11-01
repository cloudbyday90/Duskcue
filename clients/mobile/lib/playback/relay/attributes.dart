// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import '../commands/wire.dart';

void validateVariant(String source) {
  final attributes = <String, String>{};
  final pattern = RegExp(r'([A-Z-]+)=("[^"\r\n]*"|[^,\r\n]+)(,|$)');
  var offset = 0;
  while (offset < source.length) {
    final match = pattern.matchAsPrefix(source, offset);
    require(match != null);
    final key = match!.group(1)!, value = match.group(2)!;
    require(!attributes.containsKey(key));
    attributes[key] = value;
    offset = match.end;
    require(offset != source.length || match.group(3)!.isEmpty);
  }
  require(
    attributes.isNotEmpty &&
        attributes.keys.every(
          (key) => const ['BANDWIDTH', 'RESOLUTION', 'CODECS'].contains(key),
        ),
  );
  final bandwidth = attributes['BANDWIDTH'];
  require(
    bandwidth != null && RegExp(r'^[1-9][0-9]{0,9}$').hasMatch(bandwidth),
  );
  if (attributes['RESOLUTION'] case final value?) {
    require(RegExp(r'^[1-9][0-9]{0,4}x[1-9][0-9]{0,4}$').hasMatch(value));
  }
  if (attributes['CODECS'] case final value?) {
    require(RegExp(r'^"avc1\.[0-9a-fA-F]{6},mp4a\.40\.2"$').hasMatch(value));
  }
}

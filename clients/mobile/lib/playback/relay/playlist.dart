// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import 'dart:convert';
import '../commands/wire.dart';
import 'request.dart';

class RelayPlaylists {
  RelayPlaylists(this.upstreamPrefix, this.localPrefix);
  final String upstreamPrefix, localPrefix;
  final Map<String, Set<String>> _references = {};
  bool permits(String path) =>
      path == 'manifest.m3u8' ||
      _references.values.any((set) => set.contains(path));
  List<int> rewrite(List<int> bytes, String path) {
    require(bytes.length <= 1048576);
    final content = utf8.decode(bytes, allowMalformed: false);
    require(
      !content.contains('\u0000') &&
          !content.contains('\r') &&
          content.startsWith('#EXTM3U\n'),
    );
    final master = path == 'manifest.m3u8';
    final refs = <String>{};
    var variants = 0;
    String reference(String value) {
      require(value.startsWith(upstreamPrefix));
      final suffix = value.substring(upstreamPrefix.length);
      require(
        master
            ? RelayRequest.playlist.hasMatch(suffix)
            : RelayRequest.binary.hasMatch(suffix) &&
                  suffix.split('/').first == path.split('/').first,
      );
      refs.add(suffix);
      require(refs.length <= 8192);
      return '$localPrefix$suffix';
    }

    final lines = <String>[];
    var awaitingVariant = false;
    for (final line in content.split('\n')) {
      require(line.length <= 8192 && line == line.trim());
      if (line.isEmpty) {
        lines.add(line);
        continue;
      }
      if (!line.startsWith('#')) {
        if (master) {
          require(awaitingVariant && ++variants <= 16);
          awaitingVariant = false;
        }
        lines.add(reference(line));
      } else {
        require(
          !line.startsWith('#EXT-X-KEY') &&
              !line.startsWith('#EXT-X-SESSION-KEY') &&
              !line.startsWith('#EXT-X-DEFINE') &&
              !line.startsWith('#EXT-X-CONTENT-STEERING'),
        );
        if (line.contains('URI=')) {
          require(!master && line.startsWith('#EXT-X-MAP:'));
          final matches = RegExp('URI="([^"]+)"').allMatches(line).toList();
          require(matches.length == 1 && 'URI='.allMatches(line).length == 1);
          final match = matches.single;
          final target = reference(match.group(1)!);
          require(match.group(1)!.endsWith('/init.mp4'));
          lines.add(line.replaceRange(match.start, match.end, 'URI="$target"'));
        } else {
          if (line.startsWith('#EXT-X-STREAM-INF:')) {
            require(master && !awaitingVariant);
            awaitingVariant = true;
          }
          require(
            !line.startsWith('#EXT-X-MEDIA:') &&
                !line.startsWith('#EXT-X-I-FRAME-STREAM-INF:'),
          );
          lines.add(line);
        }
      }
    }
    require(!awaitingVariant && (!master || variants > 0));
    final next = {..._references, path: refs};
    if (master) {
      next.removeWhere((key, _) => key != path && !refs.contains(key));
    }
    require(
      next.values.fold<int>(0, (total, set) => total + set.length) <= 8192,
    );
    final result = utf8.encode(lines.join('\n'));
    require(result.length <= 1048576);
    _references
      ..clear()
      ..addAll(next);
    return result;
  }

  void clear() => _references.clear();
}

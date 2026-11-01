// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import 'dart:convert';
import '../commands/wire.dart';
import 'request.dart';
import 'attributes.dart';

class RelayPlaylists {
  RelayPlaylists(this.upstreamPrefix, this.localPrefix);
  final String upstreamPrefix, localPrefix;
  final Map<String, Set<String>> _references = {};
  int _revision = 0;
  int get revision => _revision;
  bool permits(String path) =>
      path == 'manifest.m3u8' ||
      _references.values.any((set) => set.contains(path));
  List<int> rewrite(List<int> bytes, String path, {int? revision}) {
    require(
      path == 'manifest.m3u8' ||
          permits(path) && (revision == null || revision == _revision),
    );
    require(bytes.length <= 1048576);
    final content = utf8.decode(bytes, allowMalformed: false);
    require(
      !content.contains('\u0000') &&
          !content.contains('\r') &&
          content.startsWith('#EXTM3U\n') &&
          content.endsWith('\n'),
    );
    final root = path == 'manifest.m3u8';
    final master = content
        .split('\n')
        .any((line) => line.startsWith('#EXT-X-STREAM-INF:'));
    require(!master || root);
    String? rendition = root ? null : path.split('/').first;
    final refs = <String>{};
    var variants = 0;
    String reference(String value) {
      require(value.startsWith(upstreamPrefix));
      final suffix = value.substring(upstreamPrefix.length);
      if (master) {
        require(RelayRequest.playlist.hasMatch(suffix));
      } else {
        require(RelayRequest.binary.hasMatch(suffix));
        rendition ??= suffix.split('/').first;
        require(suffix.split('/').first == rendition);
      }
      require(refs.add(suffix));
      require(refs.length <= 8192);
      return '$localPrefix$suffix';
    }

    final lines = <String>[];
    var awaitingVariant = false;
    var awaitingSegment = false, mapped = false, ended = false;
    final tags = <String>{};
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
        } else {
          require(awaitingSegment && mapped && !ended && line.endsWith('.m4s'));
          awaitingSegment = false;
        }
        lines.add(reference(line));
      } else {
        final tag = line.split(':').first;
        require(!ended && !awaitingVariant && !awaitingSegment);
        if (tag != '#EXTINF' && tag != '#EXT-X-STREAM-INF') {
          require(tags.add(tag));
        }
        if (tag == '#EXT-X-MAP') {
          require(!master && !mapped && refs.isEmpty);
          final match = RegExp(r'^#EXT-X-MAP:URI="([^"]+)"$').firstMatch(line);
          require(match != null);
          final target = reference(match!.group(1)!);
          require(match.group(1)!.endsWith('/init.mp4'));
          mapped = true;
          lines.add('#EXT-X-MAP:URI="$target"');
        } else {
          if (tag == '#EXT-X-STREAM-INF') {
            require(master && line.startsWith('$tag:'));
            validateVariant(line.substring(tag.length + 1));
            awaitingVariant = true;
          } else if (tag == '#EXTINF') {
            require(
              !master &&
                  mapped &&
                  RegExp(
                    r'^#EXTINF:[0-9]{1,5}(?:\.[0-9]{1,9})?,$',
                  ).hasMatch(line),
            );
            final duration = double.parse(line.substring(8, line.length - 1));
            require(duration > 0 && duration <= 86400);
            awaitingSegment = true;
          } else if (tag == '#EXT-X-ENDLIST') {
            require(!master && line == tag && refs.length > 1);
            ended = true;
          } else if (tag == '#EXT-X-VERSION') {
            require(RegExp(r'^#EXT-X-VERSION:[1-9][0-9]?$').hasMatch(line));
          } else if (tag == '#EXT-X-TARGETDURATION') {
            require(
              !master &&
                  RegExp(
                    r'^#EXT-X-TARGETDURATION:[1-9][0-9]{0,4}$',
                  ).hasMatch(line),
            );
            require(int.parse(line.substring(tag.length + 1)) <= 86400);
          } else if (tag == '#EXT-X-MEDIA-SEQUENCE') {
            require(
              !master &&
                  RegExp(r'^#EXT-X-MEDIA-SEQUENCE:[0-9]{1,10}$').hasMatch(line),
            );
          } else if (tag == '#EXT-X-PLAYLIST-TYPE') {
            require(!master && (line == '$tag:EVENT' || line == '$tag:VOD'));
          } else {
            require(line == '#EXTM3U' || line == '#EXT-X-INDEPENDENT-SEGMENTS');
          }
          lines.add(line);
        }
      }
    }
    require(
      !awaitingVariant &&
          !awaitingSegment &&
          (master
              ? variants > 0
              : mapped &&
                    refs.length > 1 &&
                    tags.contains('#EXT-X-TARGETDURATION')),
    );
    require(!content.contains('#EXT-X-PLAYLIST-TYPE:VOD') || ended);
    final next = {..._references, path: refs};
    if (root) {
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
    if (root) _revision++;
    return result;
  }

  void clear() {
    _references.clear();
    _revision++;
  }
}

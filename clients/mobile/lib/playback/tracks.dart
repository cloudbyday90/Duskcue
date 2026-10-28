// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import 'commands/controller.dart';
import 'commands/wire.dart';

class PlaybackTrack {
  const PlaybackTrack(this.index, this.label);
  final int index;
  final String label;
}

Future<List<PlaybackTrack>> playbackTracks(
  PlaybackCommands commands, {
  required bool audio,
}) async {
  final ready = commands.admitted, record = commands.record;
  require(
    ready != null &&
        record != null &&
        !commands.cleanupOnly &&
        const {'ready', 'unavailable'}.contains(commands.state),
  );
  final reply = await commands.transport.call(
    'GET',
    '/api/v1/media-items/${ready!.item}/files',
    record!.scope,
  );
  require(
    commands.admitted?.file == ready.file &&
        commands.record?.start == record.start,
  );
  final files = reply.body['items'];
  require(files is List && files.length <= 128);
  final matches = (files as List)
      .where((file) => object(file)['id'] == ready.file)
      .toList();
  require(matches.length == 1);
  final streams = object(object(matches.single)['additional_streams']);
  final tracks =
      streams[audio ? 'audio' : 'subtitles'] ??
      streams[audio ? 'audio_streams' : 'subtitle_streams'] ??
      [];
  require(tracks is List && tracks.length <= 64);
  final indexes = <int>{};
  return (tracks as List)
      .map((track) {
        final value = object(track);
        final index = integer(value['index']);
        require(indexes.add(index));
        final language = value['language'] == null
            ? ''
            : text(value['language'], max: 64);
        final codec = value['codec'] == null
            ? ''
            : text(value['codec'], max: 64);
        return PlaybackTrack(
          index,
          [
            language,
            codec,
            '${index + 1}',
          ].where((text) => text.isNotEmpty).join(' · '),
        );
      })
      .toList(growable: false);
}

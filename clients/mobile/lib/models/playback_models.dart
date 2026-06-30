class PlaybackStart {
  const PlaybackStart({
    required this.sessionId,
    required this.streamDecision,
    required this.streamUrl,
    required this.mediaItemId,
    this.mediaFileId,
    this.transcodeSessionId,
  });

  final String sessionId;
  final String streamDecision;
  final String streamUrl;
  final String mediaItemId;
  final String? mediaFileId;
  final String? transcodeSessionId;

  factory PlaybackStart.fromJson(Map<String, Object?> json) {
    return PlaybackStart(
      sessionId: _string(json, const ['session_id']),
      streamDecision: _string(json, const ['stream_decision', 'playback_type'], fallback: 'direct_play'),
      streamUrl: _string(json, const ['stream_url']),
      mediaItemId: _string(json, const ['media_item_id']),
      mediaFileId: _nullableString(json, const ['media_file_id']),
      transcodeSessionId: _nullableString(json, const ['transcode_session_id']),
    );
  }
}

class WatchData {
  const WatchData({
    this.resumePositionMs = 0,
    this.isWatched = false,
  });

  final int resumePositionMs;
  final bool isWatched;

  factory WatchData.fromJson(Map<String, Object?> json) {
    return WatchData(
      resumePositionMs: _int(json, const ['resume_position_ms', 'position_ms']) ?? 0,
      isWatched: json['is_watched'] == true,
    );
  }
}

class AudioTrack {
  const AudioTrack({
    required this.index,
    required this.label,
  });

  final int index;
  final String label;

  factory AudioTrack.fromJson(Map<String, Object?> json) {
    final index = _int(json, const ['index', 'stream_index', 'audio_stream_index']) ?? 0;
    final language = _nullableString(json, const ['language', 'audio_language']);
    final codec = _nullableString(json, const ['codec', 'audio_codec']);
    final channels = _int(json, const ['channels', 'audio_channels']);
    return AudioTrack(
      index: index,
      label: [language, codec, if (channels != null) '$channels ch'].whereType<String>().join(' · '),
    );
  }
}

class SubtitleTrack {
  const SubtitleTrack({
    required this.id,
    required this.label,
    this.streamIndex,
  });

  final String id;
  final String label;
  final int? streamIndex;

  factory SubtitleTrack.fromJson(Map<String, Object?> json) {
    final language = _nullableString(json, const ['language']);
    final type = _nullableString(json, const ['subtitle_type', 'type']);
    final provider = _nullableString(json, const ['source_provider']);
    return SubtitleTrack(
      id: _string(json, const ['id', 'subtitle_file_id']),
      streamIndex: _int(json, const ['subtitle_stream_index', 'stream_index']),
      label: [language, type, provider].whereType<String>().join(' · '),
    );
  }
}

class SegmentSkip {
  const SegmentSkip({
    required this.id,
    required this.segmentType,
    required this.startMs,
    required this.endMs,
    required this.skipToMs,
  });

  final String id;
  final String segmentType;
  final int startMs;
  final int endMs;
  final int skipToMs;

  factory SegmentSkip.fromJson(Map<String, Object?> json) {
    final endMs = _int(json, const ['end_ms']) ?? 0;
    return SegmentSkip(
      id: _string(json, const ['id', 'segment_id']),
      segmentType: _string(json, const ['segment_type', 'type'], fallback: 'segment'),
      startMs: _int(json, const ['start_ms']) ?? 0,
      endMs: endMs,
      skipToMs: _int(json, const ['skip_to_ms']) ?? endMs,
    );
  }

  bool isActiveAt(int positionMs) {
    return positionMs >= startMs && positionMs <= endMs;
  }
}

class PlaybackSeekResult {
  const PlaybackSeekResult({
    required this.sessionId,
    required this.positionMs,
    this.streamUrl,
    this.transcodeSessionId,
  });

  final String sessionId;
  final int positionMs;
  final String? streamUrl;
  final String? transcodeSessionId;

  factory PlaybackSeekResult.fromJson(Map<String, Object?> json) {
    return PlaybackSeekResult(
      sessionId: _string(json, const ['session_id']),
      positionMs: _int(json, const ['position_ms']) ?? 0,
      streamUrl: _nullableString(json, const ['stream_url']),
      transcodeSessionId: _nullableString(json, const ['transcode_session_id']),
    );
  }
}

String _string(Map<String, Object?> json, List<String> keys, {String fallback = ''}) {
  return _nullableString(json, keys) ?? fallback;
}

String? _nullableString(Map<String, Object?> json, List<String> keys) {
  for (final key in keys) {
    final value = json[key];
    if (value is String && value.isNotEmpty) return value;
    if (value != null && value is! Map && value is! List) return value.toString();
  }
  return null;
}

int? _int(Map<String, Object?> json, List<String> keys) {
  for (final key in keys) {
    final value = json[key];
    if (value is int) return value;
    if (value is num) return value.toInt();
    if (value is String) return int.tryParse(value);
  }
  return null;
}

import 'package:duskcue_mobile/models/playback_models.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('playback start accepts legacy playback type field', () {
    final start = PlaybackStart.fromJson({
      'session_id': 'session-1',
      'playback_type': 'transcode',
      'stream_url': '/api/v1/stream/session-1/master.m3u8',
      'media_item_id': 'item-1',
      'media_file_id': 'file-1',
    });

    expect(start.sessionId, 'session-1');
    expect(start.streamDecision, 'transcode');
    expect(start.streamUrl, '/api/v1/stream/session-1/master.m3u8');
    expect(start.mediaFileId, 'file-1');
  });

  test('watch data falls back to zero resume position', () {
    final data = WatchData.fromJson({'is_watched': true});

    expect(data.resumePositionMs, 0);
    expect(data.isWatched, isTrue);
  });

  test('segment skip state includes segment boundary positions', () {
    final segment = SegmentSkip.fromJson({
      'id': 'intro-1',
      'type': 'intro',
      'start_ms': 1000,
      'end_ms': 9000,
    });

    expect(segment.skipToMs, 9000);
    expect(segment.isActiveAt(1000), isTrue);
    expect(segment.isActiveAt(9000), isTrue);
    expect(segment.isActiveAt(9500), isFalse);
  });
}

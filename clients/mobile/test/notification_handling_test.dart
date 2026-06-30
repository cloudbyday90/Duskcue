import 'package:duskcue_mobile/models/auth_models.dart';
import 'package:duskcue_mobile/models/content_models.dart';
import 'package:duskcue_mobile/models/realtime_models.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('SSE frames decode notification payloads', () {
    const frame = SseFrame(
      event: 'notification',
      id: 'event-1',
      dataLines: ['{"notification_id":"notification-1","title":"Ready"}'],
    );

    final event = frame.toEvent();

    expect(event, isNotNull);
    expect(event!.type, 'notification');
    expect(event.id, 'event-1');
    expect(event.jsonData['notification_id'], 'notification-1');
  });

  test('notification summaries expose read state', () {
    final unread = NotificationSummary.fromJson({
      'id': 'notification-1',
      'title': 'Scan complete',
      'created_at': '2026-06-30T12:00:00Z',
    });
    final read = NotificationSummary.fromJson({
      'id': 'notification-2',
      'title': 'Playback ready',
      'read_at': '2026-06-30T12:05:00Z',
    });

    expect(unread.isRead, isFalse);
    expect(read.isRead, isTrue);
  });

  test('push device summaries preserve invalidation state without token values', () {
    final device = PushDeviceSummary.fromJson({
      'id': 'device-1',
      'provider': 'fcm',
      'token_preview': 'abcd...1234',
      'is_active': false,
      'invalidated_at': '2026-06-30T12:00:00Z',
      'created_at': '2026-06-29T12:00:00Z',
      'updated_at': '2026-06-30T12:00:00Z',
    });

    expect(device.provider, 'fcm');
    expect(device.tokenPreview, 'abcd...1234');
    expect(device.isActive, isFalse);
    expect(device.invalidatedAt, isNotNull);
  });
}

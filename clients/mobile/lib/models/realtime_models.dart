import 'dart:convert';

class RealtimeEvent {
  const RealtimeEvent({
    required this.type,
    required this.data,
    this.id,
  });

  final String type;
  final Object? data;
  final String? id;

  Map<String, Object?> get jsonData {
    final value = data;
    if (value is Map<String, Object?>) return value;
    if (value is Map) return Map<String, Object?>.from(value);
    return const {};
  }
}

class SseFrame {
  const SseFrame({
    required this.event,
    required this.dataLines,
    this.id,
  });

  final String event;
  final List<String> dataLines;
  final String? id;

  RealtimeEvent? toEvent() {
    if (dataLines.isEmpty) return null;
    final raw = dataLines.join('\n');
    Object? data;
    try {
      data = jsonDecode(raw);
    } catch (_) {
      data = raw;
    }
    return RealtimeEvent(type: event.isEmpty ? 'message' : event, data: data, id: id);
  }
}

enum RealtimeConnectionStatus {
  disconnected,
  connecting,
  connected,
}

class RealtimeState {
  const RealtimeState({
    this.status = RealtimeConnectionStatus.disconnected,
    this.unreadCount = 0,
    this.lastEventType,
    this.lastEventId,
    this.lastError,
  });

  final RealtimeConnectionStatus status;
  final int unreadCount;
  final String? lastEventType;
  final String? lastEventId;
  final String? lastError;

  RealtimeState copyWith({
    RealtimeConnectionStatus? status,
    int? unreadCount,
    String? lastEventType,
    String? lastEventId,
    String? lastError,
  }) {
    return RealtimeState(
      status: status ?? this.status,
      unreadCount: unreadCount ?? this.unreadCount,
      lastEventType: lastEventType ?? this.lastEventType,
      lastEventId: lastEventId ?? this.lastEventId,
      lastError: lastError,
    );
  }
}

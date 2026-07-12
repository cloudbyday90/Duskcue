// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

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

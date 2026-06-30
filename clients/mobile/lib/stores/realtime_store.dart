import 'package:duskcue_mobile/models/realtime_models.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

class RealtimeNotifier extends Notifier<RealtimeState> {
  @override
  RealtimeState build() {
    return const RealtimeState();
  }

  void setStatus(RealtimeConnectionStatus status, {String? error}) {
    state = state.copyWith(status: status, lastError: error);
  }

  void setUnreadCount(int count) {
    state = state.copyWith(unreadCount: count < 0 ? 0 : count);
  }

  void incrementUnread() {
    state = state.copyWith(unreadCount: state.unreadCount + 1);
  }

  void recordEvent(RealtimeEvent event) {
    state = state.copyWith(
      lastEventType: event.type,
      lastEventId: event.id ?? state.lastEventId,
      lastError: null,
    );
  }
}

final realtimeProvider = NotifierProvider<RealtimeNotifier, RealtimeState>(
  RealtimeNotifier.new,
);

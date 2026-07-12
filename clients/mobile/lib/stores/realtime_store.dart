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

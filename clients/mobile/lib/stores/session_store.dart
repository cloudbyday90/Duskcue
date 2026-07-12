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

import 'package:duskcue_mobile/models/auth_models.dart';
import 'package:duskcue_mobile/models/server_profile.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

class SessionState {
  const SessionState({
    this.server,
    this.user,
    this.isAuthenticated = false,
  });

  final ServerProfile? server;
  final UserSummary? user;
  final bool isAuthenticated;

  SessionState copyWith({
    ServerProfile? server,
    UserSummary? user,
    bool? isAuthenticated,
  }) {
    return SessionState(
      server: server ?? this.server,
      user: user ?? this.user,
      isAuthenticated: isAuthenticated ?? this.isAuthenticated,
    );
  }

  SessionState clearAuth() {
    return SessionState(server: server);
  }
}

class SessionNotifier extends Notifier<SessionState> {
  @override
  SessionState build() {
    return const SessionState();
  }

  void selectServer(ServerProfile server) {
    state = state.copyWith(server: server);
  }

  void setAuthenticated(UserSummary user) {
    state = state.copyWith(user: user, isAuthenticated: true);
  }

  void clearAuthentication() {
    state = state.clearAuth();
  }
}

final sessionProvider = NotifierProvider<SessionNotifier, SessionState>(
  SessionNotifier.new,
);

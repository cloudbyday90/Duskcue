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

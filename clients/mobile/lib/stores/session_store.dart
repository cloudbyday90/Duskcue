import 'package:duskcue_mobile/models/server_profile.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

class SessionState {
  const SessionState({
    this.server,
    this.isAuthenticated = false,
  });

  final ServerProfile? server;
  final bool isAuthenticated;

  SessionState copyWith({
    ServerProfile? server,
    bool? isAuthenticated,
  }) {
    return SessionState(
      server: server ?? this.server,
      isAuthenticated: isAuthenticated ?? this.isAuthenticated,
    );
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

  void setAuthenticated(bool value) {
    state = state.copyWith(isAuthenticated: value);
  }
}

final sessionProvider = NotifierProvider<SessionNotifier, SessionState>(
  SessionNotifier.new,
);

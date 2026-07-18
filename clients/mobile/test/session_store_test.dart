import 'package:duskcue_mobile/models/auth_models.dart';
import 'package:duskcue_mobile/models/server_profile.dart';
import 'package:duskcue_mobile/stores/session_store.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('session state keeps selected server when auth is cleared', () {
    final server = ServerProfile(origin: Uri.parse('http://localhost:48027'));
    const user = UserSummary(
      id: 'user-1',
      username: 'owner',
      displayName: 'Owner',
      role: 'owner',
      capabilities: ['can_manage_server'],
      hasAllLibraryAccess: true,
    );

    final authenticated = SessionState(
      server: server,
    ).copyWith(user: user, isAuthenticated: true);
    final cleared = authenticated.clearAuth();

    expect(authenticated.isAuthenticated, isTrue);
    expect(cleared.server, server);
    expect(cleared.user, isNull);
    expect(cleared.isAuthenticated, isFalse);
    expect(cleared.profileScopeStatus, ProfileScopeStatus.uninitialized);
  });

  test('profile scope is unresolved until the profile gate returns', () {
    final server = ServerProfile(origin: Uri.parse('http://localhost:48027'));
    const user = UserSummary(
      id: 'user-1',
      username: 'owner',
      displayName: 'Owner',
      role: 'owner',
      capabilities: [],
      hasAllLibraryAccess: true,
    );
    final initial = SessionState(
      server: server,
      user: user,
      isAuthenticated: true,
    );
    final resolved = initial.copyWith(
      user: user.copyWith(
        activeProfileId: 'profile-1',
        profileSelectionRequired: false,
      ),
      profileScopeStatus: ProfileScopeStatus.ready,
    );

    expect(initial.isProfileScopeReady, isFalse);
    expect(resolved.isProfileScopeReady, isTrue);
    expect(resolved.user?.activeProfileId, 'profile-1');
  });
}

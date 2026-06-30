import 'package:duskcue_mobile/models/server_profile.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('canonicalizes local server input to the public Duskcue port', () {
    final origin = canonicalizeServerOrigin('media-box', networkMode: NetworkMode.local);

    expect(origin.toString(), 'http://media-box:48027');
  });

  test('requires HTTPS for exposed servers', () {
    expect(
      () => canonicalizeServerOrigin('http://duskcue.example.com:48027', networkMode: NetworkMode.exposed),
      throwsFormatException,
    );
  });

  test('rejects the internal Docker API port', () {
    expect(
      () => canonicalizeServerOrigin('http://localhost:48028', networkMode: NetworkMode.local),
      throwsFormatException,
    );
  });

  test('round-trips saved server profiles', () {
    final connectedAt = DateTime.utc(2026, 6, 30, 12);
    final profile = ServerProfile(
      origin: Uri.parse('https://duskcue.example.com:48027'),
      networkMode: NetworkMode.exposed,
      displayName: 'Home',
      lastConnectedAt: connectedAt,
    );

    final restored = ServerProfile.fromJson(profile.toJson());

    expect(restored.origin.toString(), 'https://duskcue.example.com:48027');
    expect(restored.networkMode, NetworkMode.exposed);
    expect(restored.displayName, 'Home');
    expect(restored.lastConnectedAt, connectedAt);
  });
}

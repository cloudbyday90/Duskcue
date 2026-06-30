import 'dart:convert';

import 'package:duskcue_mobile/models/server_profile.dart';
import 'package:duskcue_mobile/services/secure_storage_service.dart';

class ServerRepository {
  const ServerRepository(this._storage);

  final SecureStorageService _storage;

  Future<List<ServerProfile>> readSavedServers() async {
    final value = await _storage.readSavedServers();
    if (value == null || value.isEmpty) return const [];

    final decoded = jsonDecode(value);
    if (decoded is! List) return const [];

    return decoded
        .whereType<Map>()
        .map((item) => ServerProfile.fromJson(Map<String, Object?>.from(item)))
        .toList(growable: false);
  }

  Future<ServerProfile?> readLastServer() async {
    final origin = await _storage.readLastServerOrigin();
    if (origin == null) return null;

    final saved = await readSavedServers();
    for (final server in saved) {
      if (server.origin.toString() == origin) return server;
    }
    return null;
  }

  Future<void> saveConnectedServer(ServerProfile profile) async {
    final connected = profile.copyWith(lastConnectedAt: DateTime.now().toUtc());
    final saved = await readSavedServers();
    final next = <ServerProfile>[
      connected,
      ...saved.where((server) => server.origin.toString() != connected.origin.toString()),
    ];

    await _storage.writeSavedServers(
      jsonEncode(next.map((server) => server.toJson()).toList(growable: false)),
    );
    await _storage.writeLastServerOrigin(connected.origin.toString());
  }
}

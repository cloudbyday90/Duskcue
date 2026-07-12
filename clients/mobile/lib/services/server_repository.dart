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

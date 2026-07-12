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

enum NetworkMode {
  local,
  remoteVpn,
  exposed;

  String get jsonName {
    return switch (this) {
      NetworkMode.local => 'local',
      NetworkMode.remoteVpn => 'remote_vpn',
      NetworkMode.exposed => 'exposed',
    };
  }

  String get label {
    return switch (this) {
      NetworkMode.local => 'Local',
      NetworkMode.remoteVpn => 'Remote VPN',
      NetworkMode.exposed => 'Exposed',
    };
  }

  String get description {
    return switch (this) {
      NetworkMode.local => 'LAN or local test server. HTTP is allowed on port 48027.',
      NetworkMode.remoteVpn => 'VPN-only access. HTTP is acceptable when the VPN supplies transport security.',
      NetworkMode.exposed => 'Public internet access. HTTPS with an OS-trusted certificate is required.',
    };
  }

  String get defaultScheme {
    return this == NetworkMode.exposed ? 'https' : 'http';
  }

  static NetworkMode fromJson(String? value) {
    return switch (value) {
      'remote_vpn' => NetworkMode.remoteVpn,
      'exposed' => NetworkMode.exposed,
      _ => NetworkMode.local,
    };
  }
}

class ServerProfile {
  const ServerProfile({
    required this.origin,
    this.networkMode = NetworkMode.local,
    this.displayName,
    this.lastConnectedAt,
  });

  final Uri origin;
  final NetworkMode networkMode;
  final String? displayName;
  final DateTime? lastConnectedAt;

  ServerProfile copyWith({
    Uri? origin,
    NetworkMode? networkMode,
    String? displayName,
    DateTime? lastConnectedAt,
  }) {
    return ServerProfile(
      origin: origin ?? this.origin,
      networkMode: networkMode ?? this.networkMode,
      displayName: displayName ?? this.displayName,
      lastConnectedAt: lastConnectedAt ?? this.lastConnectedAt,
    );
  }

  Map<String, Object?> toJson() {
    return {
      'origin': origin.toString(),
      'network_mode': networkMode.jsonName,
      'display_name': displayName,
      'last_connected_at': lastConnectedAt?.toIso8601String(),
    };
  }

  static ServerProfile fromJson(Map<String, Object?> json) {
    return ServerProfile(
      origin: Uri.parse(json['origin'] as String),
      networkMode: NetworkMode.fromJson(json['network_mode'] as String?),
      displayName: json['display_name'] as String?,
      lastConnectedAt: json['last_connected_at'] == null
          ? null
          : DateTime.parse(json['last_connected_at'] as String),
    );
  }

  static ServerProfile fromInput(String input, {required NetworkMode networkMode}) {
    return ServerProfile(
      origin: canonicalizeServerOrigin(input, networkMode: networkMode),
      networkMode: networkMode,
    );
  }
}

Uri canonicalizeServerOrigin(String input, {required NetworkMode networkMode}) {
  final trimmed = input.trim();
  if (trimmed.isEmpty) {
    throw const FormatException('Enter a server URL.');
  }

  final withScheme = trimmed.contains('://') ? trimmed : '${networkMode.defaultScheme}://$trimmed';
  final parsed = Uri.tryParse(withScheme);
  if (parsed == null || parsed.host.isEmpty) {
    throw const FormatException('Enter a valid http(s) server URL.');
  }

  if (parsed.scheme != 'http' && parsed.scheme != 'https') {
    throw const FormatException('Duskcue server URLs must use http or https.');
  }

  if (networkMode == NetworkMode.exposed && parsed.scheme != 'https') {
    throw const FormatException('Exposed servers require HTTPS.');
  }

  if (parsed.hasPort && parsed.port == 48028) {
    throw const FormatException('Use the public Duskcue port 48027, not the internal API port 48028.');
  }

  if (parsed.hasPort && parsed.port != 48027) {
    throw const FormatException('Duskcue clients connect through port 48027.');
  }

  return Uri(
    scheme: parsed.scheme,
    host: parsed.host,
    port: 48027,
  );
}

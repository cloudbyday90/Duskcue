class ServerProfile {
  const ServerProfile({
    required this.origin,
    this.displayName,
    this.lastConnectedAt,
  });

  final Uri origin;
  final String? displayName;
  final DateTime? lastConnectedAt;

  Map<String, Object?> toJson() {
    return {
      'origin': origin.toString(),
      'display_name': displayName,
      'last_connected_at': lastConnectedAt?.toIso8601String(),
    };
  }

  static ServerProfile fromJson(Map<String, Object?> json) {
    return ServerProfile(
      origin: Uri.parse(json['origin'] as String),
      displayName: json['display_name'] as String?,
      lastConnectedAt: json['last_connected_at'] == null
          ? null
          : DateTime.parse(json['last_connected_at'] as String),
    );
  }
}

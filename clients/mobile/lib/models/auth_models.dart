class UserSummary {
  const UserSummary({
    required this.id,
    required this.username,
    required this.displayName,
    required this.role,
    required this.capabilities,
    required this.hasAllLibraryAccess,
  });

  final String id;
  final String username;
  final String displayName;
  final String role;
  final List<String> capabilities;
  final bool hasAllLibraryAccess;

  factory UserSummary.fromJson(Map<String, Object?> json) {
    return UserSummary(
      id: json['id'] as String? ?? '',
      username: json['username'] as String? ?? '',
      displayName: json['display_name'] as String? ?? '',
      role: json['role'] as String? ?? '',
      capabilities: (json['capabilities'] as List? ?? const []).whereType<String>().toList(growable: false),
      hasAllLibraryAccess: json['has_all_library_access'] as bool? ?? false,
    );
  }

  Map<String, Object?> toJson() {
    return {
      'id': id,
      'username': username,
      'display_name': displayName,
      'role': role,
      'capabilities': capabilities,
      'has_all_library_access': hasAllLibraryAccess,
    };
  }
}

class AuthSession {
  const AuthSession({
    required this.sessionToken,
    required this.user,
  });

  final String sessionToken;
  final UserSummary user;

  factory AuthSession.fromJson(Map<String, Object?> json) {
    return AuthSession(
      sessionToken: json['session_token'] as String? ?? '',
      user: UserSummary.fromJson(Map<String, Object?>.from(json['user'] as Map? ?? const {})),
    );
  }
}

class DeviceCode {
  const DeviceCode({
    required this.deviceCode,
    required this.userCode,
    required this.verificationUri,
    required this.expiresIn,
    required this.interval,
  });

  final String deviceCode;
  final String userCode;
  final String verificationUri;
  final int expiresIn;
  final int interval;

  factory DeviceCode.fromJson(Map<String, Object?> json) {
    return DeviceCode(
      deviceCode: json['device_code'] as String? ?? '',
      userCode: json['user_code'] as String? ?? '',
      verificationUri: json['verification_uri'] as String? ?? '',
      expiresIn: json['expires_in'] as int? ?? 0,
      interval: json['interval'] as int? ?? 5,
    );
  }
}

class SessionDetail {
  const SessionDetail({
    required this.id,
    required this.isSecure,
    required this.lastActiveAt,
    required this.createdAt,
    this.deviceName,
    this.clientName,
    this.clientVersion,
    this.clientPlatform,
    this.ipAddress,
  });

  final String id;
  final String? deviceName;
  final String? clientName;
  final String? clientVersion;
  final String? clientPlatform;
  final String? ipAddress;
  final bool isSecure;
  final DateTime lastActiveAt;
  final DateTime createdAt;

  factory SessionDetail.fromJson(Map<String, Object?> json) {
    return SessionDetail(
      id: json['id'] as String? ?? '',
      deviceName: json['device_name'] as String?,
      clientName: json['client_name'] as String?,
      clientVersion: json['client_version'] as String?,
      clientPlatform: json['client_platform'] as String?,
      ipAddress: json['ip_address'] as String?,
      isSecure: json['is_secure'] as bool? ?? false,
      lastActiveAt: DateTime.tryParse(json['last_active_at'] as String? ?? '') ?? DateTime.fromMillisecondsSinceEpoch(0),
      createdAt: DateTime.tryParse(json['created_at'] as String? ?? '') ?? DateTime.fromMillisecondsSinceEpoch(0),
    );
  }
}

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

class UserSummary {
  const UserSummary({
    required this.id,
    required this.username,
    required this.displayName,
    required this.role,
    required this.capabilities,
    required this.hasAllLibraryAccess,
    this.activeProfileId = '',
    this.profileSelectionRequired = false,
  });

  final String id;
  final String username;
  final String displayName;
  final String role;
  final List<String> capabilities;
  final bool hasAllLibraryAccess;
  final String activeProfileId;
  final bool profileSelectionRequired;

  factory UserSummary.fromJson(Map<String, Object?> json) {
    return UserSummary(
      id: json['id'] as String? ?? '',
      username: json['username'] as String? ?? '',
      displayName: json['display_name'] as String? ?? '',
      role: json['role'] as String? ?? '',
      capabilities: (json['capabilities'] as List? ?? const [])
          .whereType<String>()
          .toList(growable: false),
      hasAllLibraryAccess: json['has_all_library_access'] as bool? ?? false,
      activeProfileId: json['active_profile_id'] as String? ?? '',
      profileSelectionRequired:
          json['profile_selection_required'] as bool? ?? false,
    );
  }

  UserSummary copyWith({
    String? activeProfileId,
    bool? profileSelectionRequired,
  }) {
    return UserSummary(
      id: id,
      username: username,
      displayName: displayName,
      role: role,
      capabilities: capabilities,
      hasAllLibraryAccess: hasAllLibraryAccess,
      activeProfileId: activeProfileId ?? this.activeProfileId,
      profileSelectionRequired:
          profileSelectionRequired ?? this.profileSelectionRequired,
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
      'active_profile_id': activeProfileId,
      'profile_selection_required': profileSelectionRequired,
    };
  }
}

class AuthSession {
  const AuthSession({required this.sessionToken, required this.user});

  final String sessionToken;
  final UserSummary user;

  factory AuthSession.fromJson(Map<String, Object?> json) {
    return AuthSession(
      sessionToken: json['session_token'] as String? ?? '',
      user: UserSummary.fromJson(
        Map<String, Object?>.from(json['user'] as Map? ?? const {}),
      ),
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
    this.deviceId,
    this.deviceName,
    this.clientName,
    this.clientVersion,
    this.clientPlatform,
    this.ipAddress,
  });

  final String id;
  final String? deviceId;
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
      deviceId: json['device_id'] as String?,
      deviceName: json['device_name'] as String?,
      clientName: json['client_name'] as String?,
      clientVersion: json['client_version'] as String?,
      clientPlatform: json['client_platform'] as String?,
      ipAddress: json['ip_address'] as String?,
      isSecure: json['is_secure'] as bool? ?? false,
      lastActiveAt:
          DateTime.tryParse(json['last_active_at'] as String? ?? '') ??
          DateTime.fromMillisecondsSinceEpoch(0),
      createdAt:
          DateTime.tryParse(json['created_at'] as String? ?? '') ??
          DateTime.fromMillisecondsSinceEpoch(0),
    );
  }
}

class PasskeySummary {
  const PasskeySummary({
    required this.id,
    required this.name,
    required this.transports,
    required this.createdAt,
    this.lastUsedAt,
  });

  final String id;
  final String name;
  final List<String> transports;
  final DateTime createdAt;
  final DateTime? lastUsedAt;

  factory PasskeySummary.fromJson(Map<String, Object?> json) {
    return PasskeySummary(
      id: json['id'] as String? ?? '',
      name: json['name'] as String? ?? 'Passkey',
      transports: (json['transports'] as List? ?? const [])
          .whereType<String>()
          .toList(growable: false),
      createdAt:
          DateTime.tryParse(json['created_at'] as String? ?? '') ??
          DateTime.fromMillisecondsSinceEpoch(0),
      lastUsedAt: DateTime.tryParse(json['last_used_at'] as String? ?? ''),
    );
  }
}

class NotificationPreference {
  const NotificationPreference({
    required this.notificationTypeId,
    required this.name,
    required this.category,
    required this.priority,
    required this.inAppEnabled,
    required this.webhookEnabled,
    required this.pushEnabled,
    required this.isUsingDefaults,
  });

  final String notificationTypeId;
  final String name;
  final String category;
  final String priority;
  final bool inAppEnabled;
  final bool webhookEnabled;
  final bool pushEnabled;
  final bool isUsingDefaults;

  NotificationPreference copyWith({
    bool? inAppEnabled,
    bool? webhookEnabled,
    bool? pushEnabled,
    bool? isUsingDefaults,
  }) {
    return NotificationPreference(
      notificationTypeId: notificationTypeId,
      name: name,
      category: category,
      priority: priority,
      inAppEnabled: inAppEnabled ?? this.inAppEnabled,
      webhookEnabled: webhookEnabled ?? this.webhookEnabled,
      pushEnabled: pushEnabled ?? this.pushEnabled,
      isUsingDefaults: isUsingDefaults ?? this.isUsingDefaults,
    );
  }

  factory NotificationPreference.fromJson(Map<String, Object?> json) {
    return NotificationPreference(
      notificationTypeId: json['notification_type_id'] as String? ?? '',
      name: json['name'] as String? ?? 'notification',
      category: json['category'] as String? ?? '',
      priority: json['priority'] as String? ?? '',
      inAppEnabled: json['in_app_enabled'] as bool? ?? false,
      webhookEnabled: json['webhook_enabled'] as bool? ?? false,
      pushEnabled: json['push_enabled'] as bool? ?? false,
      isUsingDefaults: json['is_using_defaults'] as bool? ?? false,
    );
  }
}

class PushDeviceSummary {
  const PushDeviceSummary({
    required this.id,
    required this.provider,
    required this.tokenPreview,
    required this.isActive,
    required this.createdAt,
    required this.updatedAt,
    this.deviceName,
    this.platform,
    this.appVersion,
    this.lastSeenAt,
    this.invalidatedAt,
  });

  final String id;
  final String provider;
  final String tokenPreview;
  final bool isActive;
  final String? deviceName;
  final String? platform;
  final String? appVersion;
  final DateTime? lastSeenAt;
  final DateTime? invalidatedAt;
  final DateTime createdAt;
  final DateTime updatedAt;

  factory PushDeviceSummary.fromJson(Map<String, Object?> json) {
    return PushDeviceSummary(
      id: json['id'] as String? ?? '',
      provider: json['provider'] as String? ?? '',
      tokenPreview: json['token_preview'] as String? ?? '',
      deviceName: json['device_name'] as String?,
      platform: json['platform'] as String?,
      appVersion: json['app_version'] as String?,
      lastSeenAt: DateTime.tryParse(json['last_seen_at'] as String? ?? ''),
      isActive: json['is_active'] as bool? ?? false,
      invalidatedAt: DateTime.tryParse(json['invalidated_at'] as String? ?? ''),
      createdAt:
          DateTime.tryParse(json['created_at'] as String? ?? '') ??
          DateTime.fromMillisecondsSinceEpoch(0),
      updatedAt:
          DateTime.tryParse(json['updated_at'] as String? ?? '') ??
          DateTime.fromMillisecondsSinceEpoch(0),
    );
  }
}

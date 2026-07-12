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

import 'package:duskcue_mobile/models/auth_models.dart';
import 'package:duskcue_mobile/models/server_profile.dart';
import 'package:duskcue_mobile/services/api_client.dart';
import 'package:duskcue_mobile/services/device_identity_service.dart';
import 'package:duskcue_mobile/services/native_passkey_service.dart';
import 'package:duskcue_mobile/services/protected_download_storage_service.dart';
import 'package:duskcue_mobile/services/secure_storage_service.dart';

class AuthService {
  const AuthService({
    required DuskcueApiClient apiClient,
    required SecureStorageService storage,
    required DeviceIdentityService deviceIdentity,
    required NativePasskeyService passkeys,
    required ProtectedDownloadStorageService protectedDownloads,
  })  : _apiClient = apiClient,
        _storage = storage,
        _deviceIdentity = deviceIdentity,
        _passkeys = passkeys,
        _protectedDownloads = protectedDownloads;

  final DuskcueApiClient _apiClient;
  final SecureStorageService _storage;
  final DeviceIdentityService _deviceIdentity;
  final NativePasskeyService _passkeys;
  final ProtectedDownloadStorageService _protectedDownloads;

  Future<AuthSession?> restore(ServerProfile server) async {
    final token = await _storage.readToken();
    final userJson = await _storage.readUser();
    if (token == null || userJson == null) return null;

    _apiClient.configure(server.origin, bearerToken: token);
    await listSessions();

    final user = UserSummary.fromJson(Map<String, Object?>.from(jsonDecode(userJson) as Map));
    return AuthSession(sessionToken: token, user: user);
  }

  Future<AuthSession> loginWithPassword({
    required String username,
    required String password,
    required ServerProfile server,
  }) async {
    final body = await _authPayload(server);
    body['username'] = username;
    body['password'] = password;
    return _completeAuth(await _apiClient.post<Map<String, Object?>>('/api/v1/auth/login', body: body));
  }

  Future<AuthSession> loginWithInvite({
    required String code,
    required ServerProfile server,
  }) async {
    final body = await _authPayload(server);
    body['code'] = code;
    return _completeAuth(await _apiClient.post<Map<String, Object?>>('/api/v1/auth/invite', body: body));
  }

  Future<AuthSession> loginWithReauthCode({
    required String code,
    required ServerProfile server,
  }) async {
    final body = await _authPayload(server);
    body['code'] = code;
    return _completeAuth(await _apiClient.post<Map<String, Object?>>('/api/v1/auth/reauth', body: body));
  }

  Future<AuthSession> loginWithPasskey() async {
    final start = await _apiClient.post<Map<String, Object?>>('/api/v1/auth/webauthn/start', body: {});
    final data = Map<String, Object?>.from(start.data ?? const {});
    final challengeId = data['challenge_id'] as String? ?? '';
    final requestOptions = Map<String, Object?>.from(data['request_options'] as Map? ?? const {});
    final credential = await _passkeys.getCredential(requestOptions);
    final identity = await _deviceIdentity.current();
    return _completeAuth(
      await _apiClient.post<Map<String, Object?>>(
        '/api/v1/auth/webauthn/finish',
        body: {
          'credential': credential,
          ...identity.toAuthJson(),
        },
        headers: {'X-Challenge-Id': challengeId},
      ),
    );
  }

  Future<void> registerPasskey(String name) async {
    final start = await _apiClient.post<Map<String, Object?>>(
      '/api/v1/user/passkeys/register/start',
      body: {'name': name},
    );
    final data = Map<String, Object?>.from(start.data ?? const {});
    final challengeId = data['challenge_id'] as String? ?? '';
    final creationOptions = Map<String, Object?>.from(data['creation_options'] as Map? ?? const {});
    final credential = await _passkeys.createCredential(creationOptions);
    await _apiClient.post<Map<String, Object?>>(
      '/api/v1/user/passkeys/register/finish',
      body: {'credential': credential, 'name': name},
      headers: {'X-Challenge-Id': challengeId},
    );
  }

  Future<List<PasskeySummary>> listPasskeys() async {
    final response = await _apiClient.get<Map<String, Object?>>('/api/v1/user/passkeys');
    final items = (response.data?['items'] as List? ?? const []).whereType<Map>();
    return items.map((item) => PasskeySummary.fromJson(Map<String, Object?>.from(item))).toList(growable: false);
  }

  Future<void> deletePasskey(String passkeyId) async {
    await _apiClient.delete<Object?>('/api/v1/user/passkeys/$passkeyId');
  }

  Future<List<NotificationPreference>> listNotificationPreferences() async {
    final response = await _apiClient.get<Map<String, Object?>>('/api/v1/user/notification-preferences');
    final items = (response.data?['preferences'] as List? ?? const []).whereType<Map>();
    return items.map((item) => NotificationPreference.fromJson(Map<String, Object?>.from(item))).toList(growable: false);
  }

  Future<NotificationPreference> updateNotificationPreference(
    NotificationPreference preference, {
    bool? inAppEnabled,
    bool? webhookEnabled,
    bool? pushEnabled,
  }) async {
    final response = await _apiClient.put<Map<String, Object?>>(
      '/api/v1/user/notification-preferences/${preference.notificationTypeId}',
      body: {
        if (inAppEnabled != null) 'in_app_enabled': inAppEnabled,
        if (webhookEnabled != null) 'webhook_enabled': webhookEnabled,
        if (pushEnabled != null) 'push_enabled': pushEnabled,
      },
    );
    final data = Map<String, Object?>.from(response.data ?? const {});
    return preference.copyWith(
      inAppEnabled: data['in_app_enabled'] as bool?,
      webhookEnabled: data['webhook_enabled'] as bool?,
      pushEnabled: data['push_enabled'] as bool?,
      isUsingDefaults: false,
    );
  }

  Future<List<PushDeviceSummary>> listPushDevices() async {
    final response = await _apiClient.get<Map<String, Object?>>('/api/v1/user/push-devices');
    final items = (response.data?['devices'] as List? ?? const []).whereType<Map>();
    return items.map((item) => PushDeviceSummary.fromJson(Map<String, Object?>.from(item))).toList(growable: false);
  }

  Future<void> deletePushDevice(String deviceId) async {
    await _apiClient.delete<Object?>('/api/v1/user/push-devices/$deviceId');
  }

  Future<DeviceCode> createDeviceCode() async {
    final identity = await _deviceIdentity.current();
    final response = await _apiClient.post<Map<String, Object?>>(
      '/api/v1/device/code',
      body: identity.toAuthJson(),
    );
    return DeviceCode.fromJson(Map<String, Object?>.from(response.data ?? const {}));
  }

  Future<AuthSession> pollDeviceToken(String deviceCode) async {
    return _completeAuth(
      await _apiClient.post<Map<String, Object?>>(
        '/api/v1/device/token',
        body: {'device_code': deviceCode},
      ),
    );
  }

  Future<List<SessionDetail>> listSessions() async {
    final response = await _apiClient.get<Map<String, Object?>>('/api/v1/user/sessions');
    final items = (response.data?['items'] as List? ?? const []).whereType<Map>();
    return items.map((item) => SessionDetail.fromJson(Map<String, Object?>.from(item))).toList(growable: false);
  }

  Future<void> deleteSession(String sessionId) async {
    await _apiClient.delete<Object?>('/api/v1/user/sessions/$sessionId');
  }

  Future<void> logout() async {
    try {
      await _apiClient.post<Object?>('/api/v1/auth/logout');
    } finally {
      await clearLocalSession();
    }
  }

  Future<void> logoutAll() async {
    try {
      await _apiClient.post<Object?>('/api/v1/auth/logout-all');
    } finally {
      await clearLocalSession();
    }
  }

  Future<void> clearLocalSession() async {
    _apiClient.clearBearerToken();
    try {
      await _protectedDownloads.deleteAllProtectedDownloads();
    } catch (_) {}
    await _storage.clearSession();
    await _storage.clearDownloadState();
  }

  Future<Map<String, Object?>> _authPayload(ServerProfile server) async {
    final identity = await _deviceIdentity.current();
    return {
      ...identity.toAuthJson(),
      'server': server.origin.toString(),
    };
  }

  Future<AuthSession> _completeAuth(dynamic response) async {
    final data = Map<String, Object?>.from(response.data as Map? ?? const {});
    final session = AuthSession.fromJson(data);
    _apiClient.setBearerToken(session.sessionToken);
    await _storage.writeToken(session.sessionToken);
    await _storage.writeUser(jsonEncode(session.user.toJson()));
    return session;
  }
}

import 'dart:convert';

import 'package:duskcue_mobile/models/auth_models.dart';
import 'package:duskcue_mobile/models/server_profile.dart';
import 'package:duskcue_mobile/services/api_client.dart';
import 'package:duskcue_mobile/services/device_identity_service.dart';
import 'package:duskcue_mobile/services/native_passkey_service.dart';
import 'package:duskcue_mobile/services/secure_storage_service.dart';

class AuthService {
  const AuthService({
    required DuskcueApiClient apiClient,
    required SecureStorageService storage,
    required DeviceIdentityService deviceIdentity,
    required NativePasskeyService passkeys,
  })  : _apiClient = apiClient,
        _storage = storage,
        _deviceIdentity = deviceIdentity,
        _passkeys = passkeys;

  final DuskcueApiClient _apiClient;
  final SecureStorageService _storage;
  final DeviceIdentityService _deviceIdentity;
  final NativePasskeyService _passkeys;

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
      body: {'credential': credential},
      headers: {'X-Challenge-Id': challengeId},
    );
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
    await _storage.clearSession();
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

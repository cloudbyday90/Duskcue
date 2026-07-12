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

import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:duskcue_mobile/services/api_client.dart';
import 'package:duskcue_mobile/services/device_identity_service.dart';
import 'package:duskcue_mobile/services/secure_storage_service.dart';
import 'package:firebase_core/firebase_core.dart';
import 'package:firebase_messaging/firebase_messaging.dart';
import 'package:flutter/services.dart';

@pragma('vm:entry-point')
Future<void> duskcueFirebaseMessagingBackgroundHandler(RemoteMessage message) async {}

class PushNotificationTap {
  const PushNotificationTap({required this.route});

  final String route;
}

class PushRegistrationService {
  PushRegistrationService({
    required DuskcueApiClient apiClient,
    required SecureStorageService storage,
    required DeviceIdentityService deviceIdentity,
    FirebaseMessaging? messaging,
    MethodChannel? platformChannel,
  })  : _apiClient = apiClient,
        _storage = storage,
        _deviceIdentity = deviceIdentity,
        _messaging = messaging ?? FirebaseMessaging.instance,
        _platformChannel = platformChannel ?? const MethodChannel('duskcue/mobile_push');

  static const _heartbeatInterval = Duration(hours: 24);

  final DuskcueApiClient _apiClient;
  final SecureStorageService _storage;
  final DeviceIdentityService _deviceIdentity;
  final FirebaseMessaging _messaging;
  final MethodChannel _platformChannel;
  final StreamController<PushNotificationTap> _tapController = StreamController.broadcast();

  StreamSubscription<String>? _tokenRefreshSubscription;
  StreamSubscription<RemoteMessage>? _tapSubscription;
  Timer? _heartbeatTimer;
  bool _started = false;
  bool _registering = false;

  Stream<PushNotificationTap> get notificationTaps => _tapController.stream;

  Future<void> startOrRefresh() async {
    if (!_apiClient.isConfigured || _apiClient.bearerToken == null) return;
    if (!_started) {
      _tapSubscription = FirebaseMessaging.onMessageOpenedApp.listen(_handleNotificationTap);
      _tokenRefreshSubscription = _messaging.onTokenRefresh.listen((token) {
        unawaited(_registerToken(provider: 'fcm', token: token));
      });
      final initialMessage = await _messaging.getInitialMessage();
      if (initialMessage != null) {
        _handleNotificationTap(initialMessage);
      }
      _heartbeatTimer = Timer.periodic(_heartbeatInterval, (_) => unawaited(refreshHeartbeat()));
      _started = true;
    }

    await registerAvailableTokens();
  }

  Future<void> registerAvailableTokens() async {
    if (_registering) return;
    _registering = true;
    try {
      await _messaging.requestPermission(alert: true, badge: true, sound: true);
      final fcmToken = await _messaging.getToken();
      if (fcmToken != null && fcmToken.isNotEmpty) {
        await _registerToken(provider: 'fcm', token: fcmToken);
      }

      if (Platform.isIOS) {
        final apnsToken = await _messaging.getAPNSToken();
        if (apnsToken != null && apnsToken.isNotEmpty) {
          await _registerToken(provider: 'apns', token: apnsToken);
        }
      }

      if (Platform.isAndroid) {
        final unifiedPushEndpoint = await _optionalPlatformString('getUnifiedPushEndpoint');
        if (unifiedPushEndpoint != null && unifiedPushEndpoint.isNotEmpty) {
          await _registerToken(provider: 'unifiedpush', token: unifiedPushEndpoint);
        }
      }
    } on FirebaseException {
      // Missing Firebase app configuration or denied platform services should not block auth.
    } catch (_) {
      // Push is best-effort; auth, browsing, and foreground SSE still work without it.
    } finally {
      _registering = false;
    }
  }

  Future<void> refreshHeartbeat() async {
    if (!_apiClient.isConfigured || _apiClient.bearerToken == null) return;
    final ids = await _readDeviceIds();
    if (ids.isEmpty) {
      await registerAvailableTokens();
      return;
    }

    final identity = await _deviceIdentity.current();
    for (final entry in ids.entries) {
      try {
        await _apiClient.put<Map<String, Object?>>(
          '/api/v1/user/push-devices/${entry.value}',
          body: {
            'device_name': identity.deviceName,
            'platform': identity.clientPlatform,
            'app_version': identity.clientVersion,
          },
        );
      } catch (_) {
        await registerAvailableTokens();
        return;
      }
    }
  }

  Future<void> stop() async {
    _heartbeatTimer?.cancel();
    _heartbeatTimer = null;
    await _tokenRefreshSubscription?.cancel();
    await _tapSubscription?.cancel();
    _tokenRefreshSubscription = null;
    _tapSubscription = null;
    _started = false;
  }

  Future<void> dispose() async {
    await stop();
    await _tapController.close();
  }

  Future<void> _registerToken({required String provider, required String token}) async {
    if (!_apiClient.isConfigured || _apiClient.bearerToken == null) return;
    try {
      final identity = await _deviceIdentity.current();
      final response = await _apiClient.post<Map<String, Object?>>(
        '/api/v1/user/push-devices',
        body: {
          'provider': provider,
          'token': token,
          'device_name': identity.deviceName,
          'platform': identity.clientPlatform,
          'app_version': identity.clientVersion,
        },
      );
      final id = response.data?['id']?.toString();
      if (id == null || id.isEmpty) return;
      final ids = await _readDeviceIds();
      ids[provider] = id;
      await _writeDeviceIds(ids);
    } catch (_) {
      // Push registration is best-effort; foreground SSE and REST still cover active use.
    }
  }

  void _handleNotificationTap(RemoteMessage message) {
    final route = _routeFromMessage(message);
    if (route == null) return;
    _tapController.add(PushNotificationTap(route: route));
  }

  String? _routeFromMessage(RemoteMessage message) {
    final data = message.data;
    final link = (data['link'] ?? data['action_url'])?.toString();
    if (_isSafeInternalRoute(link)) return link;

    final relatedId = data['related_item_id']?.toString();
    if (_looksLikeUuid(relatedId)) {
      return '/media/$relatedId';
    }

    final notificationId = data['notification_id']?.toString();
    if (_looksLikeUuid(notificationId)) {
      return '/notifications';
    }

    return null;
  }

  bool _isSafeInternalRoute(String? value) {
    if (value == null || value.isEmpty) return false;
    if (!value.startsWith('/') || value.startsWith('//')) return false;
    final uri = Uri.tryParse(value);
    return uri != null && !uri.hasScheme && !uri.hasAuthority;
  }

  bool _looksLikeUuid(String? value) {
    if (value == null) return false;
    return RegExp(r'^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$').hasMatch(value);
  }

  Future<String?> _optionalPlatformString(String method) async {
    try {
      return await _platformChannel.invokeMethod<String>(method);
    } on MissingPluginException {
      return null;
    } on PlatformException {
      return null;
    }
  }

  Future<Map<String, String>> _readDeviceIds() async {
    try {
      final raw = await _storage.readPushDeviceIds();
      if (raw == null || raw.isEmpty) return <String, String>{};
      final decoded = jsonDecode(raw);
      if (decoded is! Map) return <String, String>{};
      return decoded.map((key, value) => MapEntry(key.toString(), value.toString()));
    } catch (_) {
      // Corrupt local push metadata should recover through normal re-registration.
      return <String, String>{};
    }
  }

  Future<void> _writeDeviceIds(Map<String, String> ids) {
    return _storage.writePushDeviceIds(jsonEncode(ids));
  }
}

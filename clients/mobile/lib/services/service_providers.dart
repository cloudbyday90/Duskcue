import 'dart:async';

import 'package:duskcue_mobile/services/auth_service.dart';
import 'package:duskcue_mobile/services/api_client.dart';
import 'package:duskcue_mobile/services/content_service.dart';
import 'package:duskcue_mobile/services/connectivity_service.dart';
import 'package:duskcue_mobile/services/device_identity_service.dart';
import 'package:duskcue_mobile/services/native_passkey_service.dart';
import 'package:duskcue_mobile/services/playback_service.dart';
import 'package:duskcue_mobile/services/push_registration_service.dart';
import 'package:duskcue_mobile/services/quality_service.dart';
import 'package:duskcue_mobile/services/realtime_service.dart';
import 'package:duskcue_mobile/services/secure_storage_service.dart';
import 'package:duskcue_mobile/services/server_repository.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

final secureStorageProvider = Provider<SecureStorageService>((ref) {
  return const SecureStorageService();
});

final apiClientProvider = Provider<DuskcueApiClient>((ref) {
  return DuskcueApiClient();
});

final serverRepositoryProvider = Provider<ServerRepository>((ref) {
  return ServerRepository(ref.watch(secureStorageProvider));
});

final deviceIdentityProvider = Provider<DeviceIdentityService>((ref) {
  return DeviceIdentityService(ref.watch(secureStorageProvider));
});

final connectivityServiceProvider = Provider<ConnectivityService>((ref) {
  return ConnectivityService();
});

final nativePasskeyProvider = Provider<NativePasskeyService>((ref) {
  return const NativePasskeyService();
});

final authServiceProvider = Provider<AuthService>((ref) {
  return AuthService(
    apiClient: ref.watch(apiClientProvider),
    storage: ref.watch(secureStorageProvider),
    deviceIdentity: ref.watch(deviceIdentityProvider),
    passkeys: ref.watch(nativePasskeyProvider),
  );
});

final contentServiceProvider = Provider<ContentService>((ref) {
  return ContentService(ref.watch(apiClientProvider));
});

final playbackServiceProvider = Provider<PlaybackService>((ref) {
  return PlaybackService(ref.watch(apiClientProvider));
});

final pushRegistrationServiceProvider = Provider<PushRegistrationService>((ref) {
  final service = PushRegistrationService(
    apiClient: ref.watch(apiClientProvider),
    storage: ref.watch(secureStorageProvider),
    deviceIdentity: ref.watch(deviceIdentityProvider),
  );
  ref.onDispose(() {
    unawaited(service.dispose());
  });
  return service;
});

final qualityServiceProvider = Provider<QualityService>((ref) {
  return QualityService(
    apiClient: ref.watch(apiClientProvider),
    storage: ref.watch(secureStorageProvider),
    deviceIdentity: ref.watch(deviceIdentityProvider),
    connectivity: ref.watch(connectivityServiceProvider),
  );
});

final realtimeServiceProvider = Provider<RealtimeService>((ref) {
  final service = RealtimeService(ref.watch(apiClientProvider));
  ref.onDispose(() {
    unawaited(service.disconnect());
  });
  return service;
});

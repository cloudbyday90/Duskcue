import 'dart:async';

import 'package:duskcue_mobile/services/auth_service.dart';
import 'package:duskcue_mobile/services/api_client.dart';
import 'package:duskcue_mobile/services/content_service.dart';
import 'package:duskcue_mobile/services/device_identity_service.dart';
import 'package:duskcue_mobile/services/native_passkey_service.dart';
import 'package:duskcue_mobile/services/playback_service.dart';
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

final realtimeServiceProvider = Provider<RealtimeService>((ref) {
  final service = RealtimeService(ref.watch(apiClientProvider));
  ref.onDispose(() {
    unawaited(service.disconnect());
  });
  return service;
});

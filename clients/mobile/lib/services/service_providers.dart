import 'package:duskcue_mobile/services/api_client.dart';
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

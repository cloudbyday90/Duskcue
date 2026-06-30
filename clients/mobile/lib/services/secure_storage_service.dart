import 'package:flutter_secure_storage/flutter_secure_storage.dart';

class SecureStorageService {
  const SecureStorageService({FlutterSecureStorage? storage})
      : _storage = storage ?? const FlutterSecureStorage();

  final FlutterSecureStorage _storage;

  Future<void> writeToken(String token) {
    return _storage.write(key: 'session_token', value: token);
  }

  Future<String?> readToken() {
    return _storage.read(key: 'session_token');
  }

  Future<void> clearToken() {
    return _storage.delete(key: 'session_token');
  }

  Future<void> writeSavedServers(String value) {
    return _storage.write(key: 'saved_servers', value: value);
  }

  Future<String?> readSavedServers() {
    return _storage.read(key: 'saved_servers');
  }

  Future<void> writeLastServerOrigin(String origin) {
    return _storage.write(key: 'last_server_origin', value: origin);
  }

  Future<String?> readLastServerOrigin() {
    return _storage.read(key: 'last_server_origin');
  }
}

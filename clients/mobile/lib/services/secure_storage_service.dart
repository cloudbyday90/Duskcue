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

  Future<void> writeUser(String value) {
    return _storage.write(key: 'session_user', value: value);
  }

  Future<String?> readUser() {
    return _storage.read(key: 'session_user');
  }

  Future<void> clearUser() {
    return _storage.delete(key: 'session_user');
  }

  Future<void> writeDeviceIdentifier(String value) {
    return _storage.write(key: 'device_identifier', value: value);
  }

  Future<String?> readDeviceIdentifier() {
    return _storage.read(key: 'device_identifier');
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

  Future<void> writePushDeviceIds(String value) {
    return _storage.write(key: 'push_device_ids', value: value);
  }

  Future<String?> readPushDeviceIds() {
    return _storage.read(key: 'push_device_ids');
  }

  Future<void> clearPushDeviceIds() {
    return _storage.delete(key: 'push_device_ids');
  }

  Future<void> writeDownloadInventory(String value) {
    return _storage.write(key: 'download_inventory', value: value);
  }

  Future<String?> readDownloadInventory() {
    return _storage.read(key: 'download_inventory');
  }

  Future<void> writeDownloadSettings(String value) {
    return _storage.write(key: 'download_settings', value: value);
  }

  Future<String?> readDownloadSettings() {
    return _storage.read(key: 'download_settings');
  }

  Future<void> writeQualityPreferences(String value) {
    return _storage.write(key: 'quality_preferences', value: value);
  }

  Future<String?> readQualityPreferences() {
    return _storage.read(key: 'quality_preferences');
  }

  Future<void> clearSession() async {
    await clearToken();
    await clearUser();
  }
}

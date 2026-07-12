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

  Future<void> clearDownloadState() async {
    await _storage.delete(key: 'download_inventory');
    await _storage.delete(key: 'download_settings');
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

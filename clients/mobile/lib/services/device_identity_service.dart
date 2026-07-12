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

import 'dart:io';
import 'dart:math';

import 'package:duskcue_mobile/services/secure_storage_service.dart';

class DeviceIdentity {
  const DeviceIdentity({
    required this.deviceId,
    required this.deviceName,
    required this.clientName,
    required this.clientVersion,
    required this.clientPlatform,
  });

  final String deviceId;
  final String deviceName;
  final String clientName;
  final String clientVersion;
  final String clientPlatform;

  Map<String, Object?> toAuthJson() {
    return {
      'device_id': deviceId,
      'device_name': deviceName,
      'client_name': clientName,
      'client_version': clientVersion,
      'client_platform': clientPlatform,
    };
  }
}

class DeviceIdentityService {
  const DeviceIdentityService(this._storage);

  static const clientName = 'Duskcue Mobile';
  static const clientVersion = '0.1.0';

  final SecureStorageService _storage;

  Future<DeviceIdentity> current() async {
    final deviceId = await _readOrCreateDeviceId();
    final platform = _platformName();
    return DeviceIdentity(
      deviceId: deviceId,
      deviceName: 'Duskcue $platform',
      clientName: clientName,
      clientVersion: clientVersion,
      clientPlatform: platform.toLowerCase(),
    );
  }

  Future<String> _readOrCreateDeviceId() async {
    final existing = await _storage.readDeviceIdentifier();
    if (existing != null && existing.isNotEmpty) return existing;

    final bytes = List<int>.generate(16, (_) => Random.secure().nextInt(256));
    final value = 'duskcue-mobile-${bytes.map((b) => b.toRadixString(16).padLeft(2, '0')).join()}';
    await _storage.writeDeviceIdentifier(value);
    return value;
  }

  String _platformName() {
    if (Platform.isAndroid) return 'Android';
    if (Platform.isIOS) return 'iOS';
    return Platform.operatingSystem;
  }
}

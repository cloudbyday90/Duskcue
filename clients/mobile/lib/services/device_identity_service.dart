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

import 'package:flutter/services.dart';

class NativePasskeyService {
  const NativePasskeyService();

  static const MethodChannel _channel = MethodChannel('com.duskcue.mobile/passkeys');

  Future<Map<String, Object?>> getCredential(Map<String, Object?> requestOptions) async {
    final result = await _channel.invokeMapMethod<String, Object?>(
      'getCredential',
      {'request_options': requestOptions},
    );
    if (result == null) {
      throw UnsupportedError('Passkeys are not available on this device.');
    }
    return result;
  }

  Future<Map<String, Object?>> createCredential(Map<String, Object?> creationOptions) async {
    final result = await _channel.invokeMapMethod<String, Object?>(
      'createCredential',
      {'creation_options': creationOptions},
    );
    if (result == null) {
      throw UnsupportedError('Passkey registration is not available on this device.');
    }
    return result;
  }
}

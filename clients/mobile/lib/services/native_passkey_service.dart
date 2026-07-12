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

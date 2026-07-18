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

import 'package:duskcue_mobile/models/profile_models.dart';
import 'package:duskcue_mobile/services/api_client.dart';
import 'package:flutter/services.dart';

class AmbientPlaybackService {
  AmbientPlaybackService({
    required DuskcueApiClient apiClient,
    MethodChannel? channel,
  }) : _apiClient = apiClient,
       _channel = channel ?? const MethodChannel('duskcue/ambient_player');

  final DuskcueApiClient _apiClient;
  final MethodChannel _channel;

  Future<void> start(AmbientChannelSummary channel) async {
    final origin = _apiClient.serverOrigin;
    final token = _apiClient.bearerToken;
    if (origin == null || token == null || token.isEmpty) {
      throw StateError(
        'An authenticated server session is required for ambient playback.',
      );
    }
    await _channel.invokeMethod<void>('start', {
      'server_origin': origin.toString(),
      'bearer_token': token,
      'channel_id': channel.id,
      'channel_name': channel.name,
    });
  }

  Future<void> stop() {
    return _channel.invokeMethod<void>('stop');
  }

  Future<void> clear() {
    return _channel.invokeMethod<void>('clear');
  }

  Future<NativeAmbientPlaybackStatus> status() async {
    final data = await _channel.invokeMapMethod<Object?, Object?>('status');
    return NativeAmbientPlaybackStatus.fromJson(data ?? const {});
  }
}

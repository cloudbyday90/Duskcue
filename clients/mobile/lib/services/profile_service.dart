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

class ProfileService {
  const ProfileService(this._apiClient);

  final DuskcueApiClient _apiClient;

  Future<ProfileListResponse> listProfiles() async {
    final response = await _apiClient.get<Map<String, Object?>>(
      '/api/v1/profiles',
    );
    return ProfileListResponse.fromJson(
      Map<String, Object?>.from(response.data ?? const {}),
    );
  }

  Future<SwitchProfileResponse> switchProfile(
    String profileId, {
    bool? rememberOnDevice,
  }) async {
    final response = await _apiClient.post<Map<String, Object?>>(
      '/api/v1/profiles/$profileId/switch',
      body: {
        if (rememberOnDevice != null) 'remember_on_device': rememberOnDevice,
      },
    );
    return SwitchProfileResponse.fromJson(
      Map<String, Object?>.from(response.data ?? const {}),
    );
  }

  Future<ParentUnlockResponse> unlockParentProfile(String pin) async {
    final response = await _apiClient.post<Map<String, Object?>>(
      '/api/v1/profiles/parent-unlock',
      body: {'pin': pin},
    );
    return ParentUnlockResponse.fromJson(
      Map<String, Object?>.from(response.data ?? const {}),
    );
  }

  Future<AmbientChannelListResponse> listAmbientChannels() async {
    final response = await _apiClient.get<Map<String, Object?>>(
      '/api/v1/ambient-channels',
    );
    return AmbientChannelListResponse.fromJson(
      Map<String, Object?>.from(response.data ?? const {}),
    );
  }
}

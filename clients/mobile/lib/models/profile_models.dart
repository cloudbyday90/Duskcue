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

class ProfileSummary {
  const ProfileSummary({
    required this.id,
    required this.name,
    required this.profileType,
    required this.isDefault,
    required this.parentPinConfigured,
    this.avatar,
  });

  final String id;
  final String name;
  final String profileType;
  final bool isDefault;
  final bool parentPinConfigured;
  final String? avatar;

  bool get isKids => profileType == 'kids';

  factory ProfileSummary.fromJson(Map<String, Object?> json) {
    return ProfileSummary(
      id: json['id'] as String? ?? '',
      name: json['name'] as String? ?? 'Profile',
      avatar: json['avatar'] as String?,
      profileType: json['profile_type'] as String? ?? 'standard',
      isDefault: json['is_default'] as bool? ?? false,
      parentPinConfigured: json['parent_pin_configured'] as bool? ?? false,
    );
  }
}

class ProfileListResponse {
  const ProfileListResponse({
    required this.activeProfileId,
    required this.profileSelectionRequired,
    required this.deviceCanRememberProfile,
    required this.parentUnlockRequired,
    required this.items,
    this.rememberedProfileId,
  });

  final String activeProfileId;
  final bool profileSelectionRequired;
  final String? rememberedProfileId;
  final bool deviceCanRememberProfile;
  final bool parentUnlockRequired;
  final List<ProfileSummary> items;

  ProfileSummary? get activeProfile {
    for (final profile in items) {
      if (profile.id == activeProfileId) return profile;
    }
    return null;
  }

  factory ProfileListResponse.fromJson(Map<String, Object?> json) {
    final rawItems = (json['items'] as List? ?? const []).whereType<Map>();
    return ProfileListResponse(
      activeProfileId: json['active_profile_id'] as String? ?? '',
      profileSelectionRequired:
          json['profile_selection_required'] as bool? ?? false,
      rememberedProfileId: json['remembered_profile_id'] as String?,
      deviceCanRememberProfile:
          json['device_can_remember_profile'] as bool? ?? false,
      parentUnlockRequired: json['parent_unlock_required'] as bool? ?? false,
      items: rawItems
          .map(
            (item) => ProfileSummary.fromJson(Map<String, Object?>.from(item)),
          )
          .toList(growable: false),
    );
  }
}

class SwitchProfileResponse {
  const SwitchProfileResponse({
    required this.activeProfile,
    required this.profileSelectionRequired,
    required this.deviceCanRememberProfile,
    required this.parentUnlockRequired,
    this.rememberedProfileId,
  });

  final ProfileSummary activeProfile;
  final bool profileSelectionRequired;
  final String? rememberedProfileId;
  final bool deviceCanRememberProfile;
  final bool parentUnlockRequired;

  factory SwitchProfileResponse.fromJson(Map<String, Object?> json) {
    return SwitchProfileResponse(
      activeProfile: ProfileSummary.fromJson(
        Map<String, Object?>.from(json['active_profile'] as Map? ?? const {}),
      ),
      profileSelectionRequired:
          json['profile_selection_required'] as bool? ?? false,
      rememberedProfileId: json['remembered_profile_id'] as String?,
      deviceCanRememberProfile:
          json['device_can_remember_profile'] as bool? ?? false,
      parentUnlockRequired: json['parent_unlock_required'] as bool? ?? false,
    );
  }
}

class ParentUnlockResponse {
  const ParentUnlockResponse({required this.unlockedUntil});

  final DateTime unlockedUntil;

  factory ParentUnlockResponse.fromJson(Map<String, Object?> json) {
    return ParentUnlockResponse(
      unlockedUntil:
          DateTime.tryParse(json['unlocked_until'] as String? ?? '') ??
          DateTime.fromMillisecondsSinceEpoch(0),
    );
  }
}

class AmbientChannelSummary {
  const AmbientChannelSummary({
    required this.id,
    required this.name,
    required this.audience,
    required this.isEnabled,
    required this.itemCount,
  });

  final String id;
  final String name;
  final String audience;
  final bool isEnabled;
  final int itemCount;

  bool get isKids => audience == 'kids';
  bool get isPlayable => isEnabled && itemCount > 0;

  factory AmbientChannelSummary.fromJson(Map<String, Object?> json) {
    final itemCount = json['item_count'];
    return AmbientChannelSummary(
      id: json['id'] as String? ?? '',
      name: json['name'] as String? ?? 'Ambient channel',
      audience: json['audience'] as String? ?? 'standard',
      isEnabled: json['is_enabled'] as bool? ?? false,
      itemCount: itemCount is num
          ? itemCount.toInt()
          : int.tryParse(itemCount?.toString() ?? '') ?? 0,
    );
  }
}

class AmbientChannelListResponse {
  const AmbientChannelListResponse({required this.items});

  final List<AmbientChannelSummary> items;

  factory AmbientChannelListResponse.fromJson(Map<String, Object?> json) {
    final rawItems = (json['items'] as List? ?? const []).whereType<Map>();
    return AmbientChannelListResponse(
      items: rawItems
          .map(
            (item) =>
                AmbientChannelSummary.fromJson(Map<String, Object?>.from(item)),
          )
          .toList(growable: false),
    );
  }
}

class NativeAmbientPlaybackStatus {
  const NativeAmbientPlaybackStatus({
    required this.isActive,
    this.channelId,
    this.channelName,
    this.mediaItemId,
    this.positionMs = 0,
    this.isPlaying = false,
    this.isBuffering = false,
    this.error,
    this.protocol = 'legacy',
    this.state = 'idle',
    this.busy = false,
    this.cleanupPending = false,
    this.requiresPlay = false,
    this.durationMs = 0,
    this.requestId,
    this.outputRevision,
    this.videoFrames = 0,
  });

  final bool isActive;
  final String? channelId;
  final String? channelName;
  final String? mediaItemId;
  final int positionMs;
  final bool isPlaying;
  final bool isBuffering;
  final String? error;
  final String protocol, state;
  final bool busy, cleanupPending, requiresPlay;
  final int durationMs, videoFrames;
  final String? requestId, outputRevision;

  factory NativeAmbientPlaybackStatus.fromJson(Map<Object?, Object?> json) {
    if (json.length > 32 || json.values.any((value) => value is String && value.length > 256)) {
      throw const FormatException('Ambient status exceeds its limits.');
    }
    int bounded(Object? value) {
      if (value == null) return 0;
      if (value is! int || value < 0 || value > 2147483647) throw const FormatException('Invalid ambient position.');
      return value;
    }
    bool flag(String key) {
      final value = json[key];
      if (value == null) return false;
      if (value is! bool) throw const FormatException('Invalid ambient status flag.');
      return value;
    }
    final position = json['position_ms'];
    final protocol = json['protocol'] as String? ?? 'legacy';
    final state = json['state'] as String? ?? (json['is_active'] == true ? json['is_playing'] == true ? 'ready' : 'preparing' : 'idle');
    if (protocol != 'legacy' && protocol != 'duskcue-vod-command/2' ||
        !const {'idle', 'checking', 'starting', 'preparing', 'ready', 'changing', 'uncertain', 'stopping', 'cleanup',
          'settled', 'unavailable', 'unknown', 'released', 'failed', 'withdrawing', 'withdrawn', 'cleanup_uncertain'}.contains(state)) {
      throw const FormatException('Invalid ambient state.');
    }
    return NativeAmbientPlaybackStatus(
      isActive: flag('is_active'),
      channelId: json['channel_id'] as String?,
      channelName: json['channel_name'] as String?,
      mediaItemId: json['media_item_id'] as String?,
      positionMs: bounded(position),
      isPlaying: flag('is_playing'),
      isBuffering: flag('is_buffering'),
      error: json['error'] as String?,
      protocol: protocol,
      state: state,
      busy: flag('busy'),
      cleanupPending: flag('cleanup_pending'),
      requiresPlay: flag('requires_play'),
      durationMs: bounded(json['duration_ms']),
      requestId: json['request_id'] as String?,
      outputRevision: json['output_revision'] as String?,
      videoFrames: bounded(json['video_frames']),
    );
  }
}

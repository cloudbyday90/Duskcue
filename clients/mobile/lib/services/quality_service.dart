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

import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:connectivity_plus/connectivity_plus.dart';
import 'package:duskcue_mobile/services/api_client.dart';
import 'package:duskcue_mobile/services/connectivity_service.dart';
import 'package:duskcue_mobile/services/device_identity_service.dart';
import 'package:duskcue_mobile/services/secure_storage_service.dart';

enum QualityMode {
  auto('auto', null),
  maximum('maximum', null),
  manual480p('manual', 1500000),
  manual720p('manual', 3000000),
  manual1080p('manual', 6000000),
  manual4k('manual', 15000000);

  const QualityMode(this.apiValue, this.maxStreamingBitrate);

  final String apiValue;
  final int? maxStreamingBitrate;

  String get label {
    return switch (this) {
      QualityMode.auto => 'Auto',
      QualityMode.maximum => 'Maximum',
      QualityMode.manual480p => 'Manual 480p',
      QualityMode.manual720p => 'Manual 720p',
      QualityMode.manual1080p => 'Manual 1080p',
      QualityMode.manual4k => 'Manual 4K',
    };
  }

  static QualityMode fromName(String? value) {
    return QualityMode.values.firstWhere(
      (mode) => mode.name == value,
      orElse: () => QualityMode.auto,
    );
  }
}

class QualitySelection {
  const QualitySelection({required this.mode});

  final QualityMode mode;

  Map<String, Object?> toPlaybackJson() {
    return {
      'quality_mode': mode.apiValue,
      if (mode.maxStreamingBitrate != null) 'max_streaming_bitrate': mode.maxStreamingBitrate,
    };
  }
}

class QualityService {
  QualityService({
    required DuskcueApiClient apiClient,
    required SecureStorageService storage,
    required DeviceIdentityService deviceIdentity,
    required ConnectivityService connectivity,
  })  : _apiClient = apiClient,
        _storage = storage,
        _deviceIdentity = deviceIdentity,
        _connectivity = connectivity;

  final DuskcueApiClient _apiClient;
  final SecureStorageService _storage;
  final DeviceIdentityService _deviceIdentity;
  final ConnectivityService _connectivity;
  String? _lastCapabilityReportKey;

  Future<void> reportCapabilities() async {
    if (!_apiClient.isConfigured || _apiClient.bearerToken == null) return;
    final identity = await _deviceIdentity.current();
    final profile = await mobileDeviceProfile();
    final reportKey = '${identity.deviceId}:${identity.clientVersion}:${Platform.operatingSystemVersion}';
    if (_lastCapabilityReportKey == reportKey) return;

    try {
      await _apiClient.post<Map<String, Object?>>(
        '/api/v1/device/capabilities',
        body: {
          'device_identifier': identity.deviceId,
          'platform': identity.clientPlatform,
          'model': identity.deviceName,
          'os_version': Platform.operatingSystemVersion,
          'client_name': identity.clientName,
          'client_version': identity.clientVersion,
          'video_codecs': profile['video_codecs'],
          'audio_codecs': profile['audio_codecs'],
          'subtitle_formats': profile['subtitle_formats'],
          'containers': profile['containers'],
          'max_resolution': profile['max_resolution'],
          'max_framerate': profile['max_framerate'],
          'hdr_support': profile['hdr_formats'],
          'max_audio_channels': profile['max_audio_channels'],
          'spatial_audio': profile['spatial_audio'],
          'max_bitrate_bps': profile['max_bitrate_bps'],
          'allow_client_side_dv_fallback': profile['allow_client_side_dv_fallback'],
        },
      );
      _lastCapabilityReportKey = reportKey;
    } catch (_) {
      // Best-effort. Playback falls back to conservative server defaults.
    }
  }

  Future<Map<String, Object?>> mobileDeviceProfile() async {
    final identity = await _deviceIdentity.current();
    final isIos = Platform.isIOS;
    return {
      'client': 'duskcue_mobile',
      'device_identifier': identity.deviceId,
      'platform': identity.clientPlatform,
      'client_version': identity.clientVersion,
      'video_codecs': [
        'h264',
        if (isIos) 'hevc',
      ],
      'audio_codecs': ['aac', 'mp3', 'opus'],
      'subtitle_formats': ['webvtt', 'srt'],
      'containers': ['mp4', 'm4v', 'hls'],
      'max_resolution': isIos ? '4k' : '1080p',
      'max_framerate': 60,
      'hdr_formats': isIos ? ['hdr10'] : <String>[],
      'max_audio_channels': isIos ? 6 : 2,
      'spatial_audio': false,
      'max_bitrate_bps': isIos ? 25000000 : 12000000,
      'supports_dolby_vision': false,
      'allow_client_side_dv_fallback': true,
      'max_video_bit_depth': isIos ? 10 : 8,
    };
  }

  Future<QualitySelection> selectionForItem(String itemId) async {
    final values = await _readQualityPreferences();
    final modeName = values[itemId] ?? values['_default'];
    return QualitySelection(mode: QualityMode.fromName(modeName));
  }

  Future<QualitySelection> defaultSelection() async {
    final values = await _readQualityPreferences();
    return QualitySelection(mode: QualityMode.fromName(values['_default']));
  }

  Future<void> saveDefaultSelection(QualityMode mode) async {
    final values = await _readQualityPreferences();
    values['_default'] = mode.name;
    await _storage.writeQualityPreferences(jsonEncode(values));
  }

  Future<void> saveSelectionForItem(String itemId, QualityMode mode) async {
    final values = await _readQualityPreferences();
    values[itemId] = mode.name;
    await _storage.writeQualityPreferences(jsonEncode(values));
  }

  Future<void> submitSegmentTelemetry({
    required String sessionId,
    required int sampleIndex,
    required String rung,
    required int rebufferCount,
    required int rebufferTotalMs,
  }) async {
    try {
      await _apiClient.post<Map<String, Object?>>(
        '/api/v1/playback/telemetry',
        body: {
          'session_id': sessionId,
          'segment_index': sampleIndex,
          'rung': rung,
          'rebuffer_count': rebufferCount,
          'rebuffer_total_ms': rebufferTotalMs,
        },
      );
    } catch (_) {
      // Telemetry is advisory; playback must not fail if reporting is unavailable.
    }
  }

  Future<void> submitQoeReport({
    required String sessionId,
    int? startupTimeMs,
    double? rebufferRatio,
    int? averageBitrateBps,
    double? switchesPerMinute,
    int? qualityDrops,
    String? currentRung,
    double? currentBufferSeconds,
  }) async {
    try {
      await _apiClient.post<Map<String, Object?>>(
        '/api/v1/playback/qoe',
        body: {
          'session_id': sessionId,
          if (startupTimeMs != null) 'startup_time_ms': startupTimeMs,
          if (rebufferRatio != null) 'rebuffer_ratio': rebufferRatio,
          if (averageBitrateBps != null) 'average_bitrate_bps': averageBitrateBps,
          if (switchesPerMinute != null) 'switches_per_minute': switchesPerMinute,
          if (qualityDrops != null) 'quality_drops': qualityDrops,
          if (currentRung != null) 'current_rung': currentRung,
          if (currentBufferSeconds != null) 'current_buffer_seconds': currentBufferSeconds,
        },
      );
    } catch (_) {
      // QoE is advisory; playback must not fail if reporting is unavailable.
    }
  }

  Future<int?> runBandwidthProbe({
    required String sessionId,
    bool allowCellular = false,
  }) async {
    final connectivity = await _connectivity.current();
    if (!allowCellular && connectivity.contains(ConnectivityResult.mobile)) {
      return null;
    }

    try {
      final started = DateTime.now();
      final response = await _apiClient.getBytes(
        '/api/v1/probe/bandwidth',
        query: {'t': started.millisecondsSinceEpoch},
      );
      final elapsedMs = DateTime.now().difference(started).inMilliseconds.clamp(1, 1 << 31).toInt();
      final bytes = response.data?.length ?? 0;
      if (bytes <= 0) return null;
      final throughput = ((bytes * 8 * 1000) / elapsedMs).round();
      await _apiClient.post<Map<String, Object?>>(
        '/api/v1/probe/bandwidth/result',
        body: {
          'session_id': sessionId,
          'probe_bytes': bytes,
          'download_ms': elapsedMs,
          'estimated_throughput_bps': throughput,
        },
      );
      return throughput;
    } catch (_) {
      return null;
    }
  }

  Future<Map<String, String>> _readQualityPreferences() async {
    try {
      final raw = await _storage.readQualityPreferences();
      if (raw == null || raw.isEmpty) return <String, String>{};
      final decoded = jsonDecode(raw);
      if (decoded is! Map) return <String, String>{};
      return decoded.map((key, value) => MapEntry(key.toString(), value.toString()));
    } catch (_) {
      return <String, String>{};
    }
  }
}

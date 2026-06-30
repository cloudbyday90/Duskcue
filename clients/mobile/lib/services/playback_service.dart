import 'package:duskcue_mobile/models/playback_models.dart';
import 'package:duskcue_mobile/services/api_client.dart';
import 'package:video_player/video_player.dart';

class PlaybackService {
  const PlaybackService(this._apiClient);

  final DuskcueApiClient _apiClient;

  Future<PlaybackStart> startPlayback({
    required String mediaItemId,
    String? mediaFileId,
    int? audioStreamIndex,
    int? subtitleStreamIndex,
    int? maxStreamingBitrate,
    String? qualityMode,
    Map<String, Object?>? deviceProfile,
    bool forceTranscode = false,
  }) async {
    final response = await _apiClient.post<Map<String, Object?>>(
      '/api/v1/playback/start',
      body: {
        'media_item_id': mediaItemId,
        if (mediaFileId != null) 'media_file_id': mediaFileId,
        if (audioStreamIndex != null) 'audio_stream_index': audioStreamIndex,
        if (subtitleStreamIndex != null) 'subtitle_stream_index': subtitleStreamIndex,
        if (maxStreamingBitrate != null) 'max_streaming_bitrate': maxStreamingBitrate,
        if (qualityMode != null) 'quality_mode': qualityMode,
        'force_transcode': forceTranscode,
        'device_profile': deviceProfile ?? _mobileDeviceProfile,
      },
    );
    return PlaybackStart.fromJson(Map<String, Object?>.from(response.data ?? const {}));
  }

  Future<WatchData> getWatchData(String itemId) async {
    final response = await _apiClient.get<Map<String, Object?>>('/api/v1/items/$itemId/watch-data');
    return WatchData.fromJson(Map<String, Object?>.from(response.data ?? const {}));
  }

  Future<List<AudioTrack>> listAudioTracks(String itemId) async {
    final response = await _apiClient.get<Map<String, Object?>>('/api/v1/media-items/$itemId/files');
    final data = Map<String, Object?>.from(response.data ?? const {});
    final files = (data['items'] as List? ?? const []).whereType<Map>();
    final tracks = <AudioTrack>[];
    for (final file in files) {
      final additional = Map<String, Object?>.from((file['additional_streams'] as Map?) ?? const {});
      final audio = ((additional['audio'] as List?) ?? (additional['audio_streams'] as List?) ?? const []).whereType<Map>();
      for (final track in audio) {
        tracks.add(AudioTrack.fromJson(Map<String, Object?>.from(track)));
      }
    }
    return tracks;
  }

  Future<List<SubtitleTrack>> listSubtitles(String itemId) async {
    final response = await _apiClient.get<Map<String, Object?>>('/api/v1/items/$itemId/subtitles');
    final data = Map<String, Object?>.from(response.data ?? const {});
    final items = (data['items'] as List? ?? data['subtitles'] as List? ?? const []).whereType<Map>();
    return items.map((item) => SubtitleTrack.fromJson(Map<String, Object?>.from(item))).toList(growable: false);
  }

  Future<List<SegmentSkip>> listSegments(String itemId) async {
    final response = await _apiClient.get<Map<String, Object?>>('/api/v1/items/$itemId/segments');
    final data = Map<String, Object?>.from(response.data ?? const {});
    final items = (data['segments'] as List? ?? data['items'] as List? ?? const []).whereType<Map>();
    return items.map((item) => SegmentSkip.fromJson(Map<String, Object?>.from(item))).toList(growable: false);
  }

  Future<void> heartbeat({
    required String sessionId,
    required int positionMs,
    required bool isPaused,
    required bool isBuffering,
  }) async {
    await _apiClient.post<Map<String, Object?>>(
      '/api/v1/playback/heartbeat',
      body: {
        'session_id': sessionId,
        'position_ms': positionMs,
        'state': isPaused ? 'paused' : 'playing',
        'is_paused': isPaused,
        'is_buffering': isBuffering,
      },
    );
  }

  Future<PlaybackSeekResult> seek({
    required String sessionId,
    required int positionMs,
  }) async {
    final response = await _apiClient.post<Map<String, Object?>>(
      '/api/v1/playback/seek',
      body: {
        'session_id': sessionId,
        'position_ms': positionMs,
      },
    );
    return PlaybackSeekResult.fromJson(Map<String, Object?>.from(response.data ?? const {}));
  }

  Future<void> stop({
    required String sessionId,
    required int positionMs,
  }) async {
    await _apiClient.post<Map<String, Object?>>(
      '/api/v1/playback/stop',
      body: {
        'session_id': sessionId,
        'position_ms': positionMs,
      },
    );
  }

  Future<VideoPlayerController> createNetworkController(Uri uri) async {
    final controller = VideoPlayerController.networkUrl(uri, httpHeaders: mediaHeaders ?? const {});
    await controller.initialize();
    return controller;
  }

  Uri streamUri(String streamUrl) {
    final uri = Uri.parse(streamUrl);
    if (uri.hasScheme) return uri;
    return _apiClient.absoluteUri(streamUrl);
  }

  Map<String, String>? get mediaHeaders {
    final token = _apiClient.bearerToken;
    return token == null ? null : {'Authorization': 'Bearer $token'};
  }

  Map<String, Object?> get _mobileDeviceProfile {
    return const {
      'client': 'duskcue_mobile',
      'platform': 'flutter',
      'video_codecs': ['h264'],
      'audio_codecs': ['aac', 'mp3', 'opus'],
      'subtitle_formats': ['webvtt', 'srt'],
      'max_resolution': '1080p',
      'hls_supported': true,
      'hdr_supported': false,
    };
  }
}

import 'package:duskcue_mobile/models/content_models.dart';
import 'package:duskcue_mobile/models/download_models.dart';
import 'package:duskcue_mobile/services/api_client.dart';
import 'package:duskcue_mobile/services/device_identity_service.dart';

class DownloadService {
  const DownloadService({
    required DuskcueApiClient apiClient,
    required DeviceIdentityService deviceIdentity,
  })  : _apiClient = apiClient,
        _deviceIdentity = deviceIdentity;

  final DuskcueApiClient _apiClient;
  final DeviceIdentityService _deviceIdentity;

  Future<DownloadPlan> planDownload({
    required String mediaItemId,
    required DownloadQualityMode qualityMode,
  }) async {
    final identity = await _deviceIdentity.current();
    final response = await _apiClient.get<Map<String, Object?>>(
      '/api/v1/downloads/plan/$mediaItemId',
      query: {
        'device_identifier': identity.deviceId,
        'client_platform': identity.clientPlatform,
        'quality_mode': qualityMode.apiValue,
        'include_artwork': true,
        'include_storyboards': false,
      },
    );
    return DownloadPlan.fromJson(_payload(response.data));
  }

  Future<DownloadJob> createDownloadJob({
    required MediaItemSummary item,
    required DownloadQualityMode qualityMode,
    DownloadPlan? plan,
  }) async {
    final identity = await _deviceIdentity.current();
    final selectedPlan = plan ?? await planDownload(mediaItemId: item.id, qualityMode: qualityMode);
    final response = await _apiClient.post<Map<String, Object?>>(
      '/api/v1/downloads/jobs',
      body: {
        'media_item_id': item.id,
        'media_file_id': selectedPlan.mediaFileId,
        'device_identifier': identity.deviceId,
        'device_name': identity.deviceName,
        'client_platform': identity.clientPlatform,
        'client_version': identity.clientVersion,
        'quality_mode': qualityMode.apiValue,
        'selected_audio': <String, Object?>{},
        'selected_subtitles': <Object?>[],
        'include_storyboards': false,
        'include_artwork': true,
        'plan_revision': selectedPlan.planRevision,
        'plan_hash': selectedPlan.planHash,
      },
    );
    return DownloadJob.fromJson(_payload(response.data));
  }

  Future<DownloadJob> getJob(String jobId) async {
    final response = await _apiClient.get<Map<String, Object?>>('/api/v1/downloads/jobs/$jobId');
    return DownloadJob.fromJson(_payload(response.data));
  }

  Future<void> cancelJob(String jobId, {String reason = 'cancelled on mobile'}) async {
    await _apiClient.post<Map<String, Object?>>(
      '/api/v1/downloads/jobs/$jobId/cancel',
      body: {'reason': reason},
    );
  }

  Future<void> deletePackage(String packageId) async {
    await _apiClient.delete<Map<String, Object?>>('/api/v1/downloads/packages/$packageId');
  }

  Future<DownloadPackageRenewal> renewPackage(String packageId) async {
    final identity = await _deviceIdentity.current();
    final response = await _apiClient.post<Map<String, Object?>>(
      '/api/v1/downloads/packages/$packageId/renew',
      body: {'device_identifier': identity.deviceId},
    );
    return DownloadPackageRenewal.fromJson(_payload(response.data));
  }

  Future<DownloadPackageManifest> getPackageManifest(String packageId) async {
    final identity = await _deviceIdentity.current();
    final response = await _apiClient.get<Map<String, Object?>>(
      '/api/v1/downloads/packages/$packageId/manifest',
      query: {'device_identifier': identity.deviceId},
    );
    return DownloadPackageManifest.fromJson(_payload(response.data));
  }

  Future<PackageTransferUrls> createTransferUrls({
    required String packageId,
    required List<String> filePaths,
  }) async {
    final identity = await _deviceIdentity.current();
    final response = await _apiClient.post<Map<String, Object?>>(
      '/api/v1/downloads/packages/$packageId/transfer-urls',
      body: {
        'device_identifier': identity.deviceId,
        'file_paths': filePaths,
      },
    );
    return PackageTransferUrls.fromJson(_payload(response.data));
  }

  Future<List<int>> downloadPackageFile(String url) async {
    final uri = Uri.parse(url);
    final response = await _apiClient.getBytes(
      uri.path,
      query: uri.queryParameters,
    );
    return response.data ?? const [];
  }

  Future<DownloadSyncResponse> syncDownloadState({
    required List<Map<String, Object?>> packageStates,
    required List<OfflinePlaybackEvent> playbackEvents,
  }) async {
    final identity = await _deviceIdentity.current();
    final response = await _apiClient.post<Map<String, Object?>>(
      '/api/v1/downloads/sync',
      body: {
        'device_identifier': identity.deviceId,
        'client_platform': identity.clientPlatform,
        'package_states': packageStates,
        'playback_events': playbackEvents.map((event) => event.toJson()).toList(growable: false),
      },
    );
    return DownloadSyncResponse.fromJson(_payload(response.data));
  }

  Map<String, Object?> _payload(Object? data) {
    if (data is Map<String, Object?>) return data;
    if (data is Map) return Map<String, Object?>.from(data);
    return const {};
  }
}

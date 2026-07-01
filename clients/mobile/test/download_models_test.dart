import 'package:duskcue_mobile/models/download_models.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('download inventory scope separates server user and device', () {
    const first = DownloadInventoryScope(
      serverOrigin: 'http://one.example:48027',
      userId: 'user-a',
      deviceIdentifier: 'device-a',
    );
    const second = DownloadInventoryScope(
      serverOrigin: 'http://one.example:48027',
      userId: 'user-a',
      deviceIdentifier: 'device-b',
    );
    const third = DownloadInventoryScope(
      serverOrigin: 'http://two.example:48027',
      userId: 'user-a',
      deviceIdentifier: 'device-a',
    );

    expect(first.key, isNot(second.key));
    expect(first.key, isNot(third.key));
  });

  test('job status events update package and failure metadata', () {
    final item = DownloadItem(
      mediaItemId: 'media-1',
      title: 'Movie',
      jobId: 'job-1',
      status: DownloadItemStatus.preparing,
      progressPercent: 10,
      updatedAt: DateTime.utc(2026),
    );
    const event = DownloadJobStatusEvent(
      jobId: 'job-1',
      packageId: 'package-1',
      mediaItemId: 'media-1',
      deviceIdentifier: 'device-1',
      status: DownloadItemStatus.ready,
      progressPercent: 100,
      bytesExpected: 1000,
      bytesPrepared: 1000,
      reason: 'download package ready',
    );

    final updated = item.applyStatusEvent(event);

    expect(updated.status, DownloadItemStatus.ready);
    expect(updated.packageId, 'package-1');
    expect(updated.progressPercent, 100);
    expect(updated.bytesPrepared, 1000);
    expect(updated.waitingReason, 'download package ready');
  });

  test('download settings round trip quality and network controls', () {
    const settings = DownloadManagerSettings(
      defaultQualityMode: DownloadQualityMode.dataSaver,
      wifiOnly: false,
      allowCellular: true,
      chargingOnly: true,
      pauseOnLowStorage: true,
      storageCapBytes: 1073741824,
    );

    final decoded = DownloadManagerSettings.fromJson(settings.toJson());

    expect(decoded.defaultQualityMode, DownloadQualityMode.dataSaver);
    expect(decoded.wifiOnly, isFalse);
    expect(decoded.allowCellular, isTrue);
    expect(decoded.chargingOnly, isTrue);
    expect(decoded.storageCapBytes, 1073741824);
  });

  test('package manifest resolves local playback file by package format', () {
    final hls = DownloadPackageManifest.fromJson({
      'package_id': 'package-1',
      'download_job_id': 'job-1',
      'schema_version': 1,
      'manifest_version': 1,
      'package_format': 'hls_fmp4',
      'package_strategy': 'remux',
      'media_item_id': 'media-1',
      'total_bytes': 100,
      'files': [
        {
          'relative_path': 'manifest.json',
          'file_role': 'manifest',
          'byte_size': 10,
          'checksum_sha256': 'a',
          'is_required': true,
        },
        {
          'relative_path': 'stream.m3u8',
          'file_role': 'manifest',
          'byte_size': 10,
          'checksum_sha256': 'b',
          'is_required': true,
        },
      ],
    });

    expect(hls.primaryPlaybackFile?.relativePath, 'stream.m3u8');

    final mp4 = DownloadPackageManifest.fromJson({
      'package_id': 'package-1',
      'download_job_id': 'job-1',
      'schema_version': 1,
      'manifest_version': 1,
      'package_format': 'mp4',
      'package_strategy': 'direct_copy',
      'media_item_id': 'media-1',
      'total_bytes': 100,
      'files': [
        {
          'relative_path': 'media.mp4',
          'file_role': 'mp4',
          'byte_size': 10,
          'checksum_sha256': 'c',
          'is_required': true,
        },
      ],
    });

    expect(mp4.primaryPlaybackFile?.relativePath, 'media.mp4');
  });

  test('download item tracks playable offline state and pending sync events', () {
    final item = DownloadItem(
      mediaItemId: 'media-1',
      title: 'Movie',
      packageId: 'package-1',
      status: DownloadItemStatus.playableOffline,
      localPlaybackPath: '/app/downloads/media.mp4',
      localResumePositionMs: 45000,
      pendingPlaybackEventCount: 2,
      updatedAt: DateTime.utc(2026),
    );

    final decoded = DownloadItem.fromJson(item.toJson());

    expect(decoded.canPlayOffline, isTrue);
    expect(decoded.localResumePositionMs, 45000);
    expect(decoded.pendingPlaybackEventCount, 2);
    expect(decoded.status, DownloadItemStatus.playableOffline);
  });
}

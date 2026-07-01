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
      autoDeleteWatched: true,
    );

    final decoded = DownloadManagerSettings.fromJson(settings.toJson());

    expect(decoded.defaultQualityMode, DownloadQualityMode.dataSaver);
    expect(decoded.wifiOnly, isFalse);
    expect(decoded.allowCellular, isTrue);
    expect(decoded.chargingOnly, isTrue);
    expect(decoded.storageCapBytes, 1073741824);
    expect(decoded.autoDeleteWatched, isTrue);
  });

  test('download settings can clear storage cap independently', () {
    const settings = DownloadManagerSettings(storageCapBytes: 1073741824);

    final cleared = settings.copyWith(clearStorageCap: true);
    final retained = settings.copyWith(allowCellular: true);

    expect(cleared.storageCapBytes, isNull);
    expect(retained.storageCapBytes, 1073741824);
    expect(retained.allowCellular, isTrue);
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

  test('transfer urls preserve checksums and byte-size hints', () {
    final urls = PackageTransferUrls.fromJson({
      'package_id': 'package-1',
      'expires_at': '2026-07-01T00:15:00Z',
      'files': [
        {
          'relative_path': 'media.mp4',
          'url': '/api/v1/downloads/packages/package-1/files/media.mp4',
          'method': 'GET',
          'headers': {
            'Accept-Ranges': 'bytes',
            'X-Duskcue-Checksum-Sha256': 'abc',
            'X-Duskcue-Byte-Size': 42,
          },
        },
      ],
    });

    expect(urls.packageId, 'package-1');
    expect(urls.files.single.relativePath, 'media.mp4');
    expect(urls.files.single.headers['Accept-Ranges'], 'bytes');
    expect(urls.files.single.headers['X-Duskcue-Checksum-Sha256'], 'abc');
    expect(urls.files.single.headers['X-Duskcue-Byte-Size'], 42);
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

  test('sync response separates accepted events and invalidated packages', () {
    final response = DownloadSyncResponse.fromJson({
      'accepted_package_states': 1,
      'accepted_playback_events': 2,
      'accepted_playback_event_ids': ['event-1', 'event-2'],
      'revoked_package_ids': ['package-revoked'],
      'expired_package_ids': ['package-expired'],
      'deleted_package_ids': ['package-deleted'],
      'server_time': '2026-07-01T00:00:00Z',
    });

    expect(response.acceptedPackageStates, 1);
    expect(response.acceptedPlaybackEvents, 2);
    expect(response.acceptedPlaybackEventIds, ['event-1', 'event-2']);
    expect(response.revokedPackageIds, ['package-revoked']);
    expect(response.expiredPackageIds, ['package-expired']);
    expect(response.deletedPackageIds, ['package-deleted']);
    expect(response.serverTime, DateTime.parse('2026-07-01T00:00:00Z'));
  });
}

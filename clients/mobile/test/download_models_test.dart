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
}

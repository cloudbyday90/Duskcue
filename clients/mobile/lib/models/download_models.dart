import 'package:duskcue_mobile/models/content_models.dart';

enum DownloadQualityMode {
  auto('auto', 'Auto'),
  dataSaver('data_saver', 'Data Saver'),
  standard('standard', 'Standard'),
  maximum('maximum', 'Maximum');

  const DownloadQualityMode(this.apiValue, this.label);

  final String apiValue;
  final String label;

  static DownloadQualityMode fromApiValue(String? value) {
    return DownloadQualityMode.values.firstWhere(
      (mode) => mode.apiValue == value || mode.name == value,
      orElse: () => DownloadQualityMode.auto,
    );
  }
}

enum DownloadItemStatus {
  queued,
  preparing,
  ready,
  downloading,
  paused,
  playableOffline,
  failed,
  expired,
  unavailable,
  cancelled;

  String get apiValue => name;

  String get label {
    return switch (this) {
      DownloadItemStatus.queued => 'Queued',
      DownloadItemStatus.preparing => 'Preparing',
      DownloadItemStatus.ready => 'Ready',
      DownloadItemStatus.downloading => 'Downloading',
      DownloadItemStatus.paused => 'Paused',
      DownloadItemStatus.playableOffline => 'Playable offline',
      DownloadItemStatus.failed => 'Failed',
      DownloadItemStatus.expired => 'Expired',
      DownloadItemStatus.unavailable => 'Unavailable',
      DownloadItemStatus.cancelled => 'Cancelled',
    };
  }

  bool get isTerminal {
    return this == DownloadItemStatus.ready ||
        this == DownloadItemStatus.playableOffline ||
        this == DownloadItemStatus.failed ||
        this == DownloadItemStatus.expired ||
        this == DownloadItemStatus.unavailable ||
        this == DownloadItemStatus.cancelled;
  }

  static DownloadItemStatus fromServerStatus(String? value) {
    return switch (value) {
      'queued' => DownloadItemStatus.queued,
      'preparing' => DownloadItemStatus.preparing,
      'ready' => DownloadItemStatus.ready,
      'failed' => DownloadItemStatus.failed,
      'expired' => DownloadItemStatus.expired,
      'revoked' => DownloadItemStatus.unavailable,
      'cancelled' => DownloadItemStatus.cancelled,
      _ => DownloadItemStatus.unavailable,
    };
  }

  static DownloadItemStatus fromJson(String? value) {
    if (value == 'playable_offline') return DownloadItemStatus.playableOffline;
    return DownloadItemStatus.values.firstWhere(
      (status) => status.name == value,
      orElse: () => DownloadItemStatus.unavailable,
    );
  }
}

class DownloadPackageManifest {
  const DownloadPackageManifest({
    required this.packageId,
    required this.downloadJobId,
    required this.schemaVersion,
    required this.manifestVersion,
    required this.packageFormat,
    required this.packageStrategy,
    required this.mediaItemId,
    required this.totalBytes,
    required this.files,
    this.mediaFileId,
    this.packageHashSha256,
    this.selectedAudio = const {},
    this.selectedSubtitles = const [],
    this.expiresAt,
    this.syncMetadata = const {},
  });

  final String packageId;
  final String downloadJobId;
  final int schemaVersion;
  final int manifestVersion;
  final String packageFormat;
  final String packageStrategy;
  final String mediaItemId;
  final String? mediaFileId;
  final int totalBytes;
  final String? packageHashSha256;
  final List<DownloadPackageFile> files;
  final Map<String, Object?> selectedAudio;
  final List<Map<String, Object?>> selectedSubtitles;
  final DateTime? expiresAt;
  final Map<String, Object?> syncMetadata;

  DownloadPackageFile? get primaryPlaybackFile {
    if (packageFormat == 'mp4') {
      return _firstWhereOrNull(
        files,
        (file) => file.fileRole == 'mp4' || file.relativePath.endsWith('.mp4'),
      );
    }
    return _firstWhereOrNull(
      files,
      (file) => file.relativePath.endsWith('.m3u8'),
    );
  }

  int get requiredFileCount {
    return files.where((file) => file.isRequired).length;
  }

  Map<String, Object?> toJson() {
    return {
      'package_id': packageId,
      'download_job_id': downloadJobId,
      'schema_version': schemaVersion,
      'manifest_version': manifestVersion,
      'package_format': packageFormat,
      'package_strategy': packageStrategy,
      'media_item_id': mediaItemId,
      'media_file_id': mediaFileId,
      'total_bytes': totalBytes,
      'package_hash_sha256': packageHashSha256,
      'files': files.map((file) => file.toJson()).toList(growable: false),
      'selected_audio': selectedAudio,
      'selected_subtitles': selectedSubtitles,
      'expires_at': expiresAt?.toIso8601String(),
      'sync_metadata': syncMetadata,
    };
  }

  static DownloadPackageManifest fromJson(Map<String, Object?> json) {
    final selectedSubtitles = (json['selected_subtitles'] as List? ?? const [])
        .whereType<Map>()
        .map((item) => Map<String, Object?>.from(item))
        .toList(growable: false);
    return DownloadPackageManifest(
      packageId: _string(json, const ['package_id']),
      downloadJobId: _string(json, const ['download_job_id', 'job_id']),
      schemaVersion: _int(json['schema_version']) ?? 1,
      manifestVersion: _int(json['manifest_version']) ?? 1,
      packageFormat: _string(json, const ['package_format'], fallback: 'hls_fmp4'),
      packageStrategy: _string(json, const ['package_strategy'], fallback: 'remux'),
      mediaItemId: _string(json, const ['media_item_id']),
      mediaFileId: _nullableString(json, const ['media_file_id']),
      totalBytes: _int(json['total_bytes']) ?? 0,
      packageHashSha256: _nullableString(json, const ['package_hash_sha256']),
      files: (json['files'] as List? ?? const [])
          .whereType<Map>()
          .map((file) => DownloadPackageFile.fromJson(Map<String, Object?>.from(file)))
          .toList(growable: false),
      selectedAudio: Map<String, Object?>.from((json['selected_audio'] as Map?) ?? const {}),
      selectedSubtitles: selectedSubtitles,
      expiresAt: _date(json['expires_at']),
      syncMetadata: Map<String, Object?>.from((json['sync_metadata'] as Map?) ?? const {}),
    );
  }
}

class DownloadPackageFile {
  const DownloadPackageFile({
    required this.relativePath,
    required this.fileRole,
    required this.byteSize,
    required this.checksumSha256,
    required this.isRequired,
    this.contentType,
    this.segmentIndex,
  });

  final String relativePath;
  final String fileRole;
  final String? contentType;
  final int byteSize;
  final String checksumSha256;
  final int? segmentIndex;
  final bool isRequired;

  Map<String, Object?> toJson() {
    return {
      'relative_path': relativePath,
      'file_role': fileRole,
      'content_type': contentType,
      'byte_size': byteSize,
      'checksum_sha256': checksumSha256,
      'segment_index': segmentIndex,
      'is_required': isRequired,
    };
  }

  static DownloadPackageFile fromJson(Map<String, Object?> json) {
    return DownloadPackageFile(
      relativePath: _string(json, const ['relative_path']),
      fileRole: _string(json, const ['file_role'], fallback: 'metadata'),
      contentType: _nullableString(json, const ['content_type']),
      byteSize: _int(json['byte_size']) ?? 0,
      checksumSha256: _string(json, const ['checksum_sha256']),
      segmentIndex: _int(json['segment_index']),
      isRequired: json['is_required'] as bool? ?? true,
    );
  }
}

class PackageTransferUrl {
  const PackageTransferUrl({
    required this.relativePath,
    required this.url,
    required this.method,
    this.headers = const {},
  });

  final String relativePath;
  final String url;
  final String method;
  final Map<String, Object?> headers;

  static PackageTransferUrl fromJson(Map<String, Object?> json) {
    return PackageTransferUrl(
      relativePath: _string(json, const ['relative_path']),
      url: _string(json, const ['url']),
      method: _string(json, const ['method'], fallback: 'GET'),
      headers: Map<String, Object?>.from((json['headers'] as Map?) ?? const {}),
    );
  }
}

class PackageTransferUrls {
  const PackageTransferUrls({
    required this.packageId,
    required this.expiresAt,
    required this.files,
  });

  final String packageId;
  final DateTime? expiresAt;
  final List<PackageTransferUrl> files;

  static PackageTransferUrls fromJson(Map<String, Object?> json) {
    return PackageTransferUrls(
      packageId: _string(json, const ['package_id']),
      expiresAt: _date(json['expires_at']),
      files: (json['files'] as List? ?? const [])
          .whereType<Map>()
          .map((file) => PackageTransferUrl.fromJson(Map<String, Object?>.from(file)))
          .toList(growable: false),
    );
  }
}

class OfflinePlaybackEvent {
  const OfflinePlaybackEvent({
    required this.eventId,
    required this.packageId,
    required this.eventType,
    required this.positionMs,
    required this.occurredAt,
    this.details = const {},
  });

  final String eventId;
  final String packageId;
  final String eventType;
  final int positionMs;
  final DateTime occurredAt;
  final Map<String, Object?> details;

  Map<String, Object?> toJson() {
    return {
      'event_id': eventId,
      'package_id': packageId,
      'event_type': eventType,
      'position_ms': positionMs,
      'occurred_at': occurredAt.toIso8601String(),
      'details': details,
    };
  }

  static OfflinePlaybackEvent fromJson(Map<String, Object?> json) {
    return OfflinePlaybackEvent(
      eventId: _string(json, const ['event_id'], fallback: _legacyEventId(json)),
      packageId: _string(json, const ['package_id']),
      eventType: _string(json, const ['event_type'], fallback: 'heartbeat'),
      positionMs: _int(json['position_ms']) ?? 0,
      occurredAt: _date(json['occurred_at']) ?? DateTime.fromMillisecondsSinceEpoch(0),
      details: Map<String, Object?>.from((json['details'] as Map?) ?? const {}),
    );
  }
}

class DownloadSyncResponse {
  const DownloadSyncResponse({
    required this.acceptedPackageStates,
    required this.acceptedPlaybackEvents,
    required this.acceptedPlaybackEventIds,
    required this.revokedPackageIds,
    required this.expiredPackageIds,
    required this.serverTime,
  });

  final int acceptedPackageStates;
  final int acceptedPlaybackEvents;
  final List<String> acceptedPlaybackEventIds;
  final List<String> revokedPackageIds;
  final List<String> expiredPackageIds;
  final DateTime? serverTime;

  static DownloadSyncResponse fromJson(Map<String, Object?> json) {
    return DownloadSyncResponse(
      acceptedPackageStates: _int(json['accepted_package_states']) ?? 0,
      acceptedPlaybackEvents: _int(json['accepted_playback_events']) ?? 0,
      acceptedPlaybackEventIds: _stringList(json['accepted_playback_event_ids']),
      revokedPackageIds: _stringList(json['revoked_package_ids']),
      expiredPackageIds: _stringList(json['expired_package_ids']),
      serverTime: _date(json['server_time']),
    );
  }
}

class DownloadInventoryScope {
  const DownloadInventoryScope({
    required this.serverOrigin,
    required this.userId,
    required this.deviceIdentifier,
  });

  final String serverOrigin;
  final String userId;
  final String deviceIdentifier;

  String get key => '$serverOrigin|$userId|$deviceIdentifier';

  Map<String, Object?> toJson() {
    return {
      'server_origin': serverOrigin,
      'user_id': userId,
      'device_identifier': deviceIdentifier,
    };
  }

  static DownloadInventoryScope fromJson(Map<String, Object?> json) {
    return DownloadInventoryScope(
      serverOrigin: json['server_origin'] as String? ?? '',
      userId: json['user_id'] as String? ?? '',
      deviceIdentifier: json['device_identifier'] as String? ?? '',
    );
  }
}

class DownloadManagerSettings {
  const DownloadManagerSettings({
    this.defaultQualityMode = DownloadQualityMode.auto,
    this.wifiOnly = true,
    this.allowCellular = false,
    this.chargingOnly = false,
    this.pauseOnLowStorage = true,
    this.storageCapBytes,
    this.autoDeleteWatched = false,
  });

  final DownloadQualityMode defaultQualityMode;
  final bool wifiOnly;
  final bool allowCellular;
  final bool chargingOnly;
  final bool pauseOnLowStorage;
  final int? storageCapBytes;
  final bool autoDeleteWatched;

  DownloadManagerSettings copyWith({
    DownloadQualityMode? defaultQualityMode,
    bool? wifiOnly,
    bool? allowCellular,
    bool? chargingOnly,
    bool? pauseOnLowStorage,
    int? storageCapBytes,
    bool clearStorageCap = false,
    bool? autoDeleteWatched,
  }) {
    return DownloadManagerSettings(
      defaultQualityMode: defaultQualityMode ?? this.defaultQualityMode,
      wifiOnly: wifiOnly ?? this.wifiOnly,
      allowCellular: allowCellular ?? this.allowCellular,
      chargingOnly: chargingOnly ?? this.chargingOnly,
      pauseOnLowStorage: pauseOnLowStorage ?? this.pauseOnLowStorage,
      storageCapBytes: clearStorageCap ? null : storageCapBytes ?? this.storageCapBytes,
      autoDeleteWatched: autoDeleteWatched ?? this.autoDeleteWatched,
    );
  }

  Map<String, Object?> toJson() {
    return {
      'default_quality_mode': defaultQualityMode.apiValue,
      'wifi_only': wifiOnly,
      'allow_cellular': allowCellular,
      'charging_only': chargingOnly,
      'pause_on_low_storage': pauseOnLowStorage,
      'storage_cap_bytes': storageCapBytes,
      'auto_delete_watched': autoDeleteWatched,
    };
  }

  static DownloadManagerSettings fromJson(Map<String, Object?> json) {
    return DownloadManagerSettings(
      defaultQualityMode: DownloadQualityMode.fromApiValue(json['default_quality_mode'] as String?),
      wifiOnly: json['wifi_only'] as bool? ?? true,
      allowCellular: json['allow_cellular'] as bool? ?? false,
      chargingOnly: json['charging_only'] as bool? ?? false,
      pauseOnLowStorage: json['pause_on_low_storage'] as bool? ?? true,
      storageCapBytes: _int(json['storage_cap_bytes']),
      autoDeleteWatched: json['auto_delete_watched'] as bool? ?? false,
    );
  }
}

class DownloadPlan {
  const DownloadPlan({
    required this.mediaItemId,
    required this.packageFormat,
    required this.packageStrategy,
    required this.qualityMode,
    required this.estimatedBytes,
    required this.planRevision,
    required this.planHash,
    this.mediaFileId,
    this.expiresAt,
  });

  final String mediaItemId;
  final String? mediaFileId;
  final String packageFormat;
  final String packageStrategy;
  final DownloadQualityMode qualityMode;
  final int estimatedBytes;
  final String planRevision;
  final String planHash;
  final DateTime? expiresAt;

  static DownloadPlan fromJson(Map<String, Object?> json) {
    return DownloadPlan(
      mediaItemId: _string(json, const ['media_item_id']),
      mediaFileId: _nullableString(json, const ['media_file_id']),
      packageFormat: _string(json, const ['package_format'], fallback: 'hls_fmp4'),
      packageStrategy: _string(json, const ['package_strategy'], fallback: 'remux'),
      qualityMode: DownloadQualityMode.fromApiValue(json['quality_mode'] as String?),
      estimatedBytes: _int(json['estimated_bytes']) ?? 0,
      planRevision: _string(json, const ['plan_revision']),
      planHash: _string(json, const ['plan_hash']),
      expiresAt: _date(json['expires_at']),
    );
  }
}

class DownloadJob {
  const DownloadJob({
    required this.id,
    required this.mediaItemId,
    required this.deviceIdentifier,
    required this.status,
    required this.packageFormat,
    required this.qualityMode,
    required this.progressPercent,
    required this.bytesPrepared,
    this.mediaFileId,
    this.bytesExpected,
    this.failureReason,
    this.expiresAt,
  });

  final String id;
  final String mediaItemId;
  final String? mediaFileId;
  final String deviceIdentifier;
  final DownloadItemStatus status;
  final String packageFormat;
  final DownloadQualityMode qualityMode;
  final double progressPercent;
  final int? bytesExpected;
  final int bytesPrepared;
  final String? failureReason;
  final DateTime? expiresAt;

  static DownloadJob fromJson(Map<String, Object?> json) {
    return DownloadJob(
      id: _string(json, const ['id', 'job_id']),
      mediaItemId: _string(json, const ['media_item_id']),
      mediaFileId: _nullableString(json, const ['media_file_id']),
      deviceIdentifier: _string(json, const ['device_identifier']),
      status: DownloadItemStatus.fromServerStatus(json['status'] as String?),
      packageFormat: _string(json, const ['package_format'], fallback: 'hls_fmp4'),
      qualityMode: DownloadQualityMode.fromApiValue(json['quality_mode'] as String?),
      progressPercent: _double(json['progress_percent']) ?? 0,
      bytesExpected: _int(json['bytes_expected']),
      bytesPrepared: _int(json['bytes_prepared']) ?? 0,
      failureReason: _nullableString(json, const ['failure_reason']),
      expiresAt: _date(json['expires_at']),
    );
  }
}

class DownloadJobStatusEvent {
  const DownloadJobStatusEvent({
    required this.jobId,
    required this.mediaItemId,
    required this.deviceIdentifier,
    required this.status,
    required this.progressPercent,
    required this.bytesPrepared,
    this.packageId,
    this.mediaFileId,
    this.bytesExpected,
    this.failureReason,
    this.retryCount,
    this.reason,
  });

  final String jobId;
  final String? packageId;
  final String mediaItemId;
  final String? mediaFileId;
  final String deviceIdentifier;
  final DownloadItemStatus status;
  final double progressPercent;
  final int? bytesExpected;
  final int bytesPrepared;
  final String? failureReason;
  final int? retryCount;
  final String? reason;

  static DownloadJobStatusEvent fromJson(Map<String, Object?> json) {
    return DownloadJobStatusEvent(
      jobId: _string(json, const ['job_id']),
      packageId: _nullableString(json, const ['package_id']),
      mediaItemId: _string(json, const ['media_item_id']),
      mediaFileId: _nullableString(json, const ['media_file_id']),
      deviceIdentifier: _string(json, const ['device_identifier']),
      status: DownloadItemStatus.fromServerStatus(json['status'] as String?),
      progressPercent: _double(json['progress_percent']) ?? 0,
      bytesExpected: _int(json['bytes_expected']),
      bytesPrepared: _int(json['bytes_prepared']) ?? 0,
      failureReason: _nullableString(json, const ['failure_reason']),
      retryCount: _int(json['retry_count']),
      reason: _nullableString(json, const ['reason']),
    );
  }
}

class DownloadItem {
  const DownloadItem({
    required this.mediaItemId,
    required this.title,
    required this.status,
    required this.updatedAt,
    this.mediaType,
    this.jobId,
    this.packageId,
    this.mediaFileId,
    this.qualityMode = DownloadQualityMode.auto,
    this.progressPercent = 0,
    this.bytesExpected,
    this.bytesPrepared = 0,
    this.localFilesVerified = 0,
    this.localResumePositionMs = 0,
    this.pendingPlaybackEventCount = 0,
    this.failureReason,
    this.waitingReason,
    this.localPlaybackPath,
    this.localManifestHashSha256,
    this.expiresAt,
    this.localPlaybackUpdatedAt,
    this.localCompleted = false,
    this.localWatched = false,
  });

  final String mediaItemId;
  final String title;
  final String? mediaType;
  final String? jobId;
  final String? packageId;
  final String? mediaFileId;
  final DownloadItemStatus status;
  final DownloadQualityMode qualityMode;
  final double progressPercent;
  final int? bytesExpected;
  final int bytesPrepared;
  final int localFilesVerified;
  final int localResumePositionMs;
  final int pendingPlaybackEventCount;
  final String? failureReason;
  final String? waitingReason;
  final String? localPlaybackPath;
  final String? localManifestHashSha256;
  final DateTime? expiresAt;
  final DateTime? localPlaybackUpdatedAt;
  final bool localCompleted;
  final bool localWatched;
  final DateTime updatedAt;

  String get id => jobId ?? mediaItemId;

  bool get canRetry => status == DownloadItemStatus.failed || status == DownloadItemStatus.unavailable;

  bool get canPlayOffline {
    return status == DownloadItemStatus.playableOffline &&
        localPlaybackPath != null &&
        localPlaybackPath!.isNotEmpty &&
        (expiresAt == null || expiresAt!.isAfter(DateTime.now()));
  }

  DownloadItem copyWith({
    String? title,
    String? mediaType,
    String? jobId,
    String? packageId,
    String? mediaFileId,
    DownloadItemStatus? status,
    DownloadQualityMode? qualityMode,
    double? progressPercent,
    int? bytesExpected,
    int? bytesPrepared,
    int? localFilesVerified,
    int? localResumePositionMs,
    int? pendingPlaybackEventCount,
    String? failureReason,
    String? waitingReason,
    bool clearWaitingReason = false,
    String? localPlaybackPath,
    String? localManifestHashSha256,
    DateTime? expiresAt,
    DateTime? localPlaybackUpdatedAt,
    bool? localCompleted,
    bool? localWatched,
    DateTime? updatedAt,
  }) {
    return DownloadItem(
      mediaItemId: mediaItemId,
      title: title ?? this.title,
      mediaType: mediaType ?? this.mediaType,
      jobId: jobId ?? this.jobId,
      packageId: packageId ?? this.packageId,
      mediaFileId: mediaFileId ?? this.mediaFileId,
      status: status ?? this.status,
      qualityMode: qualityMode ?? this.qualityMode,
      progressPercent: progressPercent ?? this.progressPercent,
      bytesExpected: bytesExpected ?? this.bytesExpected,
      bytesPrepared: bytesPrepared ?? this.bytesPrepared,
      localFilesVerified: localFilesVerified ?? this.localFilesVerified,
      localResumePositionMs: localResumePositionMs ?? this.localResumePositionMs,
      pendingPlaybackEventCount: pendingPlaybackEventCount ?? this.pendingPlaybackEventCount,
      failureReason: failureReason,
      waitingReason: clearWaitingReason ? null : waitingReason ?? this.waitingReason,
      localPlaybackPath: localPlaybackPath ?? this.localPlaybackPath,
      localManifestHashSha256: localManifestHashSha256 ?? this.localManifestHashSha256,
      expiresAt: expiresAt ?? this.expiresAt,
      localPlaybackUpdatedAt: localPlaybackUpdatedAt ?? this.localPlaybackUpdatedAt,
      localCompleted: localCompleted ?? this.localCompleted,
      localWatched: localWatched ?? this.localWatched,
      updatedAt: updatedAt ?? this.updatedAt,
    );
  }

  DownloadItem applyJob(DownloadJob job) {
    return copyWith(
      jobId: job.id,
      mediaFileId: job.mediaFileId,
      status: job.status,
      qualityMode: job.qualityMode,
      progressPercent: job.progressPercent,
      bytesExpected: job.bytesExpected,
      bytesPrepared: job.bytesPrepared,
      failureReason: job.failureReason,
      expiresAt: job.expiresAt,
      updatedAt: DateTime.now(),
    );
  }

  DownloadItem applyStatusEvent(DownloadJobStatusEvent event) {
    return copyWith(
      jobId: event.jobId,
      packageId: event.packageId,
      mediaFileId: event.mediaFileId,
      status: event.status,
      progressPercent: event.progressPercent,
      bytesExpected: event.bytesExpected,
      bytesPrepared: event.bytesPrepared,
      failureReason: event.failureReason,
      waitingReason: event.reason,
      updatedAt: DateTime.now(),
    );
  }

  Map<String, Object?> toJson() {
    return {
      'media_item_id': mediaItemId,
      'title': title,
      'media_type': mediaType,
      'job_id': jobId,
      'package_id': packageId,
      'media_file_id': mediaFileId,
      'status': status.name,
      'quality_mode': qualityMode.apiValue,
      'progress_percent': progressPercent,
      'bytes_expected': bytesExpected,
      'bytes_prepared': bytesPrepared,
      'local_files_verified': localFilesVerified,
      'local_resume_position_ms': localResumePositionMs,
      'pending_playback_event_count': pendingPlaybackEventCount,
      'failure_reason': failureReason,
      'waiting_reason': waitingReason,
      'local_playback_path': localPlaybackPath,
      'local_manifest_hash_sha256': localManifestHashSha256,
      'expires_at': expiresAt?.toIso8601String(),
      'local_playback_updated_at': localPlaybackUpdatedAt?.toIso8601String(),
      'local_completed': localCompleted,
      'local_watched': localWatched,
      'updated_at': updatedAt.toIso8601String(),
    };
  }

  static DownloadItem fromJson(Map<String, Object?> json) {
    return DownloadItem(
      mediaItemId: _string(json, const ['media_item_id']),
      title: _string(json, const ['title'], fallback: 'Download'),
      mediaType: _nullableString(json, const ['media_type']),
      jobId: _nullableString(json, const ['job_id']),
      packageId: _nullableString(json, const ['package_id']),
      mediaFileId: _nullableString(json, const ['media_file_id']),
      status: DownloadItemStatus.fromJson(json['status'] as String?),
      qualityMode: DownloadQualityMode.fromApiValue(json['quality_mode'] as String?),
      progressPercent: _double(json['progress_percent']) ?? 0,
      bytesExpected: _int(json['bytes_expected']),
      bytesPrepared: _int(json['bytes_prepared']) ?? 0,
      localFilesVerified: _int(json['local_files_verified']) ?? 0,
      localResumePositionMs: _int(json['local_resume_position_ms']) ?? 0,
      pendingPlaybackEventCount: _int(json['pending_playback_event_count']) ?? 0,
      failureReason: _nullableString(json, const ['failure_reason']),
      waitingReason: _nullableString(json, const ['waiting_reason']),
      localPlaybackPath: _nullableString(json, const ['local_playback_path']),
      localManifestHashSha256: _nullableString(json, const ['local_manifest_hash_sha256']),
      expiresAt: _date(json['expires_at']),
      localPlaybackUpdatedAt: _date(json['local_playback_updated_at']),
      localCompleted: json['local_completed'] as bool? ?? false,
      localWatched: json['local_watched'] as bool? ?? false,
      updatedAt: _date(json['updated_at']) ?? DateTime.fromMillisecondsSinceEpoch(0),
    );
  }

  static DownloadItem queued(MediaItemSummary item, DownloadJob job) {
    return DownloadItem(
      mediaItemId: item.id,
      title: item.title,
      mediaType: item.mediaType,
      jobId: job.id,
      mediaFileId: job.mediaFileId,
      status: job.status,
      qualityMode: job.qualityMode,
      progressPercent: job.progressPercent,
      bytesExpected: job.bytesExpected,
      bytesPrepared: job.bytesPrepared,
      failureReason: job.failureReason,
      expiresAt: job.expiresAt,
      updatedAt: DateTime.now(),
    );
  }
}

String _string(Map<String, Object?> json, List<String> keys, {String fallback = ''}) {
  return _nullableString(json, keys) ?? fallback;
}

String? _nullableString(Map<String, Object?> json, List<String> keys) {
  for (final key in keys) {
    final value = json[key];
    if (value is String && value.isNotEmpty) return value;
    if (value != null && value is! Map && value is! List) return value.toString();
  }
  return null;
}

int? _int(Object? value) {
  if (value is int) return value;
  if (value is num) return value.toInt();
  if (value is String) return int.tryParse(value);
  return null;
}

double? _double(Object? value) {
  if (value is double) return value;
  if (value is num) return value.toDouble();
  if (value is String) return double.tryParse(value);
  return null;
}

DateTime? _date(Object? value) {
  if (value is DateTime) return value;
  if (value is String && value.isNotEmpty) return DateTime.tryParse(value);
  return null;
}

List<String> _stringList(Object? value) {
  return (value as List? ?? const [])
      .map((item) => item.toString())
      .where((item) => item.isNotEmpty)
      .toList(growable: false);
}

String _legacyEventId(Map<String, Object?> json) {
  return [
    _string(json, const ['package_id']),
    _string(json, const ['event_type'], fallback: 'heartbeat'),
    _int(json['position_ms']) ?? 0,
    _string(json, const ['occurred_at']),
  ].join(':');
}

T? _firstWhereOrNull<T>(Iterable<T> items, bool Function(T item) test) {
  for (final item in items) {
    if (test(item)) return item;
  }
  return null;
}

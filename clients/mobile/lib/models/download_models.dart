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
      DownloadItemStatus.failed => 'Failed',
      DownloadItemStatus.expired => 'Expired',
      DownloadItemStatus.unavailable => 'Unavailable',
      DownloadItemStatus.cancelled => 'Cancelled',
    };
  }

  bool get isTerminal {
    return this == DownloadItemStatus.ready ||
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
    return DownloadItemStatus.values.firstWhere(
      (status) => status.name == value,
      orElse: () => DownloadItemStatus.unavailable,
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
    this.failureReason,
    this.waitingReason,
    this.expiresAt,
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
  final String? failureReason;
  final String? waitingReason;
  final DateTime? expiresAt;
  final DateTime updatedAt;

  String get id => jobId ?? mediaItemId;

  bool get canRetry => status == DownloadItemStatus.failed || status == DownloadItemStatus.unavailable;

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
    String? failureReason,
    String? waitingReason,
    bool clearWaitingReason = false,
    DateTime? expiresAt,
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
      failureReason: failureReason,
      waitingReason: clearWaitingReason ? null : waitingReason ?? this.waitingReason,
      expiresAt: expiresAt ?? this.expiresAt,
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
      'failure_reason': failureReason,
      'waiting_reason': waitingReason,
      'expires_at': expiresAt?.toIso8601String(),
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
      failureReason: _nullableString(json, const ['failure_reason']),
      waitingReason: _nullableString(json, const ['waiting_reason']),
      expiresAt: _date(json['expires_at']),
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

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

class PageResult<T> {
  const PageResult({
    required this.items,
    this.nextCursor,
    this.total,
  });

  final List<T> items;
  final String? nextCursor;
  final int? total;

  bool get hasMore => nextCursor != null && nextCursor!.isNotEmpty;
}

class LibrarySummary {
  const LibrarySummary({
    required this.id,
    required this.name,
    this.kind,
    this.itemCount,
  });

  final String id;
  final String name;
  final String? kind;
  final int? itemCount;

  factory LibrarySummary.fromJson(Map<String, Object?> json) {
    return LibrarySummary(
      id: _string(json, const ['id', 'library_id']),
      name: _string(json, const ['name', 'title'], fallback: 'Library'),
      kind: _nullableString(json, const ['kind', 'type', 'media_type']),
      itemCount: _nullableInt(json, const ['item_count', 'media_count', 'items_count']),
    );
  }
}

class MediaItemSummary {
  const MediaItemSummary({
    required this.id,
    required this.title,
    this.mediaType,
    this.year,
    this.overview,
    this.libraryName,
    this.durationMs,
  });

  final String id;
  final String title;
  final String? mediaType;
  final int? year;
  final String? overview;
  final String? libraryName;
  final int? durationMs;

  factory MediaItemSummary.fromJson(Map<String, Object?> json) {
    return MediaItemSummary(
      id: _string(json, const ['id', 'media_item_id', 'item_id']),
      title: _string(json, const ['title', 'name', 'sort_title'], fallback: 'Untitled'),
      mediaType: _nullableString(json, const ['media_type', 'type', 'kind']),
      year: _nullableInt(json, const ['release_year', 'year']),
      overview: _nullableString(json, const ['overview', 'summary', 'description']),
      libraryName: _nullableString(json, const ['library_name']),
      durationMs: _nullableInt(json, const ['duration_ms', 'runtime_ms']),
    );
  }
}

class CollectionSummary {
  const CollectionSummary({
    required this.id,
    required this.name,
    this.description,
    this.itemCount,
  });

  final String id;
  final String name;
  final String? description;
  final int? itemCount;

  factory CollectionSummary.fromJson(Map<String, Object?> json) {
    return CollectionSummary(
      id: _string(json, const ['id', 'collection_id']),
      name: _string(json, const ['name', 'title'], fallback: 'Collection'),
      description: _nullableString(json, const ['description', 'summary']),
      itemCount: _nullableInt(json, const ['item_count', 'items_count']),
    );
  }
}

class NotificationSummary {
  const NotificationSummary({
    required this.id,
    required this.title,
    this.body,
    this.notificationType,
    this.readAt,
    this.createdAt,
  });

  final String id;
  final String title;
  final String? body;
  final String? notificationType;
  final DateTime? readAt;
  final DateTime? createdAt;

  bool get isRead => readAt != null;

  factory NotificationSummary.fromJson(Map<String, Object?> json) {
    return NotificationSummary(
      id: _string(json, const ['id', 'notification_id']),
      title: _string(json, const ['title', 'subject'], fallback: 'Notification'),
      body: _nullableString(json, const ['body', 'message', 'detail']),
      notificationType: _nullableString(json, const ['notification_type', 'type_name', 'type']),
      readAt: _nullableDate(json, const ['read_at']),
      createdAt: _nullableDate(json, const ['created_at']),
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

int? _nullableInt(Map<String, Object?> json, List<String> keys) {
  for (final key in keys) {
    final value = json[key];
    if (value is int) return value;
    if (value is num) return value.toInt();
    if (value is String) return int.tryParse(value);
  }
  return null;
}

DateTime? _nullableDate(Map<String, Object?> json, List<String> keys) {
  for (final key in keys) {
    final value = json[key];
    if (value is DateTime) return value;
    if (value is String) return DateTime.tryParse(value);
  }
  return null;
}

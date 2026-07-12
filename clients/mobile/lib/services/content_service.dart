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

import 'package:duskcue_mobile/models/content_models.dart';
import 'package:duskcue_mobile/services/api_client.dart';

class ContentService {
  const ContentService(this._apiClient);

  final DuskcueApiClient _apiClient;

  Future<PageResult<LibrarySummary>> listLibraries() async {
    final response = await _apiClient.get<Map<String, Object?>>('/api/v1/libraries');
    return _page(response.data, LibrarySummary.fromJson);
  }

  Future<PageResult<MediaItemSummary>> listMediaItems({String? cursor, int limit = 24}) async {
    final response = await _apiClient.get<Map<String, Object?>>(
      '/api/v1/media-items',
      query: _pageQuery(cursor: cursor, limit: limit),
    );
    return _page(response.data, _mediaItem);
  }

  Future<PageResult<MediaItemSummary>> listLibraryItems(String libraryId, {String? cursor, int limit = 24}) async {
    final response = await _apiClient.get<Map<String, Object?>>(
      '/api/v1/libraries/$libraryId/items',
      query: _pageQuery(cursor: cursor, limit: limit),
    );
    return _page(response.data, _mediaItem);
  }

  Future<MediaItemSummary> getMediaItem(String itemId) async {
    final response = await _apiClient.get<Map<String, Object?>>('/api/v1/media-items/$itemId');
    return _mediaItem(_payload(response.data));
  }

  Future<PageResult<MediaItemSummary>> search(String query, {String? cursor, int limit = 24}) async {
    if (query.trim().isEmpty) {
      return const PageResult(items: []);
    }
    final response = await _apiClient.get<Map<String, Object?>>(
      '/api/v1/search',
      query: {
        'q': query.trim(),
        ..._pageQuery(cursor: cursor, limit: limit),
      },
    );
    return _page(response.data, _mediaItem);
  }

  Future<PageResult<CollectionSummary>> listCollections({String? cursor, int limit = 24}) async {
    final response = await _apiClient.get<Map<String, Object?>>(
      '/api/v1/collections',
      query: _pageQuery(cursor: cursor, limit: limit),
    );
    return _page(response.data, CollectionSummary.fromJson);
  }

  Future<CollectionSummary> getCollection(String collectionId) async {
    final response = await _apiClient.get<Map<String, Object?>>('/api/v1/collections/$collectionId');
    return CollectionSummary.fromJson(_payload(response.data));
  }

  Future<PageResult<MediaItemSummary>> listCollectionItems(String collectionId, {String? cursor, int limit = 24}) async {
    final response = await _apiClient.get<Map<String, Object?>>(
      '/api/v1/collections/$collectionId/items',
      query: _pageQuery(cursor: cursor, limit: limit),
    );
    return _page(response.data, _mediaItem);
  }

  Future<PageResult<NotificationSummary>> listNotifications({String? cursor, int limit = 30}) async {
    final response = await _apiClient.get<Map<String, Object?>>(
      '/api/v1/notifications',
      query: _pageQuery(cursor: cursor, limit: limit),
    );
    return _page(response.data, NotificationSummary.fromJson);
  }

  Future<int> unreadNotificationCount() async {
    final response = await _apiClient.get<Map<String, Object?>>('/api/v1/notifications/unread-count');
    final data = _payload(response.data);
    final count = data['count'] ?? data['unread_count'];
    if (count is int) return count;
    if (count is num) return count.toInt();
    return int.tryParse(count?.toString() ?? '') ?? 0;
  }

  Future<void> markNotificationRead(String notificationId) async {
    await _apiClient.post<Object?>('/api/v1/notifications/$notificationId/read');
  }

  Future<void> markAllNotificationsRead() async {
    await _apiClient.post<Object?>('/api/v1/notifications/read-all');
  }

  Uri artworkUri(String itemId, {String type = 'poster'}) {
    return _apiClient.absoluteUri('/api/v1/items/$itemId/artwork/$type');
  }

  Map<String, String>? get mediaHeaders {
    final token = _apiClient.bearerToken;
    return token == null ? null : {'Authorization': 'Bearer $token'};
  }

  Map<String, Object?> _pageQuery({String? cursor, required int limit}) {
    return {
      'limit': limit,
      if (cursor != null && cursor.isNotEmpty) 'cursor': cursor,
    };
  }

  PageResult<T> _page<T>(Object? data, T Function(Map<String, Object?> json) fromJson) {
    final payload = _payload(data);
    final rows = _items(payload);
    return PageResult<T>(
      items: rows.map(fromJson).toList(growable: false),
      nextCursor: _string(payload, const ['next_cursor', 'nextCursor', 'cursor']),
      total: _int(payload, const ['total', 'total_count', 'count']),
    );
  }

  Map<String, Object?> _payload(Object? data) {
    if (data is Map<String, Object?>) return data;
    if (data is Map) return Map<String, Object?>.from(data);
    return const {};
  }

  List<Map<String, Object?>> _items(Map<String, Object?> payload) {
    final value = payload['items'] ?? payload['results'] ?? payload['data'];
    if (value is List) {
      return value.whereType<Map>().map((item) => Map<String, Object?>.from(item)).toList(growable: false);
    }
    return const [];
  }

  MediaItemSummary _mediaItem(Map<String, Object?> json) {
    final nested = json['media_item'] ?? json['item'] ?? json['media'];
    if (nested is Map) {
      return MediaItemSummary.fromJson({
        ...json,
        ...Map<String, Object?>.from(nested),
      });
    }
    return MediaItemSummary.fromJson(json);
  }

  String? _string(Map<String, Object?> payload, List<String> keys) {
    for (final key in keys) {
      final value = payload[key];
      if (value is String && value.isNotEmpty) return value;
    }
    return null;
  }

  int? _int(Map<String, Object?> payload, List<String> keys) {
    for (final key in keys) {
      final value = payload[key];
      if (value is int) return value;
      if (value is num) return value.toInt();
      if (value is String) return int.tryParse(value);
    }
    return null;
  }
}

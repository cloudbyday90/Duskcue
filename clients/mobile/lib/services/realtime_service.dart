import 'dart:async';
import 'dart:convert';

import 'package:dio/dio.dart';
import 'package:duskcue_mobile/models/realtime_models.dart';
import 'package:duskcue_mobile/services/api_client.dart';

class RealtimeService {
  RealtimeService(this._apiClient);

  final DuskcueApiClient _apiClient;
  final StreamController<RealtimeEvent> _events = StreamController<RealtimeEvent>.broadcast();
  final StreamController<RealtimeConnectionStatus> _status = StreamController<RealtimeConnectionStatus>.broadcast();
  StreamSubscription<String>? _lineSubscription;
  Timer? _reconnectTimer;
  String? _lastEventId;
  String _event = 'message';
  String? _id;
  List<String> _dataLines = [];
  bool _desired = false;
  bool _connecting = false;

  Stream<RealtimeEvent> get events => _events.stream;

  Stream<RealtimeConnectionStatus> get status => _status.stream;

  String? get lastEventId => _lastEventId;

  Future<void> connect() async {
    _desired = true;
    if (_connecting || _lineSubscription != null) return;
    _connecting = true;
    _status.add(RealtimeConnectionStatus.connecting);

    try {
      final response = await _apiClient.stream(
        '/api/v1/events',
        query: {
          'types': 'notification,session_kicked,playback_updated,transcode_progress,storyboard_progress,scan_progress,admin_task',
        },
        headers: {
          if (_lastEventId != null) 'Last-Event-ID': _lastEventId!,
        },
      );
      final body = response.data;
      if (body == null) {
        throw StateError('SSE stream did not return a response body.');
      }
      _lineSubscription = body.stream.cast<List<int>>().transform(utf8.decoder).transform(const LineSplitter()).listen(
        _handleLine,
        onError: (_) => _handleDisconnect(reconnect: true),
        onDone: () => _handleDisconnect(reconnect: true),
        cancelOnError: true,
      );
      _status.add(RealtimeConnectionStatus.connected);
    } on DioException {
      _handleDisconnect(reconnect: true);
    } catch (_) {
      _handleDisconnect(reconnect: true);
    } finally {
      _connecting = false;
    }
  }

  Future<void> disconnect() async {
    _desired = false;
    _reconnectTimer?.cancel();
    _reconnectTimer = null;
    await _lineSubscription?.cancel();
    _lineSubscription = null;
    _resetFrame();
    _status.add(RealtimeConnectionStatus.disconnected);
  }

  void _handleLine(String line) {
    if (line.isEmpty) {
      _dispatchFrame();
      return;
    }
    if (line.startsWith(':')) return;

    final separator = line.indexOf(':');
    final field = separator < 0 ? line : line.substring(0, separator);
    final value = separator < 0 ? '' : line.substring(separator + 1).trimLeft();

    switch (field) {
      case 'event':
        _event = value;
        break;
      case 'id':
        _id = value;
        _lastEventId = value;
        break;
      case 'data':
        _dataLines = [..._dataLines, value];
        break;
    }
  }

  void _dispatchFrame() {
    final event = SseFrame(event: _event, id: _id, dataLines: _dataLines).toEvent();
    if (event != null) {
      _events.add(event);
    }
    _resetFrame();
  }

  void _resetFrame() {
    _event = 'message';
    _id = null;
    _dataLines = [];
  }

  void _handleDisconnect({required bool reconnect}) {
    _lineSubscription?.cancel();
    _lineSubscription = null;
    _status.add(RealtimeConnectionStatus.disconnected);
    if (!_desired || !reconnect) return;
    _reconnectTimer?.cancel();
    _reconnectTimer = Timer(const Duration(seconds: 5), () => unawaited(connect()));
  }
}

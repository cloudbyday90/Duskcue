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

import 'dart:async';
import 'dart:io';
import 'package:duskcue_mobile/models/profile_models.dart';
import 'package:duskcue_mobile/services/api_client.dart';
import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';

class AmbientClientScope {
  const AmbientClientScope(this.user, this.profile, {required this.ready});
  final String user, profile;
  final bool ready;
}

class AmbientPlaybackService extends ChangeNotifier {
  AmbientPlaybackService({
    required DuskcueApiClient apiClient,
    MethodChannel? channel,
    AmbientClientScope? Function()? scope,
    bool? nativeAndroid,
  }) : _apiClient = apiClient,
       _channel = channel ?? const MethodChannel('duskcue/ambient_player'),
       _scope = scope,
       _android = nativeAndroid ?? Platform.isAndroid {
    _subscription = apiClient.scopeChanges.listen((_) => scopeChanged());
    if (_android) _bind();
  }

  final DuskcueApiClient _apiClient;
  final MethodChannel _channel;
  final AmbientClientScope? Function()? _scope;
  final bool _android;
  late final StreamSubscription<void> _subscription;
  int _epoch = 0, _bindRevision = 0, _boundRevision = -1;
  int? _logoutCredential;
  bool _disposed = false, _bindingFailed = false, _fenceRequired = false;
  Future<void>? _binding, _fencing, _command, _cleanup, _checking, _pause;
  Completer<void>? _fenceSettlement;
  Future<NativeAmbientPlaybackStatus>? _status;
  int get epoch => _epoch;

  Map<String, Object?>? _runtime() {
    final origin = _apiClient.serverOrigin;
    final token = _apiClient.bearerToken;
    final scope = _scope?.call();
    if (origin == null || token == null || token.isEmpty || scope == null ||
        !scope.ready || _logoutCredential == _apiClient.credentialEpoch) return null;
    return {
      'server_origin': origin.origin,
      'bearer_token': token,
      'user_id': scope.user,
      'profile_id': scope.profile,
      'scope_epoch': '${_apiClient.credentialEpoch}:${_apiClient.profileEpoch}',
    };
  }

  void scopeChanged() {
    _epoch++;
    _bindRevision++;
    if (_android) {
      _requireFence();
    }
    if (!_disposed) notifyListeners();
  }

  void _requireFence() {
    _fenceRequired = true;
    if (_fenceSettlement == null || _fenceSettlement!.isCompleted) _fenceSettlement = Completer<void>();
    _fence();
  }

  Future<void> _waitFence() async {
    final revision = _bindRevision;
    await _fenceSettlement?.future.timeout(const Duration(seconds: 5));
    if (_disposed || revision != _bindRevision || _fenceRequired || _bindingFailed) {
      throw StateError('The current native playback fence is not confirmed.');
    }
  }

  void _fence() {
    if (_fencing != null || _disposed) return;
    if (_fenceSettlement == null || _fenceSettlement!.isCompleted) _fenceSettlement = Completer<void>();
    final revision = _bindRevision;
    _bindingFailed = false;
    final work = _channel.invokeMethod<void>('fence', _runtime()).then((_) {
      if (!_disposed && revision == _bindRevision) _fenceRequired = false;
    }, onError: (Object _) {
      if (!_disposed && revision == _bindRevision) _bindingFailed = true;
    });
    _fencing = work;
    work.whenComplete(() {
      if (identical(_fencing, work)) _fencing = null;
      if (_disposed) return;
      if (revision != _bindRevision) _fence();
      else {
        if (_fenceSettlement?.isCompleted == false) _fenceSettlement!.complete();
        if (!_fenceRequired && !_bindingFailed) _bind();
      }
    });
  }

  void _bind() {
    if (_binding != null || _fencing != null || _fenceRequired || _disposed) return;
    final revision = _bindRevision;
    final work = _channel.invokeMethod<void>('bind', _runtime()).then((_) {
      if (!_disposed && revision == _bindRevision) {
        _bindingFailed = false;
        _boundRevision = revision;
      }
    }, onError: (Object error) {
      if (!_disposed && revision == _bindRevision) {
        _bindingFailed = true;
        if (error is PlatformException && error.code == 'scope_changed') {
          _requireFence();
        }
      }
    });
    _binding = work;
    work.whenComplete(() {
      if (identical(_binding, work)) _binding = null;
      if (!_disposed && !_fenceRequired && !_bindingFailed && _boundRevision != _bindRevision) _bind();
    });
  }

  Future<void> fenceForLogout() async {
    _logoutCredential = _apiClient.credentialEpoch;
    scopeChanged();
    if (_android) await _waitFence();
  }

  Future<void> _invoke(String method, [Object? arguments]) async {
    final captured = _epoch;
    final current = switch (method) {
      'stop' => _cleanup,
      'check' => _checking,
      'pause' => _pause,
      _ => _command,
    };
    if (current != null) throw StateError('Ambient playback is still checking the previous action.');
    final supplied = _android && method != 'start'
        ? <String, Object?>{'runtime': _runtime(), if (arguments is Map<String, Object?>) ...arguments}
        : arguments;
    final work = _channel.invokeMethod<void>(method, supplied);
    switch (method) {
      case 'stop': _cleanup = work;
      case 'check': _checking = work;
      case 'pause': _pause = work;
      default: _command = work;
    }
    void finished() {
      if (identical(_command, work)) _command = null;
      if (identical(_cleanup, work)) _cleanup = null;
      if (identical(_checking, work)) _checking = null;
      if (identical(_pause, work)) _pause = null;
    }
    work.then((_) => finished(), onError: (Object _) => finished());
    await work.timeout(const Duration(seconds: 5));
    if (captured != _epoch) throw StateError('The playback session changed.');
  }

  Future<void> start(AmbientChannelSummary channel) async {
    final captured = _epoch;
    final origin = _apiClient.serverOrigin;
    final token = _apiClient.bearerToken;
    if (origin == null || token == null || token.isEmpty) {
      throw StateError(
        'An authenticated server session is required for ambient playback.',
      );
    }
    if (_android) {
      if (_fenceRequired) {
        _fence();
        await _waitFence();
      }
      _bind();
      await _binding?.timeout(const Duration(seconds: 5));
      if (_fenceRequired || _bindingFailed || _boundRevision != _bindRevision || captured != _epoch || _runtime() == null) {
        throw StateError('Resolve the current profile before ambient playback.');
      }
    }
    await _invoke('start', {
      'server_origin': origin.toString(),
      'bearer_token': token,
      'channel_id': channel.id,
      'channel_name': channel.name,
      if (_android) ..._runtime()!,
    });
    if (captured != _epoch) throw StateError('The playback session changed.');
  }

  Future<void> stop() {
    return _invoke('stop');
  }

  Future<void> clear() async {
    if (_android) {
      scopeChanged();
      await _waitFence();
    } else {
      await _invoke('clear');
    }
  }

  Future<void> play() => _invoke('play');
  Future<void> pause() => _invoke('pause');
  Future<void> check() => _invoke('check');
  Future<void> seek(int positionMs) {
    if (positionMs < 0 || positionMs > 2147483647) throw ArgumentError.value(positionMs);
    return _invoke('seek', {'position_ms': positionMs});
  }

  Future<NativeAmbientPlaybackStatus> status() {
    if (_status != null) return _status!.timeout(const Duration(seconds: 5));
    final captured = _epoch;
    final work = () async {
      final data = await _channel.invokeMapMethod<Object?, Object?>('status', _android ? {'runtime': _runtime()} : null);
      if (captured != _epoch) throw StateError('The playback session changed.');
      return NativeAmbientPlaybackStatus.fromJson(data ?? const {});
    }();
    _status = work;
    work.then((_) { if (identical(_status, work)) _status = null; }, onError: (Object _) {
      if (identical(_status, work)) _status = null;
    });
    return work.timeout(const Duration(seconds: 5));
  }

  @override
  void dispose() {
    _disposed = true;
    unawaited(_subscription.cancel());
    super.dispose();
  }
}

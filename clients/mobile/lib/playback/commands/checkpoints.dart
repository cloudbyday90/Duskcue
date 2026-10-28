// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import 'wire.dart';

class PlaybackCheckpoints {
  PlaybackCheckpoints({
    required this.send,
    required this.read,
    required this.current,
    required this.now,
    required this.onUncertain,
  });
  final Future<Wire> Function(Wire) send;
  final Future<Wire> Function() read;
  final bool Function(String) current;
  final int Function() now;
  final void Function() onUncertain;
  int _epoch = 0, _sequence = 0, _last = -15000;
  Wire? _latest, confirmed;
  Future<Wire?>? _pending;
  Future<Wire?>? _forced;
  bool uncertain = false;

  bool observe(String output, int position, String state) {
    if (!current(output) ||
        position < 0 ||
        position > 2147483647 ||
        !const {'playing', 'paused', 'buffering'}.contains(state)) {
      return false;
    }
    final previous = _latest ?? confirmed;
    _latest = {
      'output_revision': output,
      'position_ms': position,
      'playback_state': state,
    };
    return previous?['output_revision'] != output ||
        previous?['playback_state'] != state;
  }

  Future<Wire?> flush({bool force = false}) {
    if (_pending != null) {
      if (!force) return _pending!;
      if (_forced != null) return _forced!;
      final epoch = _epoch;
      final work = _pending!.then(
        (_) => epoch == _epoch ? flush(force: true) : null,
      );
      _forced = work;
      work.then(
        (_) {
          if (identical(_forced, work)) _forced = null;
        },
        onError: (Object _) {
          if (identical(_forced, work)) _forced = null;
        },
      );
      return work;
    }
    if (uncertain ||
        _latest == null ||
        !current(_latest!['output_revision'] as String) ||
        !force && now() - _last < 15000) {
      return Future.value(confirmed);
    }
    final sample = _latest!;
    _latest = null;
    final epoch = _epoch;
    final body = {'protocol': vodProtocol, ...sample, 'sequence': ++_sequence};
    _last = now();
    final work = () async {
      try {
        final value = await send(body);
        if (epoch != _epoch) return null;
        require(
          value['protocol'] == vodProtocol &&
              value['sequence'] == body['sequence'] &&
              value['output_revision'] == body['output_revision'] &&
              value['position_ms'] == body['position_ms'] &&
              value['playback_state'] == body['playback_state'],
        );
        confirmed = value;
        return value;
      } catch (_) {
        if (epoch == _epoch) {
          uncertain = true;
          onUncertain();
        }
        rethrow;
      }
    }();
    _pending = work;
    work.then(
      (_) {
        if (identical(_pending, work)) _pending = null;
      },
      onError: (Object _) {
        if (identical(_pending, work)) _pending = null;
      },
    );
    return work;
  }

  Future<void> reconcile() async {
    final epoch = _epoch;
    try {
      await _pending;
    } catch (_) {}
    if (epoch != _epoch) return;
    final value = await read();
    if (epoch != _epoch) return;
    final sequence = integer(value['sequence'], max: 9007199254740991);
    require(
      value['protocol'] == vodProtocol &&
          sequence >= (confirmed?['sequence'] as int? ?? 0) &&
          (value['output_revision'] == null ||
              current(entity(value['output_revision'], v7: true))),
    );
    _sequence = _sequence > sequence ? _sequence : sequence;
    confirmed = value;
    uncertain = false;
  }

  void discard() => _latest = null;
  void reset() {
    _epoch++;
    _sequence = 0;
    _latest = null;
    confirmed = null;
    uncertain = false;
  }

  Future<void> drain() async {
    try {
      await _pending;
    } catch (_) {}
  }
}

// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import 'dart:math';
import 'package:uuid/data.dart';
import 'package:uuid/uuid.dart';
import 'wire.dart';

class PlaybackClock {
  PlaybackClock({int Function()? now, int Function()? wall, Random? random})
    : _now = now ?? (() => _watch.elapsedMilliseconds),
      _wall = wall ?? (() => DateTime.now().millisecondsSinceEpoch),
      _random = random ?? Random.secure();
  static final Stopwatch _watch = Stopwatch()..start();
  final int Function() _now;
  final int Function() _wall;
  final Random _random;
  int? _observed;
  int _began = 0;
  int _wallAt = 0;
  int get now => _now();
  void observe(DateTime value, int began) {
    require(began <= now);
    _observed = value.millisecondsSinceEpoch;
    _began = began;
    _wallAt = _wall() - (now - began);
  }

  int get elapsed {
    require(_observed != null && now >= _began, 'clock_unavailable');
    final elapsed = now - _began;
    require((_wall() - _wallAt - elapsed).abs() <= 2000, 'clock_unavailable');
    return elapsed;
  }

  bool get stale => _observed == null || elapsed > 30000;
  String id() {
    require(!stale, 'clock_unavailable');
    final at = _observed! + elapsed;
    require(at >= 0 && at < 281474976710656, 'clock_unavailable');
    return const Uuid().v7(
      config: V7Options(at, List.generate(16, (_) => _random.nextInt(256))),
    );
  }

  int remaining(DateTime deadline) => _observed == null
      ? -1
      : deadline.millisecondsSinceEpoch - _observed! - elapsed;
  void invalidate() => _observed = null;
}

// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import 'dart:async';
import 'dart:convert';
import 'dart:io';
import 'dart:typed_data';
import 'package:flutter_test/flutter_test.dart';
import 'package:duskcue_mobile/playback/relay/connection.dart';
import 'playback_relay_transport_test.dart' show DeferredRaw;
import 'relay_fixture.dart';

class ObservedTimer implements Timer {
  ObservedTimer(this.duration, this.callback);
  final Duration duration;
  final void Function() callback;
  @override
  bool isActive = true;
  @override
  int tick = 0;
  void fire() {
    if (isActive) {
      tick++;
      isActive = false;
      callback();
    }
  }

  @override
  void cancel() {
    isActive = false;
  }
}

void main() {
  test(
    'whole-request deadline retains destruction instead of treating timeout as release',
    () async {
      final raw = DeferredRaw(), timers = <ObservedTimer>[];
      final connection = RelayConnection(
        raw,
        createTimer: (duration, callback) {
          final timer = ObservedTimer(duration, callback);
          timers.add(timer);
          return timer;
        },
      );
      raw.pending = Uint8List.fromList(
        ascii.encode('GET / HTTP/1.1\r\nHost: local\r\n\r\n'),
      );
      raw.events.add(RawSocketEvent.read);
      await connection.headers;
      expect(timers.map((t) => t.duration.inSeconds), [5, 75]);
      expect(timers.first.isActive, isFalse);
      expect(connection.closed, isFalse);
      timers.last.fire();
      await eventually(() => raw.closing);
      expect(connection.closed, isTrue);
      var released = false;
      final done = connection.close().then((_) => released = true);
      await Future<void>.delayed(Duration.zero);
      expect(released, isFalse);
      raw.destruction.complete(raw);
      await done;
      await raw.events.close();
    },
  );
}

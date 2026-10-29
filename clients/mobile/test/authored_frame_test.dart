// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import 'dart:typed_data';
import 'dart:ui' show Rect;
import 'package:flutter_test/flutter_test.dart';
import '../integration_test/authored_frame.dart';

ByteData picture({bool black = false, int frame = 24}) {
  final pixels = ByteData(320 * 180 * 4);
  const patches = [0, 16, 32, 64, 128, 191, 223, 255];
  for (var y = 0; y < 180; y++) {
    for (var x = 0; x < 320; x++) {
      var value = 0;
      if (!black && x >= 16 && x < 272) {
        if (y >= 16 && y < 64) {
          value = patches[(x - 16) ~/ 32];
        }
        if (y >= 160 && y < 176) {
          value = frame & (1 << ((x - 16) ~/ 32)) == 0 ? 0 : 255;
        }
      }
      for (var channel = 0; channel < 3; channel++) {
        pixels.setUint8((y * 320 + x) * 4 + channel, value);
      }
      pixels.setUint8((y * 320 + x) * 4 + 3, 255);
    }
  }
  return pixels;
}

void main() {
  test(
    'native picture proof requires authored patches and a source frame near observed presentation',
    () {
      Map<String, Object> verify(ByteData pixels, {int time = 1000}) =>
          verifyAuthoredFrame(
            pixels,
            width: 320,
            height: 180,
            video: const Rect.fromLTWH(0, 0, 320, 180),
            scale: 1,
            beforeMs: time,
            afterMs: time + 100,
          );
      expect(verify(picture())['frame_id'], 24);
      expect(() => verify(picture(black: true)), throwsStateError);
      expect(() => verify(picture(frame: 220)), throwsStateError);
      expect(() => verify(picture(), time: 6000), throwsStateError);
      expect(() => verify(ByteData(16)), throwsStateError);
    },
  );
}

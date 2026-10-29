// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import 'dart:typed_data';
import 'dart:ui' show Rect;

Map<String, Object> verifyAuthoredFrame(
  ByteData pixels, {
  required int width,
  required int height,
  required Rect video,
  required double scale,
  required int beforeMs,
  required int afterMs,
}) {
  void check(bool value) {
    if (!value) throw StateError('Authored native frame evidence is invalid');
  }

  check(width > 0 && height > 0 && width * height * 4 <= 16 * 1024 * 1024);
  check(pixels.lengthInBytes == width * height * 4);
  check(scale.isFinite && scale > 0 && video.width > 0 && video.height > 0);
  check(beforeMs >= 0 && afterMs >= beforeMs);
  double sample(double sourceX, double sourceY) {
    final channels = [0, 0, 0];
    for (final dx in [-3, 0, 3]) {
      for (final dy in [-2, 0, 2]) {
        final x = ((video.left + video.width * (sourceX + dx) / 320) * scale)
            .round();
        final y = ((video.top + video.height * (sourceY + dy) / 180) * scale)
            .round();
        check(x >= 0 && x < width && y >= 0 && y < height);
        for (var channel = 0; channel < 3; channel++) {
          channels[channel] += pixels.getUint8((y * width + x) * 4 + channel);
        }
      }
    }
    final rgb = channels.map((value) => value / 9).toList();
    check((rgb[0] - rgb[1]).abs() <= 14 && (rgb[1] - rgb[2]).abs() <= 14);
    return (rgb[0] + rgb[1] + rgb[2]) / 3;
  }

  const expected = [0, 16, 32, 64, 128, 191, 223, 255];
  final patches = [
    for (var index = 0; index < 8; index++) sample(32.0 + index * 32, 40),
  ];
  for (var index = 0; index < expected.length; index++) {
    check((patches[index] - expected[index]).abs() <= 18);
  }
  var frame = 0;
  for (var bit = 0; bit < 8; bit++) {
    final value = sample(32.0 + bit * 32, 168);
    check(value <= 48 || value >= 207);
    if (value >= 207) frame |= 1 << bit;
  }
  final presentedMs = frame * 1000 / 24;
  check(
    frame < 192 &&
        presentedMs >= beforeMs - 1000 &&
        presentedMs <= afterMs + 1000,
  );
  return {
    'frame_id': frame,
    'position_before_ms': beforeMs,
    'position_after_ms': afterMs,
    'gray_patches': patches,
  };
}

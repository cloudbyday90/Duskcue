// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import 'wire.dart';

Wire androidSelection(
  String item, {
  String? file,
  int? audio,
  int? subtitle,
  int height = 1080,
}) {
  entity(item);
  if (file != null) entity(file);
  require(const {480, 720, 1080}.contains(height));
  if (audio != null) integer(audio);
  if (subtitle != null) integer(subtitle);
  return {
    'media_item_id': item,
    if (file != null) 'media_file_id': file,
    'audio_stream_index': audio,
    'subtitle_stream_index': subtitle,
    'max_streaming_bitrate': height == 480
        ? 2000000
        : height == 720
        ? 5000000
        : 12000000,
    'quality_mode': 'manual',
    'force_transcode': true,
    'force_hls': true,
    'playback_mode': 'interactive',
    'device_profile': {
      'client': 'duskcue_mobile',
      'platform': 'android',
      'video_codecs': ['h264'],
      'audio_codecs': ['aac'],
      'subtitle_formats': ['webvtt'],
      'max_resolution': '${height}p',
      'max_audio_channels': 2,
      'hls_supported': true,
      'hdr_supported': false,
    },
  };
}

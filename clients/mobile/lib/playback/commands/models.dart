// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import 'scope.dart';
import 'wire.dart';

class VodReady {
  const VodReady(
    this.output,
    this.history,
    this.item,
    this.file,
    this.position,
    this.offset,
    this.audio,
    this.subtitle,
  );
  final String output, history, item, file;
  final int position, offset;
  final int? audio, subtitle;
  factory VodReady.parse(Object? input) {
    final value = object(input);
    require(value['playback_mode'] == 'interactive');
    require(value['delivery_mode'] == 'transcode', 'unqualified_output');
    return VodReady(
      entity(value['output_revision'], v7: true),
      entity(value['session_id'], v7: true),
      entity(value['media_item_id']),
      entity(value['media_file_id']),
      integer(value['position_ms']),
      integer(value['stream_offset_ms']),
      value['audio_stream_index'] == null
          ? null
          : integer(value['audio_stream_index']),
      value['subtitle_stream_index'] == null
          ? null
          : integer(value['subtitle_stream_index']),
    );
  }
}

class VodReceipt {
  const VodReceipt(
    this.status,
    this.observed,
    this.deadline,
    this.ready,
    this.stream,
  );
  final String status;
  final DateTime observed;
  final DateTime? deadline;
  final VodReady? ready;
  final Uri? stream;
  factory VodReceipt.parse(Wire value, String id, Uri origin) {
    require(value['protocol'] == vodProtocol && value['request_id'] == id);
    resource(origin, value['status_url'], commandPath(id));
    final status = text(value['status']);
    require(
      const {
        'unknown',
        'preparing',
        'ready',
        'changing',
        'unavailable',
        'uncertain',
        'stopping',
        'released',
        'failed',
        'withdrawing',
        'withdrawn',
      }.contains(status),
    );
    final observed = instant(value['observed_at']);
    final deadline = value['lease_deadline'] == null
        ? null
        : instant(value['lease_deadline']);
    if (status != 'ready') {
      require(value['ready'] == null && value['stream_url'] == null);
      return VodReceipt(status, observed, deadline, null, null);
    }
    final ready = VodReady.parse(value['ready']);
    require(deadline != null);
    return VodReceipt(
      status,
      observed,
      deadline,
      ready,
      resource(
        origin,
        value['stream_url'],
        commandPath(id, '/outputs/${ready.output}/hls/manifest.m3u8'),
      ),
    );
  }
}

class VodSettlement {
  const VodSettlement(this.status);
  final String status;
  bool get settled => status == 'settled';
  factory VodSettlement.parse(Wire value, String id, Uri origin) {
    require(value['protocol'] == vodProtocol && value['request_id'] == id);
    resource(origin, value['status_url'], commandPath(id, '/settlement'));
    instant(value['observed_at']);
    final status = text(value['status']);
    require(const {'pending', 'uncertain', 'settled'}.contains(status));
    final fenced = boolean(value['admission_fenced']);
    final completed = boolean(value['preparation_complete']);
    require(
      value['parent_released'] == null || value['parent_released'] is bool,
    );
    require(
      status != 'settled' ||
          fenced && completed && value['parent_released'] != false,
    );
    return VodSettlement(status);
  }
}

class VodRecord {
  const VodRecord({
    required this.scope,
    required this.start,
    this.operation,
    this.output,
    this.phase = 'active',
  });
  final PlaybackScope scope;
  final String start;
  final String? operation, output;
  final String phase;
  factory VodRecord.parse(Wire value) {
    exactKeys(value, {
      'version',
      'scope',
      'start',
      'operation',
      'output',
      'phase',
    });
    require(
      value['version'] == 1 &&
          const {'active', 'uncertain', 'cleanup'}.contains(value['phase']),
    );
    return VodRecord(
      scope: PlaybackScope.fromJson(object(value['scope'])),
      start: entity(value['start'], v7: true),
      operation: value['operation'] == null
          ? null
          : entity(value['operation'], v7: true),
      output: value['output'] == null
          ? null
          : entity(value['output'], v7: true),
      phase: value['phase'] as String,
    );
  }
  Wire toJson() => {
    'version': 1,
    'scope': scope.toJson(),
    'start': start,
    'operation': operation,
    'output': output,
    'phase': phase,
  };
  VodRecord copy({
    String? operation,
    bool clearOperation = false,
    String? output,
    String? phase,
  }) => VodRecord(
    scope: scope,
    start: start,
    operation: clearOperation ? null : operation ?? this.operation,
    output: output ?? this.output,
    phase: phase ?? this.phase,
  );
}

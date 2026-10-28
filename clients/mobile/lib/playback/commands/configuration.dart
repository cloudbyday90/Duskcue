// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import 'models.dart';
import 'transport.dart';
import 'wire.dart';

class VodConfiguration {
  const VodConfiguration(this.value, this.etag);
  final Wire value;
  final String etag;
  String get output => value['output_revision'] as String;
  bool get workerRecovery => value['worker_recovery'] == true;
  static Future<VodConfiguration> read(
    PlaybackTransport transport,
    VodRecord record,
  ) async {
    final reply = await transport.call(
      'GET',
      commandPath(record.start, '/configuration'),
      record.scope,
    );
    final value = reply.body;
    require(
      value['protocol'] == vodProtocol &&
          value['request_id'] == record.start &&
          const {
            'preparing',
            'ready',
            'changing',
            'unavailable',
            'uncertain',
            'retiring',
          }.contains(value['state']) &&
          value['worker_recovery'] is bool &&
          reply.etag != null &&
          RegExp(r'^"vod-config-[a-f0-9]{64}"$').hasMatch(reply.etag!),
    );
    entity(value['output_revision'], v7: true);
    revisionValue(value['revision']);
    if (value['pending_operation'] != null) {
      entity(value['pending_operation'], v7: true);
    }
    return VodConfiguration(value, reply.etag!);
  }
}

Wire operation(VodReply reply, VodRecord record) {
  final value = reply.body;
  require(
    value['protocol'] == vodProtocol &&
        value['request_id'] == record.start &&
        value['operation_id'] == record.operation &&
        value['cleanup_complete'] is bool &&
        const {
          'unknown',
          'accepted',
          'preparing',
          'completed',
          'failed',
          'retiring',
          'cancelled',
          'uncertain',
        }.contains(value['status']),
  );
  resource(
    record.scope.origin,
    value['status_url'],
    commandPath(record.start, '/operations/${record.operation}'),
  );
  if (reply.status == 202) {
    resource(
      record.scope.origin,
      reply.location,
      value['status_url'] as String,
    );
  }
  return value;
}

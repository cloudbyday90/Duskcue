// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import 'dart:io';
import 'package:flutter_test/flutter_test.dart';
import 'package:duskcue_mobile/playback/commands/wire.dart';
import '../integration_test/scope_support.dart';

void main() {
  for (final coded in [false, true]) {
    test(
      'scope witness requests identity and ${coded ? 'refuses coding' : 'reads real problem responses'}',
      () async {
        final server = await HttpServer.bind(InternetAddress.loopbackIPv4, 0);
        final origin = Uri.parse('http://127.0.0.1:${server.port}');
        server.listen((request) async {
          expect(request.headers.value('accept-encoding'), 'identity');
          expect(request.headers.value('authorization'), 'Bearer fixture-only');
          request.response.statusCode = 401;
          request.response.headers.contentType = ContentType(
            'application',
            'problem+json',
          );
          if (coded) request.response.headers.set('content-encoding', 'gzip');
          request.response.write('{"status":401}');
          await request.response.close();
        });
        try {
          if (coded) {
            await expectLater(
              scopeHttp(origin, 'fixture-only', '/api/v1/auth/session-context'),
              throwsA(isA<VodFailure>()),
            );
          } else {
            final result = await scopeHttp(
              origin,
              'fixture-only',
              '/api/v1/auth/session-context',
            );
            expect(result.$1, 401);
            expect(result.$2['status'], 401);
          }
        } finally {
          await server.close(force: true);
        }
      },
    );
  }
}

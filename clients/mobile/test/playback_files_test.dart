// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import 'package:flutter_test/flutter_test.dart';
import 'package:duskcue_mobile/playback/commands/wire.dart';
import 'playback_commands_test.dart' show CommandFixture;

void main() {
  test(
    'media-file adoption rejects a server-side profile-context change',
    () async {
      final fixture = CommandFixture();
      await fixture.open();
      try {
        final scope = (await fixture.transport.context()).scope;
        fixture.filePayload = [
          {'id': fixture.file},
        ];
        fixture.fileContextRevision = '2';
        await expectLater(
          fixture.transport.files(fixture.item, scope),
          throwsA(
            isA<VodFailure>().having(
              (error) => error.code,
              'code',
              'scope_changed',
            ),
          ),
        );
        expect(fixture.starts, 0);
      } finally {
        await fixture.close();
      }
    },
  );
  test(
    'media-file collection preserves bounded array shape and refuses wrappers or exhausted limits',
    () async {
      final fixture = CommandFixture();
      await fixture.open();
      try {
        final scope = (await fixture.transport.context()).scope;
        fixture.filePayload = [
          {
            'id': fixture.file,
            'additional_streams': {'audio': [], 'subtitles': []},
          },
        ];
        expect(
          (await fixture.transport.files(fixture.item, scope)).single['id'],
          fixture.file,
        );
        fixture.filePayload = [];
        expect(await fixture.transport.files(fixture.item, scope), isEmpty);
        for (final invalid in [
          {'items': []},
          List.generate(129, (_) => {'id': fixture.file}),
          [
            {'id': fixture.file, 'unexpected': 'x' * 32768},
          ],
          [List.generate(1, (_) => 'not an object')],
        ]) {
          fixture.filePayload = invalid;
          await expectLater(
            fixture.transport.files(fixture.item, scope),
            throwsA(isA<VodFailure>()),
          );
        }
        expect(fixture.starts, 0);
      } finally {
        await fixture.close();
      }
    },
  );
}

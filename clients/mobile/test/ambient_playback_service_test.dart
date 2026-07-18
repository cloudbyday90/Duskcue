// Copyright (C) 2026-2026 Duskcue Contributors
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

import 'package:duskcue_mobile/models/profile_models.dart';
import 'package:duskcue_mobile/services/ambient_playback_service.dart';
import 'package:duskcue_mobile/services/api_client.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();

  const methodChannel = MethodChannel('duskcue/ambient_player');
  final messenger =
      TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger;

  tearDown(() {
    messenger.setMockMethodCallHandler(methodChannel, null);
  });

  test('passes only in-memory session runtime to the native player', () async {
    MethodCall? call;
    messenger.setMockMethodCallHandler(methodChannel, (methodCall) async {
      call = methodCall;
      return null;
    });
    final client = DuskcueApiClient()
      ..configure(
        Uri.parse('https://duskcue.example'),
        bearerToken: 'session-token',
      );
    final service = AmbientPlaybackService(
      apiClient: client,
      channel: methodChannel,
    );

    await service.start(
      const AmbientChannelSummary(
        id: 'channel-1',
        name: 'Documentaries',
        audience: 'standard',
        isEnabled: true,
        itemCount: 3,
      ),
    );

    expect(call?.method, 'start');
    expect(call?.arguments, {
      'server_origin': 'https://duskcue.example',
      'bearer_token': 'session-token',
      'channel_id': 'channel-1',
      'channel_name': 'Documentaries',
    });
  });

  test(
    'fails before invoking native playback without an authenticated session',
    () async {
      var invoked = false;
      messenger.setMockMethodCallHandler(methodChannel, (methodCall) async {
        invoked = true;
        return null;
      });
      final service = AmbientPlaybackService(
        apiClient: DuskcueApiClient(),
        channel: methodChannel,
      );

      await expectLater(
        service.start(
          const AmbientChannelSummary(
            id: 'channel-1',
            name: 'Documentaries',
            audience: 'standard',
            isEnabled: true,
            itemCount: 3,
          ),
        ),
        throwsA(isA<StateError>()),
      );

      expect(invoked, isFalse);
    },
  );

  test(
    'maps native ambient status without treating absent values as strings',
    () async {
      messenger.setMockMethodCallHandler(methodChannel, (methodCall) async {
        expect(methodCall.method, 'status');
        return {
          'is_active': true,
          'channel_id': 'channel-1',
          'channel_name': 'Documentaries',
          'media_item_id': 'item-1',
          'position_ms': 42000,
          'is_playing': true,
          'error': null,
        };
      });
      final service = AmbientPlaybackService(
        apiClient: DuskcueApiClient(),
        channel: methodChannel,
      );

      final status = await service.status();

      expect(status.isActive, isTrue);
      expect(status.channelId, 'channel-1');
      expect(status.positionMs, 42000);
      expect(status.error, isNull);
    },
  );
}

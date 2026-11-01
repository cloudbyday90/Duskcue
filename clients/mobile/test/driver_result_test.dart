// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import 'dart:convert';
import 'dart:io';
import 'package:flutter_driver/flutter_driver.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/common.dart';
import '../test_driver/vod_commands.dart';

class ResultDriver implements FlutterDriver {
  final Future<String> result;
  int requests = 0;
  int closes = 0;

  ResultDriver(this.result);

  @override
  Future<String> requestData(String? message, {Duration? timeout}) {
    requests++;
    expect(message, isNull);
    expect(timeout, const Duration(minutes: 3));
    return result;
  }

  @override
  Future<void> close() async {
    closes++;
  }

  @override
  dynamic noSuchMethod(Invocation invocation) => super.noSuchMethod(invocation);
}

void main() {
  test(
    'public SDK result sequence requests once, preserves failures and observes close',
    () async {
      for (final result in [
        Response.allTestsPassed(data: {'scenario': 'baseline'}),
        Response.someTestsFailed([Failure('journey', 'actual failure')]),
      ]) {
        final driver = ResultDriver(Future.value(result.toJson()));
        final received = await readNativeResult(driver);
        expect(received.allTestsPassed, result.allTestsPassed);
        if (!result.allTestsPassed) {
          expect(received.formattedFailureDetails, contains('actual failure'));
        }
        expect(driver.requests, 1);
        expect(driver.closes, 1);
      }
    },
  );

  test(
    'failed and malformed result reads still close without replay',
    () async {
      for (final result in [
        Future<String>.error(StateError('disconnected')),
        Future.value('invalid'),
      ]) {
        final driver = ResultDriver(result);
        await expectLater(readNativeResult(driver), throwsA(anything));
        expect(driver.requests, 1);
        expect(driver.closes, 1);
      }
    },
  );

  test(
    'bounded stage evidence retains screenshot checks and rejects wrong or oversized results',
    () async {
      final directory = await Directory.systemTemp.createTemp(
        'duskcue-driver-result-',
      );
      try {
        await writeResult(directory.path, 'baseline', {
          'scenario': 'baseline',
          'screenshots': [
            {
              'screenshotName': 'android-decoded-frame',
              'bytes': [137, 80, 78, 71],
            },
          ],
        });
        expect(
          await File('${directory.path}/decoded-frame.png').readAsBytes(),
          [137, 80, 78, 71],
        );
        expect(
          jsonDecode(
            await File('${directory.path}/native-result.json').readAsString(),
          ),
          {'scenario': 'baseline'},
        );
        for (final data in <Map<String, dynamic>?>[
          null,
          {'scenario': 'different'},
          {'scenario': 'baseline'},
          {
            'scenario': 'baseline',
            'screenshots': [
              {
                'screenshotName': 'different',
                'bytes': [1],
              },
            ],
          },
        ]) {
          await expectLater(
            writeResult(directory.path, 'baseline', data),
            throwsStateError,
          );
        }
        await expectLater(
          writeResult(directory.path, 'scope-profile', {
            'scenario': 'scope-profile',
            'screenshots': [1],
          }),
          throwsStateError,
        );
        await expectLater(
          writeResult(directory.path, 'scope-profile', {
            'scenario': 'scope-profile',
            'oversized': 'x' * 4096,
          }),
          throwsStateError,
        );
      } finally {
        expect(
          directory.parent.resolveSymbolicLinksSync(),
          Directory.systemTemp.resolveSymbolicLinksSync(),
        );
        expect(
          directory.uri.pathSegments
              .where((part) => part.startsWith('duskcue-driver-result-'))
              .length,
          1,
        );
        await directory.delete(recursive: true);
      }
    },
  );
}

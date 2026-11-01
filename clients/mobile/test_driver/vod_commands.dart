// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import 'dart:convert';
import 'dart:io';
import 'package:flutter_driver/flutter_driver.dart';
import 'package:integration_test/common.dart';

Future<void> main() async {
  final directory = Platform.environment['DUSKCUE_ANDROID_EVIDENCE'];
  if (directory == null || !Directory(directory).existsSync()) {
    throw StateError('Owned Android evidence directory is required');
  }
  final scenario = Platform.environment['DUSKCUE_ANDROID_SCENARIO'];
  if (scenario == null) throw StateError('Owned Android scenario is required');
  final driver = await FlutterDriver.connect(logCommunicationToFile: false);
  final response = await readNativeResult(driver);
  if (!response.allTestsPassed) {
    stderr.writeln('Failure Details:\n${response.formattedFailureDetails}');
    exit(1);
  }
  await writeResult(directory, scenario, response.data);
  stdout.writeln('All tests passed.');
}

Future<Response> readNativeResult(FlutterDriver driver) async {
  late final Response response;
  try {
    final result = await driver.requestData(
      null,
      timeout: const Duration(minutes: 3),
    );
    response = Response.fromJson(result);
  } finally {
    await driver.close();
  }
  return response;
}

Future<void> writeResult(
  String directory,
  String scenario,
  Map<String, dynamic>? data,
) async {
  if (data == null) throw StateError('Native result is missing');
  if (data['scenario'] != scenario) {
    throw StateError('Native scenario does not match its owned stage');
  }
  final screenshots = data.remove('screenshots');
  if (scenario == 'baseline') {
    if (screenshots is! List || screenshots.length != 1) {
      throw StateError('One decoded-frame screenshot is required');
    }
    final image = screenshots.single as Map<String, dynamic>;
    final bytes = (image['bytes'] as List).cast<int>();
    if (bytes.length > 16 * 1024 * 1024 ||
        image['screenshotName'] != 'android-decoded-frame') {
      throw StateError('Native screenshot exceeds its evidence scope');
    }
    await File('$directory/decoded-frame.png').writeAsBytes(bytes);
  } else if (screenshots != null &&
      (screenshots is! List || screenshots.isNotEmpty)) {
    throw StateError('Unexpected screenshot outside decoded-media stage');
  }
  final encoded = utf8.encode(jsonEncode(data));
  if (encoded.length > 4096) throw StateError('Native result is oversized');
  await File('$directory/native-result.json').writeAsBytes(encoded);
}

// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import 'dart:convert';
import 'dart:io';
import 'package:integration_test/integration_test_driver.dart';

Future<void> main() async {
  final directory = Platform.environment['DUSKCUE_ANDROID_EVIDENCE'];
  if (directory == null || !Directory(directory).existsSync()) {
    throw StateError('Owned Android evidence directory is required');
  }
  await integrationDriver(
    timeout: const Duration(minutes: 3),
    responseDataCallback: (data) async {
      if (data == null) throw StateError('Native result is missing');
      final screenshots = data.remove('screenshots');
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
      final encoded = utf8.encode(jsonEncode(data));
      if (encoded.length > 4096) throw StateError('Native result is oversized');
      await File('$directory/native-result.json').writeAsBytes(encoded);
    },
  );
}

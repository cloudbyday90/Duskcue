// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import 'dart:io';
import 'package:flutter/material.dart';
import 'command_playback_screen.dart';
import 'legacy_playback_entry_screen.dart';

class PlaybackEntryScreen extends StatelessWidget {
  const PlaybackEntryScreen({
    required this.itemId,
    this.offline = false,
    super.key,
  });
  final String itemId;
  final bool offline;
  @override
  Widget build(BuildContext context) => !offline && Platform.isAndroid
      ? CommandPlaybackScreen(itemId: itemId)
      : LegacyPlaybackEntryScreen(itemId: itemId, offline: offline);
}

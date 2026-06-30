import 'package:duskcue_mobile/l10n/app_strings.dart';
import 'package:duskcue_mobile/widgets/mobile_state_views.dart';
import 'package:flutter/material.dart';

class PlaybackEntryScreen extends StatelessWidget {
  const PlaybackEntryScreen({required this.itemId, super.key});

  final String itemId;

  @override
  Widget build(BuildContext context) {
    final strings = AppStrings.of(context);

    return Scaffold(
      appBar: AppBar(title: Text(strings.playbackEntry)),
      body: SafeArea(
        child: EmptyState(
          icon: Icons.play_circle_outline,
          message: '${strings.playbackTaskNotice}\n\n$itemId',
        ),
      ),
    );
  }
}

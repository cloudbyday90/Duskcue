import 'package:duskcue_mobile/stores/session_store.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';

class DashboardScreen extends ConsumerWidget {
  const DashboardScreen({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final session = ref.watch(sessionProvider);

    return Scaffold(
      appBar: AppBar(
        title: const Text('Duskcue'),
        actions: [
          IconButton(
            onPressed: () => context.go('/settings'),
            icon: const Icon(Icons.settings_outlined),
          ),
        ],
      ),
      body: SafeArea(
        child: ListView(
          padding: const EdgeInsets.all(24),
          children: [
            Text(
              session.server?.origin.toString() ?? 'No server selected',
              style: Theme.of(context).textTheme.titleMedium,
            ),
            if (session.server != null) ...[
              const SizedBox(height: 8),
              Text(session.server!.networkMode.label),
            ],
            const SizedBox(height: 16),
            const Text('Library browsing, search, playback, notifications, and quality reporting land in later Phase 16a tasks.'),
          ],
        ),
      ),
    );
  }
}

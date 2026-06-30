import 'package:duskcue_mobile/models/server_profile.dart';
import 'package:duskcue_mobile/stores/session_store.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';

class ServerSelectionScreen extends ConsumerStatefulWidget {
  const ServerSelectionScreen({super.key});

  @override
  ConsumerState<ServerSelectionScreen> createState() => _ServerSelectionScreenState();
}

class _ServerSelectionScreenState extends ConsumerState<ServerSelectionScreen> {
  final TextEditingController _controller =
      TextEditingController(text: 'http://localhost:48027');

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  void _continue() {
    final origin = Uri.tryParse(_controller.text.trim());
    if (origin == null || !origin.hasScheme || origin.host.isEmpty) {
      return;
    }

    ref.read(sessionProvider.notifier).selectServer(ServerProfile(origin: origin));
    context.go('/dashboard');
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(title: const Text('Duskcue')),
      body: SafeArea(
        child: Padding(
          padding: const EdgeInsets.all(24),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              TextField(
                controller: _controller,
                keyboardType: TextInputType.url,
                decoration: const InputDecoration(
                  labelText: 'Server URL',
                  border: OutlineInputBorder(),
                ),
                onSubmitted: (_) => _continue(),
              ),
              const SizedBox(height: 16),
              FilledButton(
                onPressed: _continue,
                child: const Text('Continue'),
              ),
            ],
          ),
        ),
      ),
    );
  }
}

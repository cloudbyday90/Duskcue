import 'package:duskcue_mobile/api/client_error.dart';
import 'package:duskcue_mobile/l10n/app_strings.dart';
import 'package:duskcue_mobile/models/content_models.dart';
import 'package:duskcue_mobile/services/service_providers.dart';
import 'package:duskcue_mobile/stores/session_store.dart';
import 'package:duskcue_mobile/widgets/mobile_state_views.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';

class LibrariesScreen extends ConsumerStatefulWidget {
  const LibrariesScreen({super.key});

  @override
  ConsumerState<LibrariesScreen> createState() => _LibrariesScreenState();
}

class _LibrariesScreenState extends ConsumerState<LibrariesScreen> {
  List<LibrarySummary> _items = const [];
  bool _loading = true;
  Object? _error;

  @override
  void initState() {
    super.initState();
    _load();
  }

  Future<void> _load() async {
    setState(() {
      _loading = true;
      _error = null;
    });
    try {
      final page = await ref.read(contentServiceProvider).listLibraries();
      if (!mounted) return;
      setState(() {
        _items = page.items;
        _loading = false;
      });
    } on ClientError catch (error) {
      if (error.kind == ClientErrorKind.authExpired) {
        ref.read(authServiceProvider).clearLocalSession();
        ref.read(sessionProvider.notifier).clearAuthentication();
        if (mounted) context.go('/auth');
        return;
      }
      _setError(error);
    } catch (error) {
      _setError(error);
    }
  }

  void _setError(Object error) {
    if (!mounted) return;
    setState(() {
      _error = error;
      _loading = false;
    });
  }

  @override
  Widget build(BuildContext context) {
    final strings = AppStrings.of(context);

    return Scaffold(
      appBar: AppBar(title: Text(strings.libraries)),
      body: SafeArea(
        child: _loading
            ? const Center(child: CircularProgressIndicator())
            : _error != null
                ? ErrorState(message: userFacingError(context, _error!), onRetry: _load)
                : RefreshIndicator(
                    onRefresh: _load,
                    child: _items.isEmpty
                        ? ListView(children: [EmptyState(icon: Icons.video_library_outlined, message: strings.emptyLibraries)])
                        : ListView.separated(
                            padding: const EdgeInsets.all(16),
                            itemCount: _items.length,
                            separatorBuilder: (context, index) => const Divider(height: 1),
                            itemBuilder: (context, index) {
                              final library = _items[index];
                              return ListTile(
                                leading: const Icon(Icons.video_library_outlined),
                                title: Text(library.name),
                                subtitle: Text([library.kind, if (library.itemCount != null) '${library.itemCount} items'].whereType<String>().join(' · ')),
                                trailing: const Icon(Icons.chevron_right),
                                onTap: () => context.go('/libraries/${library.id}'),
                              );
                            },
                          ),
                  ),
      ),
    );
  }
}

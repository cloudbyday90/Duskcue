import 'package:duskcue_mobile/api/client_error.dart';
import 'package:duskcue_mobile/l10n/app_strings.dart';
import 'package:duskcue_mobile/models/content_models.dart';
import 'package:duskcue_mobile/services/service_providers.dart';
import 'package:duskcue_mobile/stores/session_store.dart';
import 'package:duskcue_mobile/widgets/content_cards.dart';
import 'package:duskcue_mobile/widgets/mobile_state_views.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';

class DashboardScreen extends ConsumerStatefulWidget {
  const DashboardScreen({super.key});

  @override
  ConsumerState<DashboardScreen> createState() => _DashboardScreenState();
}

class _DashboardScreenState extends ConsumerState<DashboardScreen> {
  List<LibrarySummary> _libraries = const [];
  List<MediaItemSummary> _recent = const [];
  int _unread = 0;
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
      final service = ref.read(contentServiceProvider);
      final results = await Future.wait([
        service.listLibraries(),
        service.listMediaItems(limit: 12),
        service.unreadNotificationCount(),
      ]);
      if (!mounted) return;
      setState(() {
        _libraries = (results[0] as PageResult<LibrarySummary>).items;
        _recent = (results[1] as PageResult<MediaItemSummary>).items;
        _unread = results[2] as int;
        _loading = false;
      });
    } on ClientError catch (error) {
      _handleError(error);
    } catch (error) {
      if (!mounted) return;
      setState(() {
        _error = error;
        _loading = false;
      });
    }
  }

  void _handleError(ClientError error) {
    if (error.kind == ClientErrorKind.authExpired) {
      ref.read(authServiceProvider).clearLocalSession();
      ref.read(sessionProvider.notifier).clearAuthentication();
      if (mounted) context.go('/auth');
      return;
    }
    if (!mounted) return;
    setState(() {
      _error = error;
      _loading = false;
    });
  }

  @override
  Widget build(BuildContext context) {
    final strings = AppStrings.of(context);
    final session = ref.watch(sessionProvider);

    return Scaffold(
      appBar: AppBar(title: Text(strings.dashboard)),
      body: SafeArea(
        child: _loading
            ? const Center(child: CircularProgressIndicator())
            : _error != null
                ? ErrorState(message: userFacingError(context, _error!), onRetry: _load)
                : RefreshIndicator(
                    onRefresh: _load,
                    child: ListView(
                      padding: const EdgeInsets.all(16),
                      children: [
                        Text(session.server?.origin.toString() ?? strings.noServerSelected, style: Theme.of(context).textTheme.titleMedium),
                        if (session.user != null) ...[
                          const SizedBox(height: 4),
                          Text(session.user!.displayName),
                        ],
                        const SizedBox(height: 20),
                        Wrap(
                          spacing: 12,
                          runSpacing: 12,
                          children: [
                            ActionChip(
                              avatar: const Icon(Icons.video_library_outlined),
                              label: Text('${strings.libraries}: ${_libraries.length}'),
                              onPressed: () => context.go('/libraries'),
                            ),
                            ActionChip(
                              avatar: const Icon(Icons.notifications_outlined),
                              label: Text('${strings.notifications}: $_unread'),
                              onPressed: () => context.go('/notifications'),
                            ),
                          ],
                        ),
                        const SizedBox(height: 24),
                        Text(strings.recentlyAdded, style: Theme.of(context).textTheme.titleLarge),
                        const SizedBox(height: 12),
                        if (_recent.isEmpty)
                          EmptyState(icon: Icons.movie_outlined, message: strings.emptyItems)
                        else
                          GridView.builder(
                            shrinkWrap: true,
                            physics: const NeverScrollableScrollPhysics(),
                            gridDelegate: const SliverGridDelegateWithFixedCrossAxisCount(
                              crossAxisCount: 2,
                              childAspectRatio: 0.58,
                              crossAxisSpacing: 12,
                              mainAxisSpacing: 12,
                            ),
                            itemCount: _recent.length,
                            itemBuilder: (context, index) {
                              final item = _recent[index];
                              return MediaItemCard(
                                item: item,
                                onTap: () => context.go('/media/${item.id}'),
                              );
                            },
                          ),
                      ],
                    ),
                  ),
      ),
    );
  }
}

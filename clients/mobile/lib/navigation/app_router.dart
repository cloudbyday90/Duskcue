// Duskcue — Self-hosted media streaming server
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

import 'package:duskcue_mobile/screens/collection_detail_screen.dart';
import 'package:duskcue_mobile/screens/collections_screen.dart';
import 'package:duskcue_mobile/screens/auth_screen.dart';
import 'package:duskcue_mobile/screens/dashboard_screen.dart';
import 'package:duskcue_mobile/screens/downloads_screen.dart';
import 'package:duskcue_mobile/screens/libraries_screen.dart';
import 'package:duskcue_mobile/screens/library_detail_screen.dart';
import 'package:duskcue_mobile/screens/media_detail_screen.dart';
import 'package:duskcue_mobile/screens/notifications_screen.dart';
import 'package:duskcue_mobile/screens/playback_entry_screen.dart';
import 'package:duskcue_mobile/screens/search_screen.dart';
import 'package:duskcue_mobile/screens/server_selection_screen.dart';
import 'package:duskcue_mobile/screens/settings_screen.dart';
import 'package:duskcue_mobile/stores/session_store.dart';
import 'package:duskcue_mobile/widgets/app_shell.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';

final appRouterProvider = Provider<GoRouter>((ref) {
  final session = ref.watch(sessionProvider);

  return GoRouter(
    initialLocation: '/server',
    redirect: (context, state) {
      final path = state.uri.path;
      final setupPath = path == '/server' || path == '/auth';

      if (session.server == null && path != '/server') {
        return '/server';
      }
      if (session.server != null && !session.isAuthenticated && !setupPath) {
        return '/auth';
      }
      if (session.isAuthenticated && setupPath) {
        return '/dashboard';
      }
      return null;
    },
    routes: [
      GoRoute(
        path: '/server',
        builder: (context, state) => const ServerSelectionScreen(),
      ),
      GoRoute(
        path: '/auth',
        builder: (context, state) => const AuthScreen(),
      ),
      StatefulShellRoute.indexedStack(
        builder: (context, state, navigationShell) => AppShell(navigationShell: navigationShell),
        branches: [
          StatefulShellBranch(
            routes: [
              GoRoute(
                path: '/dashboard',
                builder: (context, state) => const DashboardScreen(),
              ),
            ],
          ),
          StatefulShellBranch(
            routes: [
              GoRoute(
                path: '/libraries',
                builder: (context, state) => const LibrariesScreen(),
              ),
            ],
          ),
          StatefulShellBranch(
            routes: [
              GoRoute(
                path: '/search',
                builder: (context, state) => const SearchScreen(),
              ),
            ],
          ),
          StatefulShellBranch(
            routes: [
              GoRoute(
                path: '/collections',
                builder: (context, state) => const CollectionsScreen(),
              ),
            ],
          ),
          StatefulShellBranch(
            routes: [
              GoRoute(
                path: '/downloads',
                builder: (context, state) => const DownloadsScreen(),
              ),
            ],
          ),
          StatefulShellBranch(
            routes: [
              GoRoute(
                path: '/notifications',
                builder: (context, state) => const NotificationsScreen(),
              ),
            ],
          ),
          StatefulShellBranch(
            routes: [
              GoRoute(
                path: '/settings',
                builder: (context, state) => const SettingsScreen(),
              ),
            ],
          ),
        ],
      ),
      GoRoute(
        path: '/libraries/:libraryId',
        builder: (context, state) => LibraryDetailScreen(libraryId: state.pathParameters['libraryId'] ?? ''),
      ),
      GoRoute(
        path: '/media/:itemId',
        builder: (context, state) => MediaDetailScreen(itemId: state.pathParameters['itemId'] ?? ''),
      ),
      GoRoute(
        path: '/collections/:collectionId',
        builder: (context, state) => CollectionDetailScreen(collectionId: state.pathParameters['collectionId'] ?? ''),
      ),
      GoRoute(
        path: '/play/:itemId',
        builder: (context, state) => PlaybackEntryScreen(
          itemId: state.pathParameters['itemId'] ?? '',
          offline: state.uri.queryParameters['offline'] == 'true',
        ),
      ),
    ],
  );
});

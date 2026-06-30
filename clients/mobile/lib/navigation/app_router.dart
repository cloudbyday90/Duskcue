import 'package:duskcue_mobile/screens/dashboard_screen.dart';
import 'package:duskcue_mobile/screens/server_selection_screen.dart';
import 'package:duskcue_mobile/screens/settings_screen.dart';
import 'package:go_router/go_router.dart';

final GoRouter appRouter = GoRouter(
  initialLocation: '/server',
  routes: [
    GoRoute(
      path: '/server',
      builder: (context, state) => const ServerSelectionScreen(),
    ),
    GoRoute(
      path: '/dashboard',
      builder: (context, state) => const DashboardScreen(),
    ),
    GoRoute(
      path: '/settings',
      builder: (context, state) => const SettingsScreen(),
    ),
  ],
);

import 'package:duskcue_mobile/api/client_error.dart';
import 'package:duskcue_mobile/l10n/app_strings.dart';
import 'package:flutter/material.dart';

String userFacingError(BuildContext context, Object error) {
  final strings = AppStrings.of(context);
  if (error is ClientError) {
    return switch (error.kind) {
      ClientErrorKind.network || ClientErrorKind.serverUnavailable => strings.serverUnavailable,
      ClientErrorKind.authExpired => strings.signedOut,
      _ => error.problem.detail,
    };
  }
  return strings.serverUnavailable;
}

class EmptyState extends StatelessWidget {
  const EmptyState({
    required this.icon,
    required this.message,
    super.key,
  });

  final IconData icon;
  final String message;

  @override
  Widget build(BuildContext context) {
    return Center(
      child: Padding(
        padding: const EdgeInsets.all(32),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            Icon(icon, size: 48, color: Theme.of(context).colorScheme.outline),
            const SizedBox(height: 12),
            Text(message, textAlign: TextAlign.center),
          ],
        ),
      ),
    );
  }
}

class ErrorState extends StatelessWidget {
  const ErrorState({
    required this.message,
    required this.onRetry,
    super.key,
  });

  final String message;
  final VoidCallback onRetry;

  @override
  Widget build(BuildContext context) {
    final strings = AppStrings.of(context);
    return Center(
      child: Padding(
        padding: const EdgeInsets.all(32),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            Icon(Icons.cloud_off_outlined, size: 48, color: Theme.of(context).colorScheme.error),
            const SizedBox(height: 12),
            Text(message, textAlign: TextAlign.center),
            const SizedBox(height: 16),
            FilledButton(onPressed: onRetry, child: Text(strings.retry)),
          ],
        ),
      ),
    );
  }
}

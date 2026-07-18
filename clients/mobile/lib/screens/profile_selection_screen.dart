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

import 'package:duskcue_mobile/api/client_error.dart';
import 'package:duskcue_mobile/models/profile_models.dart';
import 'package:duskcue_mobile/services/service_providers.dart';
import 'package:duskcue_mobile/stores/download_manager_store.dart';
import 'package:duskcue_mobile/stores/session_store.dart';
import 'package:flutter/material.dart';
import 'package:flutter_cache_manager/flutter_cache_manager.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';

class ProfileSelectionScreen extends ConsumerStatefulWidget {
  const ProfileSelectionScreen({required this.allowManualSelection, super.key});

  final bool allowManualSelection;

  @override
  ConsumerState<ProfileSelectionScreen> createState() =>
      _ProfileSelectionScreenState();
}

class _ProfileSelectionScreenState
    extends ConsumerState<ProfileSelectionScreen> {
  ProfileListResponse? _response;
  bool _loading = true;
  bool _switching = false;
  bool _rememberOnDevice = false;
  String? _error;

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addPostFrameCallback((_) => _load());
  }

  Future<void> _load() async {
    if (!mounted) return;
    setState(() {
      _loading = true;
      _error = null;
    });
    try {
      final response = await ref.read(profileServiceProvider).listProfiles();
      final currentUser = ref.read(sessionProvider).user;
      if (currentUser != null) {
        final updated = currentUser.copyWith(
          activeProfileId: response.activeProfileId,
          profileSelectionRequired: response.profileSelectionRequired,
        );
        try {
          await ref.read(authServiceProvider).updateStoredUser(updated);
        } catch (_) {}
      }
      if (!mounted) return;
      ref
          .read(sessionProvider.notifier)
          .resolveProfileScope(
            activeProfileId: response.activeProfileId,
            profileSelectionRequired: response.profileSelectionRequired,
          );
      setState(() {
        _response = response;
        _rememberOnDevice =
            response.rememberedProfileId == response.activeProfileId;
        _loading = false;
      });
    } catch (error) {
      if (!mounted) return;
      setState(() {
        _loading = false;
        _error = _errorMessage(
          error,
          fallback: 'Profiles could not be loaded.',
        );
      });
    }
  }

  Future<void> _selectProfile(ProfileSummary profile) async {
    final response = _response;
    if (response == null || _switching) return;
    if (!response.profileSelectionRequired &&
        response.activeProfileId == profile.id) {
      return;
    }
    if (response.parentUnlockRequired && !profile.isKids) {
      final unlocked = await _requestParentUnlock();
      if (!unlocked) return;
    }
    if (!mounted) return;
    setState(() {
      _switching = true;
      _error = null;
    });
    try {
      final switched = await ref
          .read(profileServiceProvider)
          .switchProfile(
            profile.id,
            rememberOnDevice: response.deviceCanRememberProfile
                ? _rememberOnDevice
                : null,
          );
      await _clearProfileScopedCaches();
      final currentUser = ref.read(sessionProvider).user;
      if (currentUser != null) {
        final updated = currentUser.copyWith(
          activeProfileId: switched.activeProfile.id,
          profileSelectionRequired: switched.profileSelectionRequired,
        );
        try {
          await ref.read(authServiceProvider).updateStoredUser(updated);
        } catch (_) {}
      }
      if (!mounted) return;
      ref
          .read(sessionProvider.notifier)
          .resolveProfileScope(
            activeProfileId: switched.activeProfile.id,
            profileSelectionRequired: switched.profileSelectionRequired,
          );
      setState(() {
        _response = ProfileListResponse(
          activeProfileId: switched.activeProfile.id,
          profileSelectionRequired: switched.profileSelectionRequired,
          rememberedProfileId: switched.rememberedProfileId,
          deviceCanRememberProfile: switched.deviceCanRememberProfile,
          parentUnlockRequired: switched.parentUnlockRequired,
          items: response.items
              .map(
                (item) => item.id == switched.activeProfile.id
                    ? switched.activeProfile
                    : item,
              )
              .toList(growable: false),
        );
        _rememberOnDevice =
            switched.rememberedProfileId == switched.activeProfile.id;
        _switching = false;
      });
      if (mounted && !switched.profileSelectionRequired) {
        context.go('/dashboard');
      }
    } catch (error) {
      if (!mounted) return;
      setState(() {
        _switching = false;
        _error = _errorMessage(error, fallback: 'Could not switch profiles.');
      });
    }
  }

  Future<void> _updateRememberPreference(bool value) async {
    final response = _response;
    final activeProfile = response?.activeProfile;
    if (response == null || activeProfile == null || _switching) return;
    if (response.profileSelectionRequired) {
      setState(() => _rememberOnDevice = value);
      return;
    }
    setState(() {
      _switching = true;
      _error = null;
      _rememberOnDevice = value;
    });
    try {
      final switched = await ref
          .read(profileServiceProvider)
          .switchProfile(activeProfile.id, rememberOnDevice: value);
      if (!mounted) return;
      setState(() {
        _response = ProfileListResponse(
          activeProfileId: switched.activeProfile.id,
          profileSelectionRequired: switched.profileSelectionRequired,
          rememberedProfileId: switched.rememberedProfileId,
          deviceCanRememberProfile: switched.deviceCanRememberProfile,
          parentUnlockRequired: switched.parentUnlockRequired,
          items: response.items
              .map(
                (item) => item.id == switched.activeProfile.id
                    ? switched.activeProfile
                    : item,
              )
              .toList(growable: false),
        );
        _rememberOnDevice =
            switched.rememberedProfileId == switched.activeProfile.id;
        _switching = false;
      });
    } catch (error) {
      if (!mounted) return;
      setState(() {
        _switching = false;
        _rememberOnDevice =
            response.rememberedProfileId == response.activeProfileId;
        _error = _errorMessage(
          error,
          fallback: 'Could not update this device preference.',
        );
      });
    }
  }

  Future<bool> _requestParentUnlock() async {
    final controller = TextEditingController();
    var unlocked = false;
    await showDialog<void>(
      context: context,
      barrierDismissible: false,
      builder: (dialogContext) {
        var submitting = false;
        String? error;
        return StatefulBuilder(
          builder: (context, setDialogState) {
            Future<void> submit() async {
              if (submitting || controller.text.isEmpty) return;
              setDialogState(() {
                submitting = true;
                error = null;
              });
              try {
                await ref
                    .read(profileServiceProvider)
                    .unlockParentProfile(controller.text);
                unlocked = true;
                if (dialogContext.mounted) Navigator.of(dialogContext).pop();
              } catch (failure) {
                if (dialogContext.mounted) {
                  setDialogState(() {
                    submitting = false;
                    error = _errorMessage(
                      failure,
                      fallback: 'Parent PIN could not be verified.',
                    );
                  });
                }
              }
            }

            return AlertDialog(
              title: const Text('Parent approval required'),
              content: Column(
                mainAxisSize: MainAxisSize.min,
                children: [
                  const Text(
                    'Enter the parent PIN to switch from the Kids profile.',
                  ),
                  const SizedBox(height: 16),
                  TextField(
                    controller: controller,
                    autofocus: true,
                    obscureText: true,
                    enableSuggestions: false,
                    autocorrect: false,
                    keyboardType: TextInputType.number,
                    textInputAction: TextInputAction.done,
                    onSubmitted: (_) => submit(),
                    decoration: const InputDecoration(
                      labelText: 'Parent PIN',
                      border: OutlineInputBorder(),
                    ),
                  ),
                  if (error != null) ...[
                    const SizedBox(height: 12),
                    Text(
                      error!,
                      style: TextStyle(
                        color: Theme.of(context).colorScheme.error,
                      ),
                    ),
                  ],
                ],
              ),
              actions: [
                TextButton(
                  onPressed: submitting
                      ? null
                      : () => Navigator.of(dialogContext).pop(),
                  child: const Text('Cancel'),
                ),
                FilledButton(
                  onPressed: submitting ? null : submit,
                  child: Text(submitting ? 'Checking…' : 'Continue'),
                ),
              ],
            );
          },
        );
      },
    );
    controller.clear();
    controller.dispose();
    return unlocked;
  }

  Future<void> _clearProfileScopedCaches() async {
    try {
      await ref.read(ambientPlaybackServiceProvider).clear();
    } catch (_) {}
    ref.read(downloadManagerProvider.notifier).clearForProfileChange();
    PaintingBinding.instance.imageCache.clear();
    PaintingBinding.instance.imageCache.clearLiveImages();
    try {
      await DefaultCacheManager().emptyCache();
    } catch (_) {}
  }

  Future<void> _signOut() async {
    await _clearProfileScopedCaches();
    await ref.read(authServiceProvider).logout();
    ref.read(sessionProvider.notifier).clearAuthentication();
    if (mounted) {
      context.go('/auth');
    }
  }

  String _errorMessage(Object error, {required String fallback}) {
    if (error is ClientError && error.problem.detail.isNotEmpty) {
      return error.problem.detail;
    }
    final value = error.toString();
    return value.isEmpty ? fallback : value;
  }

  @override
  Widget build(BuildContext context) {
    final response = _response;
    final selectionRequired = response?.profileSelectionRequired ?? true;
    final title = widget.allowManualSelection && !selectionRequired
        ? 'Switch profile'
        : 'Who’s watching?';
    final theme = Theme.of(context);

    return Scaffold(
      appBar: widget.allowManualSelection && !selectionRequired
          ? AppBar(
              title: const Text('Switch profile'),
              leading: IconButton(
                tooltip: 'Back',
                onPressed: _switching ? null : () => context.go('/settings'),
                icon: const Icon(Icons.arrow_back),
              ),
            )
          : null,
      body: SafeArea(
        child: Center(
          child: ConstrainedBox(
            constraints: const BoxConstraints(maxWidth: 560),
            child: ListView(
              padding: const EdgeInsets.all(24),
              children: [
                if (!widget.allowManualSelection || selectionRequired) ...[
                  const SizedBox(height: 48),
                  Icon(
                    Icons.people_alt_outlined,
                    size: 56,
                    color: theme.colorScheme.primary,
                  ),
                  const SizedBox(height: 20),
                ],
                Text(
                  title,
                  style: theme.textTheme.headlineMedium,
                  textAlign: TextAlign.center,
                ),
                const SizedBox(height: 8),
                Text(
                  selectionRequired
                      ? 'Choose a profile before viewing personalized rows, playback state, and downloads on this device.'
                      : 'Choose the household profile for this device.',
                  style: theme.textTheme.bodyLarge,
                  textAlign: TextAlign.center,
                ),
                const SizedBox(height: 24),
                if (_loading)
                  const Center(
                    child: Padding(
                      padding: EdgeInsets.all(24),
                      child: CircularProgressIndicator(),
                    ),
                  )
                else if (response == null)
                  _UnavailableProfiles(
                    error: _error,
                    onRetry: _load,
                    onSignOut: _signOut,
                  )
                else ...[
                  for (final profile in response.items)
                    Card(
                      child: ListTile(
                        enabled: !_switching,
                        leading: CircleAvatar(
                          child: Text(
                            profile.name.isEmpty
                                ? 'P'
                                : profile.name.substring(0, 1).toUpperCase(),
                          ),
                        ),
                        title: Text(profile.name),
                        subtitle: Text(
                          profile.isKids ? 'Kids profile' : 'Standard profile',
                        ),
                        trailing:
                            response.activeProfileId == profile.id &&
                                !selectionRequired
                            ? const Icon(
                                Icons.check_circle,
                                semanticLabel: 'Active profile',
                              )
                            : const Icon(Icons.chevron_right),
                        onTap: () => _selectProfile(profile),
                      ),
                    ),
                  if (response.items.isEmpty)
                    const Padding(
                      padding: EdgeInsets.all(16),
                      child: Text(
                        'No profiles are available for this account.',
                        textAlign: TextAlign.center,
                      ),
                    ),
                  if (response.deviceCanRememberProfile &&
                      response.activeProfile != null)
                    CheckboxListTile(
                      contentPadding: EdgeInsets.zero,
                      value: _rememberOnDevice,
                      onChanged: _switching
                          ? null
                          : (value) =>
                                _updateRememberPreference(value ?? false),
                      title: const Text('Remember this profile on this device'),
                      subtitle: const Text(
                        'This device can open with the selected household profile.',
                      ),
                      controlAffinity: ListTileControlAffinity.leading,
                    ),
                  if (_error != null) ...[
                    const SizedBox(height: 8),
                    Text(
                      _error!,
                      style: TextStyle(color: theme.colorScheme.error),
                      textAlign: TextAlign.center,
                    ),
                  ],
                  if (selectionRequired) ...[
                    const SizedBox(height: 16),
                    OutlinedButton.icon(
                      onPressed: _switching ? null : _signOut,
                      icon: const Icon(Icons.logout),
                      label: const Text('Sign out'),
                    ),
                  ],
                ],
              ],
            ),
          ),
        ),
      ),
    );
  }
}

class _UnavailableProfiles extends StatelessWidget {
  const _UnavailableProfiles({
    required this.error,
    required this.onRetry,
    required this.onSignOut,
  });

  final String? error;
  final Future<void> Function() onRetry;
  final Future<void> Function() onSignOut;

  @override
  Widget build(BuildContext context) {
    return Column(
      children: [
        Text(error ?? 'Profiles are unavailable.', textAlign: TextAlign.center),
        const SizedBox(height: 16),
        FilledButton(onPressed: onRetry, child: const Text('Try again')),
        const SizedBox(height: 8),
        OutlinedButton(onPressed: onSignOut, child: const Text('Sign out')),
      ],
    );
  }
}

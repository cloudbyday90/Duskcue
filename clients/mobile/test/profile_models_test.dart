import 'package:duskcue_mobile/models/profile_models.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('profile list preserves selection and parent-unlock policy', () {
    final response = ProfileListResponse.fromJson({
      'active_profile_id': 'profile-kids',
      'profile_selection_required': true,
      'remembered_profile_id': null,
      'device_can_remember_profile': true,
      'parent_unlock_required': true,
      'items': [
        {
          'id': 'profile-kids',
          'name': 'Kids',
          'profile_type': 'kids',
          'is_default': false,
          'parent_pin_configured': true,
        },
        {
          'id': 'profile-parent',
          'name': 'Parent',
          'profile_type': 'standard',
          'is_default': true,
          'parent_pin_configured': false,
        },
      ],
    });

    expect(response.profileSelectionRequired, isTrue);
    expect(response.deviceCanRememberProfile, isTrue);
    expect(response.parentUnlockRequired, isTrue);
    expect(response.activeProfile?.isKids, isTrue);
    expect(response.items.last.isKids, isFalse);
  });

  test('switch response contains no parent PIN value', () {
    final response = SwitchProfileResponse.fromJson({
      'active_profile': {
        'id': 'profile-parent',
        'name': 'Parent',
        'profile_type': 'standard',
        'is_default': true,
        'parent_pin_configured': false,
      },
      'profile_selection_required': false,
      'remembered_profile_id': 'profile-parent',
      'device_can_remember_profile': true,
      'parent_unlock_required': false,
    });

    expect(response.activeProfile.id, 'profile-parent');
    expect(response.rememberedProfileId, 'profile-parent');
    expect(response.parentUnlockRequired, isFalse);
  });
}

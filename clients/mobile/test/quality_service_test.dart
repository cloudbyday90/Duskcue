import 'package:duskcue_mobile/services/quality_service.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('manual quality selections include bitrate cap in playback payload', () {
    const selection = QualitySelection(mode: QualityMode.manual1080p);

    expect(selection.toPlaybackJson(), {
      'quality_mode': 'manual',
      'max_streaming_bitrate': 6000000,
    });
  });

  test('auto quality selection does not send a manual bitrate cap', () {
    const selection = QualitySelection(mode: QualityMode.auto);

    expect(selection.toPlaybackJson(), {'quality_mode': 'auto'});
  });

  test('unknown saved quality mode falls back to auto', () {
    expect(QualityMode.fromName('removed-mode'), QualityMode.auto);
    expect(QualityMode.fromName(null), QualityMode.auto);
  });
}

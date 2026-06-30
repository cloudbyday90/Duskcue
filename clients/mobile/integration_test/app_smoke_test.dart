import 'package:duskcue_mobile/app.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();

  testWidgets('starts at server selection', (tester) async {
    await tester.pumpWidget(const ProviderScope(child: DuskcueApp()));

    expect(find.text('Duskcue'), findsWidgets);
    expect(find.text('Server URL'), findsOneWidget);
  });
}

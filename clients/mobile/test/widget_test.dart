import 'package:duskcue_mobile/app.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

void main() {
  testWidgets('renders server selection', (tester) async {
    await tester.pumpWidget(const ProviderScope(child: DuskcueApp()));

    expect(find.text('Server URL'), findsOneWidget);
    expect(find.text('Network mode'), findsOneWidget);
    expect(find.text('Test and continue'), findsOneWidget);
  });
}

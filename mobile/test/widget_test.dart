import 'package:flutter_test/flutter_test.dart';
import 'package:conduit_mobile/main.dart';
import 'package:shared_preferences/shared_preferences.dart';

void main() {
  setUp(() {
    SharedPreferences.setMockInitialValues({});
  });

  testWidgets('Conduit Mobile smoke test', (WidgetTester tester) async {
    await tester.pumpWidget(const ConduitApp());
    await tester.pumpAndSettle();
    expect(find.text('Conduit Mobile'), findsWidgets);
  });
}

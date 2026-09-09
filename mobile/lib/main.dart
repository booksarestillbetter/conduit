import 'dart:io';
import 'package:flutter/material.dart';
import 'package:provider/provider.dart';
import 'core/theme/conduit_theme.dart';
import 'providers/auth_provider.dart';
import 'providers/telemetry_provider.dart';
import 'providers/torrents_provider.dart';
import 'providers/nodes_provider.dart';
import 'ui/screens/setup_wizard_screen.dart';
import 'ui/screens/main_navigation_screen.dart';

class ConduitHttpOverrides extends HttpOverrides {
  @override
  HttpClient createHttpClient(SecurityContext? context) {
    return super.createHttpClient(context)
      ..badCertificateCallback = (X509Certificate cert, String host, int port) => true;
  }
}

void main() {
  HttpOverrides.global = ConduitHttpOverrides();
  WidgetsFlutterBinding.ensureInitialized();
  runApp(const ConduitApp());
}

class ConduitApp extends StatelessWidget {
  const ConduitApp({super.key});

  @override
  Widget build(BuildContext context) {
    return MultiProvider(
      providers: [
        ChangeNotifierProvider(create: (_) => AuthProvider()..checkAuth()),
        ChangeNotifierProvider(create: (_) => TelemetryProvider()),
        ChangeNotifierProvider(create: (_) => TorrentsProvider()),
        ChangeNotifierProvider(create: (_) => NodesProvider()),
      ],
      child: MaterialApp(
        title: 'Conduit Mobile',
        debugShowCheckedModeBanner: false,
        theme: ConduitTheme.darkTheme,
        home: const AuthGate(),
      ),
    );
  }
}

class AuthGate extends StatelessWidget {
  const AuthGate({super.key});

  @override
  Widget build(BuildContext context) {
    final auth = context.watch<AuthProvider>();

    if (auth.status == AuthStatus.unknown) {
      return const Scaffold(
        body: Center(
          child: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              Text('🐕', style: TextStyle(fontSize: 40)),
              SizedBox(height: 16),
              CircularProgressIndicator(),
            ],
          ),
        ),
      );
    }

    if (auth.isAuthenticated && auth.isSetupCompleted) {
      return const MainNavigationScreen();
    }

    return const SetupWizardScreen();
  }
}

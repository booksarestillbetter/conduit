import 'package:flutter/material.dart';
import 'package:provider/provider.dart';
import '../../core/constants/app_colors.dart';
import '../../providers/telemetry_provider.dart';
import '../../providers/torrents_provider.dart';
import '../../providers/nodes_provider.dart';
import 'dashboard_screen.dart';
import 'pipeline_screen.dart';
import 'nodes_screen.dart';
import 'settings_screen.dart';

class MainNavigationScreen extends StatefulWidget {
  const MainNavigationScreen({super.key});

  @override
  State<MainNavigationScreen> createState() => _MainNavigationScreenState();
}

class _MainNavigationScreenState extends State<MainNavigationScreen> {
  int _currentIndex = 0;

  final List<Widget> _screens = const [
    DashboardScreen(),
    PipelineScreen(),
    NodesScreen(),
    SettingsScreen(),
  ];

  @override
  void initState() {
    super.initState();
    // Start live background stream & polling
    WidgetsBinding.instance.addPostFrameCallback((_) {
      context.read<TelemetryProvider>().startListening();
      context.read<TorrentsProvider>().startPolling();
      context.read<NodesProvider>().startRefreshing();
    });
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      body: IndexedStack(
        index: _currentIndex,
        children: _screens,
      ),
      bottomNavigationBar: Container(
        decoration: const BoxDecoration(
          border: Border(top: BorderSide(color: ConduitColors.border)),
        ),
        child: BottomNavigationBar(
          currentIndex: _currentIndex,
          onTap: (index) {
            setState(() {
              _currentIndex = index;
            });
          },
          items: const [
            BottomNavigationBarItem(
              icon: Icon(Icons.dashboard_outlined),
              activeIcon: Icon(Icons.dashboard),
              label: 'Command',
            ),
            BottomNavigationBarItem(
              icon: Icon(Icons.pets_outlined),
              activeIcon: Icon(Icons.pets),
              label: 'Pipeline',
            ),
            BottomNavigationBarItem(
              icon: Icon(Icons.dns_outlined),
              activeIcon: Icon(Icons.dns),
              label: 'Nodes',
            ),
            BottomNavigationBarItem(
              icon: Icon(Icons.settings_outlined),
              activeIcon: Icon(Icons.settings),
              label: 'Settings',
            ),
          ],
        ),
      ),
    );
  }
}
